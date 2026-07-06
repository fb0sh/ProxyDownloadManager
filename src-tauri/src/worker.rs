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
    active: Arc<Mutex<HashMap<u64, Arc<AtomicBool>>>>,
    next_id: AtomicU64,
}

impl WorkerPool {
    pub fn new(max_workers: u32, event_tx: mpsc::UnboundedSender<Event>) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(max_workers as usize)),
            pool: Arc::new(NetworkPool::new()),
            event_tx,
            active: Arc::new(Mutex::new(HashMap::new())),
            next_id: AtomicU64::new(1),
        }
    }

    pub async fn add(&self, mut cfg: DownloadConfig) -> Result<u64, String> {
        let permit = self.semaphore.clone().acquire_owned().await.map_err(|e| e.to_string())?;
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        cfg.id = id;
        self.spawn_task(cfg, permit, id).await;
        Ok(id)
    }

    pub async fn add_with_id(&self, cfg: DownloadConfig, id: u64) -> Result<u64, String> {
        let permit = self.semaphore.clone().acquire_owned().await.map_err(|e| e.to_string())?;
        self.spawn_task(cfg, permit, id).await;
        Ok(id)
    }

    async fn spawn_task(&self, mut cfg: DownloadConfig, permit: tokio::sync::OwnedSemaphorePermit, id: u64) {
        cfg.id = id;
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut active = self.active.lock().await;
            active.insert(id, cancel.clone());
        }
        let event_tx = self.event_tx.clone();
        let pool = self.pool.clone();
        let active_map = self.active.clone();

        tokio::spawn(async move {
            let limiter = Arc::new(MultiLimiter::new(
                0, // global rate limit handled elsewhere
                cfg.rate_limit_bps,
            ));

            let result = engine::run_download(cfg, pool, event_tx.clone(), limiter, cancel.clone()).await;

            if let Err(e) = &result {
                let _ = event_tx.send(Event {
                    kind: crate::types::EventKind::DownloadErrored,
                    download_id: id,
                    data: Some(e.clone()),
                });
            }

            // Cleanup
            {
                let mut active = active_map.lock().await;
                active.remove(&id);
            }
            drop(permit);
        });
    }

    pub async fn cancel(&self, id: u64) {
        let mut active = self.active.lock().await;
        if let Some(cancel) = active.remove(&id) {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    pub async fn active_count(&self) -> usize {
        self.active.lock().await.len()
    }

    pub fn pool_ref(&self) -> Arc<NetworkPool> {
        self.pool.clone()
    }
}
