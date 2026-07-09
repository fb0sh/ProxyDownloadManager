use crate::types::{DownloadConfig, Event};
use crate::network::pool::NetworkPool;
use crate::network::limiter::MultiLimiter;
use crate::engine;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, Semaphore};

pub struct WorkerPool {
    semaphore: Arc<Semaphore>,
    pool: Arc<NetworkPool>,
    event_tx: mpsc::UnboundedSender<Event>,
    active: Arc<Mutex<HashMap<u64, (Arc<AtomicBool>, tokio::task::JoinHandle<()>)>>>,
    next_id: AtomicU64,
}

impl WorkerPool {
    pub fn new(max_workers: u32, event_tx: mpsc::UnboundedSender<Event>, danger_accept_invalid_certs: bool, next_id_start: u64) -> Self {
        eprintln!("[ProxyDM] WorkerPool starting next_id from {}", next_id_start);
        Self {
            semaphore: Arc::new(Semaphore::new(max_workers as usize)),
            pool: Arc::new(NetworkPool::new(danger_accept_invalid_certs)),
            event_tx,
            active: Arc::new(Mutex::new(HashMap::new())),
            next_id: AtomicU64::new(next_id_start),
        }
    }

    pub fn next_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    pub async fn add(&self, mut cfg: DownloadConfig) -> Result<u64, String> {
        let permit = self.semaphore.clone().try_acquire_owned().map_err(|_| "Too many concurrent downloads (max 8) — wait for one to finish first.".to_string())?;
        let id = self.next_id();
        cfg.id = id;
        self.spawn_task(cfg, permit, id).await;
        Ok(id)
    }

    pub async fn add_with_id(&self, cfg: DownloadConfig, id: u64) -> Result<u64, String> {
        let permit = self.semaphore.clone().try_acquire_owned().map_err(|_| "Too many concurrent downloads — try again later.".to_string())?;
        self.spawn_task(cfg, permit, id).await;
        Ok(id)
    }

    async fn spawn_task(&self, mut cfg: DownloadConfig, permit: tokio::sync::OwnedSemaphorePermit, id: u64) {
        cfg.id = id;
        eprintln!("[ProxyDM] worker spawn id={} url={} proxy={} conns={}",
            id, cfg.url, cfg.proxy_name, cfg.connections);
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_for_task = cancel.clone();
        let event_tx = self.event_tx.clone();
        let pool = self.pool.clone();
        let active_map = self.active.clone();

        let handle = tokio::spawn(async move {
            let limiter = Arc::new(MultiLimiter::new(
                0, // global rate limit handled elsewhere
                cfg.rate_limit_bps,
            ));

            let result = engine::run_download(cfg, pool, event_tx.clone(), limiter, cancel_for_task.clone()).await;

            match &result {
                Ok(_) => eprintln!("[ProxyDM] worker id={} completed OK", id),
                Err(e) => {
                    eprintln!("[ProxyDM] worker id={} ERROR: {}", id, e);
                    if e != "Cancelled" {
                        let _ = event_tx.send(Event {
                            kind: crate::types::EventKind::DownloadErrored,
                            download_id: id,
                            data: Some(e.clone()),
                        });
                    }
                }
            }

            // Cleanup: only remove if entry still belongs to this worker
            // (prevents a paused→resumed worker from removing the new worker's entry)
            {
                let mut active = active_map.lock().await;
                if let Some((entry_cancel, _)) = active.get(&id) {
                    if Arc::ptr_eq(entry_cancel, &cancel_for_task) {
                        active.remove(&id);
                    }
                }
                eprintln!("[ProxyDM] worker id={} cleaned up, {} active remaining", id, active.len());
            }
            drop(permit);
        });
        {
            let mut active = self.active.lock().await;
            active.insert(id, (cancel, handle));
        }
    }

    pub async fn cancel(&self, id: u64) {
        let mut active = self.active.lock().await;
        if let Some((cancel, _handle)) = active.remove(&id) {
            eprintln!("[ProxyDM] worker cancel id={} (flag set)", id);
            cancel.store(true, Ordering::Relaxed);
        } else {
            eprintln!("[ProxyDM] worker cancel id={} (not found, already done?)", id);
        }
    }

    pub async fn cancel_and_wait(&self, id: u64) {
        let handle = {
            let mut active = self.active.lock().await;
            if let Some((cancel, handle)) = active.remove(&id) {
                eprintln!("[ProxyDM] worker cancel_and_wait id={} (flag set, waiting)", id);
                cancel.store(true, Ordering::Relaxed);
                Some(handle)
            } else {
                eprintln!("[ProxyDM] worker cancel_and_wait id={} (not found, already done?)", id);
                None
            }
        };
        if let Some(handle) = handle {
            let _ = handle.await;
            eprintln!("[ProxyDM] worker cancel_and_wait id={} worker fully stopped", id);
        }
    }

    pub async fn active_count(&self) -> usize {
        self.active.lock().await.len()
    }

    pub fn pool_ref(&self) -> Arc<NetworkPool> {
        self.pool.clone()
    }

    pub fn clear_clients(&self) {
        self.pool.clear();
    }
}
