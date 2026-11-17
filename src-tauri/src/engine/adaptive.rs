//! Adaptive Auto connections.
//!
//! Auto (`connections == 0`) starts conservatively and climbs the ladder
//! `1 → 2 → 4 → 8 → 16 → 32 → 64` one step at a time, keeping a step only when
//! a measurement window shows a real throughput gain and no new retries/stalls.
//! A failed step reverts and cools down, so the worker count cannot oscillate.
//!
//! Manual (`connections > 0`) never reaches this module: the engine only
//! adjusts `desired_connections` while the Auto flag is set.
//!
//! Scaling down never cancels an in-flight range — it lowers the target and
//! idle workers leave at a task boundary (see the worker loop in
//! `concurrent.rs`).

use std::time::{Duration, Instant};

/// Candidate worker counts. Each step is at most 2× the previous so a download
/// only ever grows one level at a time.
pub const STEPS: [u32; 7] = [1, 2, 4, 8, 16, 32, 64];

/// Smallest step strictly greater than `level`.
pub fn next_step(level: u32) -> u32 {
    STEPS.iter().copied().find(|&s| s > level).unwrap_or(level)
}

/// Roughly how much data one connection needs to be worth opening. Bounds the
/// Auto ceiling so a 12 MiB file is not chased with 64 sockets.
const MIN_BYTES_PER_CONNECTION: u64 = 2 * 1024 * 1024;

/// The highest worker count Auto may climb to for this file. It is never below
/// the conservative initial value, so the starting count is always legal.
pub fn ceiling_for(file_size: u64) -> u32 {
    let initial = crate::engine::chunk::auto_initial_connections(file_size);
    if file_size == 0 {
        return 8.max(initial);
    }
    let by_size = (file_size / MIN_BYTES_PER_CONNECTION).max(1) as u32;
    by_size.min(64).max(initial)
}

#[derive(Clone, Debug)]
pub struct AdaptiveConfig {
    /// Minimum time between two throughput decisions.
    pub min_window: Duration,
    /// Minimum bytes downloaded in a window before deciding.
    pub min_bytes: u64,
    /// Throughput must improve by this fraction to keep a probe step.
    pub upshift_gain: f64,
    /// How long to wait before probing upward again after a revert.
    pub cooldown: Duration,
    /// Give up on growing after this many failed probes at the same level.
    pub max_failed_upshifts: u32,
}

impl Default for AdaptiveConfig {
    fn default() -> Self {
        Self {
            min_window: Duration::from_millis(1200),
            min_bytes: 512 * 1024,
            upshift_gain: 0.15,
            cooldown: Duration::from_secs(15),
            max_failed_upshifts: 2,
        }
    }
}

struct Exploration {
    target: u32,
    from: u32,
    from_bps: f64,
    from_errors: u64,
}

/// A per-download Auto controller. It only decides a *target*; the engine's
/// existing scale-up watcher and worker loop apply it.
pub struct AdaptiveController {
    cfg: AdaptiveConfig,
    ceiling: u32,
    level: u32,
    baseline_bps: f64,
    pending: Option<Exploration>,
    cooldown_until: Instant,
    failed_at_level: u32,
    last_errors: u64,
}

impl AdaptiveController {
    pub fn new(initial: u32, ceiling: u32, cfg: AdaptiveConfig) -> Self {
        let ceiling = ceiling.clamp(1, 64);
        let level = initial.clamp(1, ceiling);
        Self {
            cfg,
            ceiling,
            level,
            baseline_bps: 0.0,
            pending: None,
            cooldown_until: Instant::now(),
            failed_at_level: 0,
            last_errors: 0,
        }
    }

    pub fn level(&self) -> u32 {
        self.level
    }

    pub fn ceiling(&self) -> u32 {
        self.ceiling
    }

    /// Adopt a level set outside this controller (manual override, or Auto
    /// toggled on). Pending exploration is abandoned.
    pub fn sync(&mut self, current: u32, now: Instant) {
        self.pending = None;
        let current = current.clamp(1, 64);
        if current != self.level {
            log::debug!(
                "[auto] external connections change {} → {}",
                self.level,
                current
            );
            self.level = current;
            self.baseline_bps = 0.0;
            self.failed_at_level = 0;
            self.cooldown_until = now + self.cfg.cooldown;
        }
    }

    /// Feed one measurement window. Returns `Some(level)` when the target
    /// should change, `None` when the current level stands.
    pub fn observe(&mut self, now: Instant, bps: f64, errors: u64) -> Option<u32> {
        if let Some(p) = self.pending.take() {
            let new_errors = errors.saturating_sub(p.from_errors);
            let improved = p.from_bps > 0.0 && bps >= p.from_bps * (1.0 + self.cfg.upshift_gain);
            if improved && new_errors == 0 {
                log::info!(
                    "[auto] keep connections={} ({:.2} MB/s, was {:.2} MB/s at {})",
                    p.target,
                    bps / 1_048_576.0,
                    p.from_bps / 1_048_576.0,
                    p.from
                );
                self.level = p.target;
                self.baseline_bps = bps;
                self.failed_at_level = 0;
                // One more window is measured before the next probe.
                self.cooldown_until = now;
                return None;
            }
            let why = if new_errors > 0 {
                format!("{new_errors} new retry/stall")
            } else {
                format!("gain below {:.0}%", self.cfg.upshift_gain * 100.0)
            };
            log::info!(
                "[auto] revert connections {} → {} ({why})",
                p.target,
                p.from
            );
            self.level = p.from;
            self.failed_at_level += 1;
            if self.failed_at_level >= self.cfg.max_failed_upshifts {
                log::info!(
                    "[auto] stop growing at connections={} for this download",
                    p.from
                );
                self.ceiling = p.from.max(1);
            }
            self.baseline_bps = p.from_bps;
            self.cooldown_until = now + self.cfg.cooldown;
            return Some(self.level);
        }

        let new_errors = errors.saturating_sub(self.last_errors);
        self.last_errors = errors;
        self.baseline_bps = bps;
        if new_errors > 0 {
            // Errors in the holding window: do not probe upward right now.
            self.cooldown_until = now + self.cfg.cooldown;
            return None;
        }
        if now >= self.cooldown_until && self.level < self.ceiling {
            let target = next_step(self.level).min(self.ceiling);
            if target > self.level {
                self.pending = Some(Exploration {
                    target,
                    from: self.level,
                    from_bps: bps,
                    from_errors: errors,
                });
                self.level = target;
                return Some(target);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctrl(initial: u32, ceiling: u32) -> AdaptiveController {
        AdaptiveController::new(initial, ceiling, AdaptiveConfig::default())
    }

    #[test]
    fn climbs_one_step_at_a_time_and_keeps_a_real_gain() {
        let mut c = ctrl(4, 64);
        let t0 = Instant::now();
        assert_eq!(c.observe(t0, 100.0 * 1_048_576.0, 0), Some(8));
        // 30% faster → keep 8.
        assert_eq!(
            c.observe(t0 + Duration::from_secs(3), 130.0 * 1_048_576.0, 0),
            None
        );
        assert_eq!(c.level(), 8);
        // Next window may probe 8 → 16.
        assert_eq!(
            c.observe(t0 + Duration::from_secs(6), 130.0 * 1_048_576.0, 0),
            Some(16)
        );
        assert_eq!(c.level(), 16);
    }

    #[test]
    fn reverts_when_the_gain_is_too_small() {
        let mut c = ctrl(4, 64);
        let t0 = Instant::now();
        assert_eq!(c.observe(t0, 100.0 * 1_048_576.0, 0), Some(8));
        // Only 5% faster → revert.
        assert_eq!(
            c.observe(t0 + Duration::from_secs(3), 105.0 * 1_048_576.0, 0),
            Some(4)
        );
        assert_eq!(c.level(), 4);
    }

    #[test]
    fn reverts_when_retries_or_stalls_appear() {
        let mut c = ctrl(8, 64);
        let t0 = Instant::now();
        assert_eq!(c.observe(t0, 100.0 * 1_048_576.0, 0), Some(16));
        // Much faster, but a stall showed up → revert.
        assert_eq!(
            c.observe(t0 + Duration::from_secs(3), 300.0 * 1_048_576.0, 1),
            Some(8)
        );
        assert_eq!(c.level(), 8);
    }

    #[test]
    fn cooldown_blocks_immediate_re_probe() {
        let mut c = ctrl(4, 64);
        let t0 = Instant::now();
        assert_eq!(c.observe(t0, 100.0 * 1_048_576.0, 0), Some(8));
        assert_eq!(
            c.observe(t0 + Duration::from_secs(3), 100.0 * 1_048_576.0, 0),
            Some(4)
        );
        // A great reading immediately after the revert must not re-probe.
        assert_eq!(
            c.observe(t0 + Duration::from_secs(4), 500.0 * 1_048_576.0, 0),
            None
        );
        assert_eq!(c.level(), 4);
        // After the cooldown it may try again.
        assert_eq!(
            c.observe(t0 + Duration::from_secs(30), 500.0 * 1_048_576.0, 0),
            Some(8)
        );
    }

    #[test]
    fn two_failed_probes_stop_growth_for_this_download() {
        let mut c = ctrl(4, 64);
        let mut t = Instant::now();
        for _ in 0..2 {
            assert_eq!(c.observe(t, 100.0 * 1_048_576.0, 0), Some(8));
            assert_eq!(
                c.observe(t + Duration::from_secs(3), 101.0 * 1_048_576.0, 0),
                Some(4)
            );
            t += Duration::from_secs(30);
        }
        assert_eq!(c.level(), 4);
        assert_eq!(c.ceiling(), 4);
        // Long after the cooldown, still capped.
        assert_eq!(
            c.observe(t + Duration::from_secs(120), 900.0 * 1_048_576.0, 0),
            None
        );
    }

    #[test]
    fn errors_in_a_holding_window_block_the_next_probe() {
        let mut c = ctrl(4, 64);
        let t0 = Instant::now();
        // Probe 4 → 8 and keep it.
        assert_eq!(c.observe(t0, 100.0 * 1_048_576.0, 0), Some(8));
        assert_eq!(
            c.observe(t0 + Duration::from_secs(3), 130.0 * 1_048_576.0, 0),
            None
        );
        // A holding window now reports fresh errors: no probe, and a cooldown.
        assert_eq!(
            c.observe(t0 + Duration::from_secs(6), 130.0 * 1_048_576.0, 2),
            None
        );
        // Even a great clean reading right after is blocked by the cooldown.
        assert_eq!(
            c.observe(t0 + Duration::from_secs(7), 500.0 * 1_048_576.0, 2),
            None
        );
        assert_eq!(c.level(), 8);
    }

    #[test]
    fn sync_adopts_a_manual_level() {
        let mut c = ctrl(4, 64);
        let t0 = Instant::now();
        c.sync(32, t0);
        assert_eq!(c.level(), 32);
        // A sync starts a cooldown, so the next window holds ...
        assert_eq!(c.observe(t0 + Duration::from_secs(1), 10.0, 0), None);
        // ... and after it, the probe goes 32 → 64, not back down.
        assert_eq!(c.observe(t0 + Duration::from_secs(20), 10.0, 0), Some(64));
    }

    #[test]
    fn ceiling_never_below_the_initial_value() {
        assert_eq!(ceiling_for(1024), 1);
        assert_eq!(ceiling_for(2 * 1024 * 1024), 4);
        assert_eq!(ceiling_for(3 * 1024 * 1024), 4);
        assert_eq!(ceiling_for(16 * 1024 * 1024), 8);
        assert_eq!(ceiling_for(128 * 1024 * 1024), 64);
        assert_eq!(ceiling_for(5 * 1024 * 1024 * 1024), 64);
    }

    #[test]
    fn steps_are_monotonic_and_doubling() {
        for pair in STEPS.windows(2) {
            assert_eq!(pair[1], pair[0] * 2, "{STEPS:?}");
        }
        assert_eq!(next_step(64), 64);
        assert_eq!(next_step(4), 8);
        assert_eq!(next_step(3), 4);
    }
}
