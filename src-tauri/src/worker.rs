use crate::types::{EngineConfig, PdmResult, Event};
use crate::engine::EngineHooks;
use crate::network::pool::NetworkPool;
use crate::network::limiter::MultiLimiter;
use crate::engine;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, Semaphore};

/// Outcome of submitting a download to the pool.
#[derive(Debug, PartialEq, Eq)]
pub enum Admission {
    /// A slot was free — the engine is running.
    Started,
    /// All slots busy — parked FIFO; starts automatically when a slot frees.
    Queued,
}

struct PendingDownload {
    cfg: EngineConfig,
    id: u64,
    hooks: EngineHooks,
}

struct ActiveDownload {
    cancel: Arc<AtomicBool>,
    handle: tokio::task::JoinHandle<()>,
    limiter: Arc<MultiLimiter>,
    desired_connections: Arc<AtomicU32>,
}

type ActiveMap = Arc<Mutex<HashMap<u64, ActiveDownload>>>;

/// Everything a running task needs to clean up after itself and hand its
/// permit to the next queued download.
#[derive(Clone)]
struct SpawnCtx {
    pool: Arc<NetworkPool>,
    event_tx: mpsc::UnboundedSender<Event>,
    active: ActiveMap,
    pending: Arc<Mutex<VecDeque<PendingDownload>>>,
    global_limiter: Arc<crate::network::limiter::RateLimiter>,
}

pub struct WorkerPool {
    semaphore: Arc<Semaphore>,
    pool: Arc<NetworkPool>,
    event_tx: mpsc::UnboundedSender<Event>,
    active: ActiveMap,
    pending: Arc<Mutex<VecDeque<PendingDownload>>>,
    next_id: AtomicU64,
    global_limiter: Arc<crate::network::limiter::RateLimiter>,
}

impl WorkerPool {
    pub fn new(max_workers: u32, event_tx: mpsc::UnboundedSender<Event>, danger_accept_invalid_certs: bool, next_id_start: u64, global_rate_limit: u64) -> Self {
        log::info!("WorkerPool starting next_id from {}", next_id_start);
        Self {
            semaphore: Arc::new(Semaphore::new(max_workers as usize)),
            pool: Arc::new(NetworkPool::new(danger_accept_invalid_certs)),
            event_tx,
            active: Arc::new(Mutex::new(HashMap::new())),
            pending: Arc::new(Mutex::new(VecDeque::new())),
            next_id: AtomicU64::new(next_id_start),
            global_limiter: Arc::new(crate::network::limiter::RateLimiter::new(global_rate_limit)),
        }
    }

    pub fn next_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    fn ctx(&self) -> SpawnCtx {
        SpawnCtx {
            pool: self.pool.clone(),
            event_tx: self.event_tx.clone(),
            active: self.active.clone(),
            pending: self.pending.clone(),
            global_limiter: self.global_limiter.clone(),
        }
    }

    /// Submit a download: runs now if a slot is free, otherwise parks it
    /// (Queued 状态机 admission). Idempotent for an id that is already parked.
    pub async fn add_with_id(&self, cfg: EngineConfig, id: u64, hooks: EngineHooks) -> PdmResult<Admission> {
        {
            let pending = self.pending.lock().await;
            if pending.iter().any(|p| p.id == id) {
                return Ok(Admission::Queued);
            }
        }
        if self.active.lock().await.contains_key(&id) {
            return Err(crate::types::PdmError::Other(format!(
                "Download {} is already running",
                id
            )));
        }
        match self.semaphore.clone().try_acquire_owned() {
            Ok(permit) => {
                let active = Self::launch(self.ctx(), cfg, permit, id, hooks);
                self.active.lock().await.insert(id, active);
                Ok(Admission::Started)
            }
            Err(_) => {
                log::info!("[ProxyDM] id={} queued (all slots busy)", id);
                self.pending.lock().await.push_back(PendingDownload { cfg, id, hooks });
                Ok(Admission::Queued)
            }
        }
    }

    /// Spawn the engine task for one download. At task end the permit is
    /// handed directly to the next queued download (FIFO, no release/acquire
    /// race) or dropped when the queue is empty.
    fn launch(
        ctx: SpawnCtx,
        mut cfg: EngineConfig,
        permit: tokio::sync::OwnedSemaphorePermit,
        id: u64,
        hooks: EngineHooks,
    ) -> ActiveDownload {
        cfg.id = id;
        log::info!("[ProxyDM] spawn id={} url={} proxy={} conns={}",
            id, cfg.url, crate::headers::redact_log(&cfg.proxy_url), cfg.connections);
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_for_task = cancel.clone();
        let desired = cfg
            .desired_connections
            .clone()
            .unwrap_or_else(|| Arc::new(AtomicU32::new(cfg.connections.max(1))));
        cfg.desired_connections = Some(desired.clone());
        let limiter = Arc::new(MultiLimiter::with_global(
            ctx.global_limiter.clone(),
            cfg.rate_limit_bps,
        ));
        let limiter_for_task = limiter.clone();

        let handle = tokio::spawn(async move {
            let result = engine::run_download(
                cfg,
                ctx.pool.clone(),
                ctx.event_tx.clone(),
                limiter_for_task,
                cancel_for_task.clone(),
                hooks,
            )
            .await;

            match &result {
                Ok(_) => log::info!("[ProxyDM] id={} completed OK", id),
                Err(e) => {
                    log::error!("[ProxyDM] id={} ERROR: {}", id, e);
                    if !matches!(e, crate::types::PdmError::Cancelled) {
                        let _ = ctx.event_tx.send(Event {
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
                let mut active = ctx.active.lock().await;
                if let Some(entry) = active.get(&id) {
                    if Arc::ptr_eq(&entry.cancel, &cancel_for_task) {
                        active.remove(&id);
                    }
                }
                log::info!("[ProxyDM] id={} cleaned up, {} active remaining", id, active.len());
            }

            // FIFO handoff to the next queued download.
            let next = ctx.pending.lock().await.pop_front();
            match next {
                Some(p) => {
                    log::info!("[ProxyDM] slot handoff → queued id={}", p.id);
                    let ctx_next = ctx.clone();
                    let next_active =
                        Self::launch(ctx_next.clone(), p.cfg, permit, p.id, p.hooks);
                    ctx_next.active.lock().await.insert(p.id, next_active);
                }
                None => drop(permit),
            }
        });
        ActiveDownload {
            cancel,
            handle,
            limiter,
            desired_connections: desired,
        }
    }

    /// Cancel a download by setting its cancel flag and removing it from the active map.
    /// A download still waiting in the queue is simply forgotten.
    /// Returns the JoinHandle so the caller can optionally await task completion.
    pub async fn cancel(&self, id: u64) -> Option<tokio::task::JoinHandle<()>> {
        if self.remove_pending(id).await {
            return None;
        }
        let mut active = self.active.lock().await;
        if let Some(entry) = active.remove(&id) {
            log::info!("[ProxyDM] cancel id={} (flag set)", id);
            entry.cancel.store(true, Ordering::Relaxed);
            Some(entry.handle)
        } else {
            log::info!("[ProxyDM] cancel id={} (not found, already done?)", id);
            None
        }
    }

    /// Cancel a download and wait for the task to fully stop.
    /// Use this when you need the worker to be completely done before proceeding
    /// (e.g. pause_download needs to flush progress before updating DB status).
    pub async fn cancel_and_wait(&self, id: u64) {
        if self.remove_pending(id).await {
            return;
        }
        let handle = {
            let mut active = self.active.lock().await;
            if let Some(entry) = active.remove(&id) {
                log::info!("[ProxyDM] cancel_and_wait id={} (flag set, waiting)", id);
                entry.cancel.store(true, Ordering::Relaxed);
                Some(entry.handle)
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

    async fn remove_pending(&self, id: u64) -> bool {
        let mut pending = self.pending.lock().await;
        if let Some(pos) = pending.iter().position(|p| p.id == id) {
            pending.remove(pos);
            log::info!("[ProxyDM] id={} removed from queue", id);
            true
        } else {
            false
        }
    }

    pub fn pool_ref(&self) -> Arc<NetworkPool> {
        self.pool.clone()
    }

    pub fn clear_clients(&self) {
        self.pool.clear();
    }

    pub fn set_global_rate_limit(&self, bps: u64) {
        self.global_limiter.set_bps(bps);
    }

    pub async fn set_download_rate_limit(&self, id: u64, bps: u64) -> bool {
        let active = self.active.lock().await;
        if let Some(entry) = active.get(&id) {
            entry.limiter.set_download_bps(bps);
            true
        } else {
            false
        }
    }

    pub async fn set_connections(&self, id: u64, connections: u32) -> bool {
        let n = connections.clamp(1, crate::engine::chunk::MAX_CONNECTIONS);
        let active = self.active.lock().await;
        if let Some(entry) = active.get(&id) {
            entry.desired_connections.store(n, Ordering::Relaxed);
            true
        } else {
            false
        }
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
            proxy_name: String::new(),
            total_size: 100,
            supports_range: false,
            rate_limit_bps: 0,
            connections: 1,
            max_retries: 0,
            user_agent: "test".to_string(),
            resume_tasks: vec![],
            downloaded: 0,
            part_ranges: vec![(0, 100)],
            part_downloaded: vec![],
            desired_connections: None,
        }
    }

    fn hooks() -> EngineHooks {
        EngineHooks {
            save_resume_state: Box::new(|_, _| {}),
            invalidate_for_restart: Box::new(|_| {}),
        }
    }

    /// Server that accepts, stalls ~400ms, then closes without responding —
    /// keeps one slot busy long enough to observe queueing.
    async fn spawn_slow_server() -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        tokio::spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
                            drop(stream);
                        });
                    }
                    Err(_) => return,
                }
            }
        });
        format!("http://127.0.0.1:{}/slow.bin", addr.port())
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
        let _ = pool.add_with_id(test_config(), id, hooks()).await;

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
        let _ = pool.add_with_id(test_config(), id, hooks()).await;

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
        let _ = pool.add_with_id(test_config(), id1, hooks()).await;
        let _ = pool.add_with_id(test_config(), id2, hooks()).await;

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Cancel both concurrently
        let (r1, r2) = tokio::join!(pool.cancel(id1), pool.cancel(id2));
        // Both should return without panic
        let _ = (r1, r2);

        // Wait for cleanup
        pool.cancel_and_wait(id1).await;
        pool.cancel_and_wait(id2).await;
    }

    #[tokio::test]
    async fn test_over_limit_queues_and_hands_off() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let pool = WorkerPool::new(1, tx, false, 1, 0);
        let slow_url = spawn_slow_server().await;

        let mut cfg1 = test_config();
        cfg1.url = slow_url;
        let id1 = pool.next_id();
        let a1 = pool.add_with_id(cfg1, id1, hooks()).await.unwrap();
        assert_eq!(a1, Admission::Started);

        // Slot is busy → second download parks.
        let id2 = pool.next_id();
        let a2 = pool.add_with_id(test_config(), id2, hooks()).await.unwrap();
        assert_eq!(a2, Admission::Queued);
        assert_eq!(pool.pending.lock().await.len(), 1);

        // Re-submitting a parked id is idempotent.
        let a2b = pool.add_with_id(test_config(), id2, hooks()).await.unwrap();
        assert_eq!(a2b, Admission::Queued);
        assert_eq!(pool.pending.lock().await.len(), 1);

        // When the slow download dies, the permit is handed to id2, which
        // fails fast (unreachable URL) — everything drains.
        for _ in 0..100 {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            if pool.pending.lock().await.is_empty() && pool.active.lock().await.is_empty() {
                break;
            }
        }
        assert!(pool.pending.lock().await.is_empty(), "queue never drained");
        assert!(pool.active.lock().await.is_empty(), "active never drained");
    }

    #[tokio::test]
    async fn test_cancel_removes_queued_download() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let pool = WorkerPool::new(1, tx, false, 1, 0);
        let slow_url = spawn_slow_server().await;

        let mut cfg1 = test_config();
        cfg1.url = slow_url;
        let id1 = pool.next_id();
        let _ = pool.add_with_id(cfg1, id1, hooks()).await.unwrap();

        let id2 = pool.next_id();
        let a2 = pool.add_with_id(test_config(), id2, hooks()).await.unwrap();
        assert_eq!(a2, Admission::Queued);

        // Cancelling a queued download just forgets it.
        let handle = pool.cancel(id2).await;
        assert!(handle.is_none());
        assert!(pool.pending.lock().await.is_empty());

        pool.cancel_and_wait(id1).await;
    }
}
