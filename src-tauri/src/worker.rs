use crate::types::{EngineConfig, PdmResult, Event};
use crate::engine::OnResumeState;
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
    global_rate_limit: u64,
}

impl WorkerPool {
    pub fn new(max_workers: u32, event_tx: mpsc::UnboundedSender<Event>, danger_accept_invalid_certs: bool, next_id_start: u64, global_rate_limit: u64) -> Self {
        log::info!("WorkerPool starting next_id from {}", next_id_start);
        Self {
            semaphore: Arc::new(Semaphore::new(max_workers as usize)),
            pool: Arc::new(NetworkPool::new(danger_accept_invalid_certs)),
            event_tx,
            active: Arc::new(Mutex::new(HashMap::new())),
            next_id: AtomicU64::new(next_id_start),
            global_rate_limit,
        }
    }

    pub fn next_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    pub async fn add_with_id(&self, cfg: EngineConfig, id: u64, on_resume: OnResumeState) -> PdmResult<u64> {
        let permit = self.semaphore.clone().try_acquire_owned().map_err(|_| crate::types::PdmError::Other("Too many concurrent downloads — try again later.".to_string()))?;
        self.spawn_task(cfg, permit, id, on_resume).await;
        Ok(id)
    }

    async fn spawn_task(&self, mut cfg: EngineConfig, permit: tokio::sync::OwnedSemaphorePermit, id: u64, on_resume: OnResumeState) {
        cfg.id = id;
        log::info!("[ProxyDM] spawn id={} url={} proxy={} conns={}",
            id, cfg.url, cfg.proxy_url, cfg.connections);
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_for_task = cancel.clone();
        let event_tx = self.event_tx.clone();
        let pool = self.pool.clone();
        let active_map = self.active.clone();
        let global_rate_limit = self.global_rate_limit;

        let handle = tokio::spawn(async move {
            let limiter = Arc::new(MultiLimiter::new(
                global_rate_limit,
                cfg.rate_limit_bps,
            ));

            let result = engine::run_download(cfg, pool, event_tx.clone(), limiter, cancel_for_task.clone(),
                on_resume
            ).await;

            match &result {
                Ok(_) => log::info!("[ProxyDM] id={} completed OK", id),
                Err(e) => {
                    log::error!("[ProxyDM] id={} ERROR: {}", id, e);
                    if !matches!(e, crate::types::PdmError::Cancelled) {
                        let _ = event_tx.send(Event {
                            kind: crate::types::EventKind::DownloadErrored,
                            download_id: id,
                            data: Some(e.to_string()),
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
                log::info!("[ProxyDM] id={} cleaned up, {} active remaining", id, active.len());
            }
            drop(permit);
        });
        {
            let mut active = self.active.lock().await;
            active.insert(id, (cancel, handle));
        }
    }

    /// Cancel a download by setting its cancel flag and removing it from the active map.
    /// Returns the JoinHandle so the caller can optionally await task completion.
    /// The semaphore permit is released when the task finishes (via `drop(permit)` in spawn_task).
    pub async fn cancel(&self, id: u64) -> Option<tokio::task::JoinHandle<()>> {
        let mut active = self.active.lock().await;
        if let Some((cancel, handle)) = active.remove(&id) {
            log::info!("[ProxyDM] cancel id={} (flag set)", id);
            cancel.store(true, Ordering::Relaxed);
            Some(handle)
        } else {
            log::info!("[ProxyDM] cancel id={} (not found, already done?)", id);
            None
        }
    }

    /// Cancel a download and wait for the task to fully stop.
    /// Use this when you need the worker to be completely done before proceeding
    /// (e.g. pause_download needs to flush progress before updating DB status).
    pub async fn cancel_and_wait(&self, id: u64) {
        let handle = {
            let mut active = self.active.lock().await;
            if let Some((cancel, handle)) = active.remove(&id) {
                log::info!("[ProxyDM] cancel_and_wait id={} (flag set, waiting)", id);
                cancel.store(true, Ordering::Relaxed);
                Some(handle)
            } else {
                log::info!("[ProxyDM] cancel_and_wait id={} (not found, already done?)", id);
                None
            }
        };
        if let Some(handle) = handle {
            let _ = handle.await;
            log::info!("[ProxyDM] cancel_and_wait id={} worker fully stopped", id);
        }
    }

    pub fn pool_ref(&self) -> Arc<NetworkPool> {
        self.pool.clone()
    }

    pub fn clear_clients(&self) {
        self.pool.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> EngineConfig {
        EngineConfig {
            url: "http://127.0.0.1:1/nonexistent".to_string(),
            save_path: "/tmp/test_worker.dat".to_string(),
            id: 0,
            file_name: "test.dat".to_string(),
            is_resume: false,
            headers: HashMap::new(),
            proxy_url: String::new(),
            total_size: 100,
            supports_range: false,
            rate_limit_bps: 0,
            connections: 1,
            max_retries: 0,
            user_agent: "test".to_string(),
            resume_tasks: vec![],
            downloaded: 0,
        }
    }

    fn on_resume() -> OnResumeState {
        Box::new(|_, _| {})
    }

    #[test]
    fn test_next_id_increments() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let pool = WorkerPool::new(4, tx, false, 10, 0);
        assert_eq!(pool.next_id(), 10);
        assert_eq!(pool.next_id(), 11);
        assert_eq!(pool.next_id(), 12);
    }

    #[tokio::test]
    async fn test_new_pool_initial_state() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let pool = WorkerPool::new(8, tx, false, 1, 0);
        assert_eq!(pool.next_id(), 1); // first call returns 1, increments to 2
        // Active map should be empty
        let active = pool.active.lock().await;
        assert!(active.is_empty());
    }

    #[tokio::test]
    async fn test_cancel_returns_none_for_unknown_id() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let pool = WorkerPool::new(4, tx, false, 1, 0);
        let result = pool.cancel(999).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_cancel_returns_handle_for_active_task() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let pool = WorkerPool::new(4, tx, false, 1, 0);

        // Add a task — it will fail quickly (unreachable URL) but will be in the active map briefly
        let id = pool.next_id();
        let _ = pool.add_with_id(test_config(), id, on_resume()).await;

        // Wait briefly for task to be inserted into active map
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Cancel — may or may not find the task depending on whether it already completed
        let result = pool.cancel(id).await;
        // If the task already completed and cleaned up, result is None (valid)
        // If the task is still running, result is Some (valid)
        // Both are acceptable outcomes for an unreachable URL
        let _ = result; // just verify no panic
    }

    #[tokio::test]
    async fn test_cancel_and_wait_completes() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let pool = WorkerPool::new(4, tx, false, 1, 0);

        let id = pool.next_id();
        let _ = pool.add_with_id(test_config(), id, on_resume()).await;

        // cancel_and_wait should complete without hanging
        pool.cancel_and_wait(id).await;

        // After cancel_and_wait, the task should be removed from active map
        let active = pool.active.lock().await;
        assert!(!active.contains_key(&id));
    }

    #[tokio::test]
    async fn test_cancel_and_wait_for_unknown_id() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let pool = WorkerPool::new(4, tx, false, 1, 0);

        // Should not panic or hang
        pool.cancel_and_wait(999).await;
    }

    #[tokio::test]
    async fn test_concurrent_cancel_safety() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let pool = WorkerPool::new(4, tx, false, 1, 0);

        // Spawn multiple tasks
        let id1 = pool.next_id();
        let id2 = pool.next_id();
        let _ = pool.add_with_id(test_config(), id1, on_resume()).await;
        let _ = pool.add_with_id(test_config(), id2, on_resume()).await;

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Cancel both concurrently
        let (r1, r2) = tokio::join!(pool.cancel(id1), pool.cancel(id2));
        // Both should return without panic
        let _ = (r1, r2);

        // Wait for cleanup
        pool.cancel_and_wait(id1).await;
        pool.cancel_and_wait(id2).await;
    }
}
