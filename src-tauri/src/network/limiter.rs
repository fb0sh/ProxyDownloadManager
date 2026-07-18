use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use std::sync::Mutex;

pub struct RateLimiter {
    bps: AtomicU64,
    last_check: Mutex<Instant>,
    allowance: Mutex<f64>,
}

impl RateLimiter {
    pub fn new(bps: u64) -> Self {
        Self {
            bps: AtomicU64::new(bps),
            last_check: Mutex::new(Instant::now()),
            allowance: Mutex::new(0.0),
        }
    }

    pub fn wait_n(&self, n: u64) {
        let bps = self.bps.load(Ordering::Relaxed);
        if bps == 0 {
            return; // no limit
        }
        let mut allowance = self.allowance.lock().unwrap();
        let mut last_check = self.last_check.lock().unwrap();
        let now = Instant::now();
        let elapsed = now.duration_since(*last_check).as_secs_f64();
        *last_check = now;
        *allowance += elapsed * (bps as f64);
        if *allowance > (bps as f64) * 2.0 {
            *allowance = (bps as f64) * 2.0;
        }
        if *allowance >= n as f64 {
            *allowance -= n as f64;
            return;
        }
        let deficit = n as f64 - *allowance;
        let wait_time = deficit / (bps as f64);
        std::thread::sleep(Duration::from_secs_f64(wait_time));
        *allowance = 0.0;
    }
}

pub struct MultiLimiter {
    pub global: RateLimiter,
    pub per_download: RateLimiter,
}

impl MultiLimiter {
    pub fn new(global_bps: u64, download_bps: u64) -> Self {
        Self {
            global: RateLimiter::new(global_bps),
            per_download: RateLimiter::new(download_bps),
        }
    }

    pub fn wait_n(&self, n: u64) {
        self.global.wait_n(n);
        self.per_download.wait_n(n);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter_no_limit() {
        let limiter = RateLimiter::new(0);
        // Should return immediately when bps is 0
        limiter.wait_n(10_000_000);
    }

    #[test]
    fn test_multi_limiter_no_limit() {
        let limiter = MultiLimiter::new(0, 0);
        limiter.wait_n(1000);
    }

    #[test]
    fn test_multi_limiter_partial_limit() {
        let limiter = MultiLimiter::new(100_000, 0);
        // Per-download unlimited, global has limit
        limiter.wait_n(1);
    }
}
