use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

struct Bucket {
    last_check: Instant,
    allowance: f64,
}

pub struct RateLimiter {
    bps: AtomicU64,
    bucket: Mutex<Bucket>,
}

impl RateLimiter {
    pub fn new(bps: u64) -> Self {
        Self {
            bps: AtomicU64::new(bps),
            bucket: Mutex::new(Bucket {
                last_check: Instant::now(),
                allowance: 0.0,
            }),
        }
    }

    pub fn set_bps(&self, bps: u64) {
        self.bps.store(bps, Ordering::Relaxed);
        if let Ok(mut b) = self.bucket.lock() {
            b.allowance = 0.0;
            b.last_check = Instant::now();
        }
    }

    pub fn bps(&self) -> u64 {
        self.bps.load(Ordering::Relaxed)
    }

    pub async fn wait_n(&self, n: u64) {
        let mut remaining = n as f64;
        while remaining > 0.0 {
            let bps = self.bps.load(Ordering::Relaxed);
            if bps == 0 {
                return;
            }
            let sleep_secs = {
                let mut bucket = match self.bucket.lock() {
                    Ok(g) => g,
                    Err(_) => return,
                };
                let now = Instant::now();
                let elapsed = now.duration_since(bucket.last_check).as_secs_f64();
                bucket.last_check = now;
                bucket.allowance += elapsed * (bps as f64);
                let cap = bps as f64 * 0.25;
                if bucket.allowance > cap {
                    bucket.allowance = cap;
                }
                let take = bucket.allowance.min(remaining);
                bucket.allowance -= take;
                remaining -= take;
                if remaining <= 0.0 {
                    return;
                }
                (remaining / (bps as f64)).clamp(0.001, 0.25)
            };
            tokio::time::sleep(Duration::from_secs_f64(sleep_secs)).await;
        }
    }
}

pub struct MultiLimiter {
    pub global: std::sync::Arc<RateLimiter>,
    pub per_download: std::sync::Arc<RateLimiter>,
}

impl MultiLimiter {
    pub fn new(global_bps: u64, download_bps: u64) -> Self {
        Self {
            global: std::sync::Arc::new(RateLimiter::new(global_bps)),
            per_download: std::sync::Arc::new(RateLimiter::new(download_bps)),
        }
    }

    pub fn with_global(global: std::sync::Arc<RateLimiter>, download_bps: u64) -> Self {
        Self {
            global,
            per_download: std::sync::Arc::new(RateLimiter::new(download_bps)),
        }
    }

    pub fn set_global_bps(&self, bps: u64) {
        self.global.set_bps(bps);
    }

    pub fn set_download_bps(&self, bps: u64) {
        self.per_download.set_bps(bps);
    }

    pub async fn wait_n(&self, n: u64) {
        self.global.wait_n(n).await;
        self.per_download.wait_n(n).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limiter_no_limit() {
        let limiter = RateLimiter::new(0);
        limiter.wait_n(10_000_000).await;
    }

    #[tokio::test]
    async fn test_multi_limiter_no_limit() {
        let limiter = MultiLimiter::new(0, 0);
        limiter.wait_n(1000).await;
    }

    #[tokio::test]
    async fn test_multi_limiter_partial_limit() {
        let limiter = MultiLimiter::new(100_000, 0);
        limiter.wait_n(1).await;
    }

    #[tokio::test]
    async fn test_set_bps_runtime() {
        let limiter = RateLimiter::new(0);
        limiter.set_bps(1024);
        assert_eq!(limiter.bps(), 1024);
        limiter.set_bps(0);
        limiter.wait_n(1_000_000).await;
    }

    #[tokio::test]
    async fn test_wait_n_respects_bps() {
        let limiter = RateLimiter::new(80_000);
        let start = Instant::now();
        limiter.wait_n(40_000).await;
        let elapsed = start.elapsed();
        assert!(
            elapsed >= Duration::from_millis(350),
            "expected throttle, got {:?}",
            elapsed
        );
    }
}
