//! Per-phase wall-clock timing instrumentation for the analyze pipeline
//! (hotspots#267 part 1). Opt-in: callers pass `Option<&TimingCollector>`
//! through the pipeline, and every instrumentation point is a no-op when
//! that's `None`, so default `analyze` behavior pays no cost.
//!
//! Collection uses plain atomics (no locks) so it's safe to share across the
//! rayon workers that parallelize per-file analysis without perturbing the
//! very timings being measured.

use serde::Serialize;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// Accumulates wall-clock time spent in one pipeline phase, possibly from
/// many threads.
#[derive(Default)]
pub struct PhaseTimer(AtomicU64);

impl PhaseTimer {
    pub fn record(&self, elapsed: Duration) {
        self.0
            .fetch_add(elapsed.as_nanos() as u64, Ordering::Relaxed);
    }

    pub fn total(&self) -> Duration {
        Duration::from_nanos(self.0.load(Ordering::Relaxed))
    }
}

/// Shared accumulator for one `analyze` invocation's per-phase timing.
#[derive(Default)]
pub struct TimingCollector {
    pub discover: PhaseTimer,
    pub parse: PhaseTimer,
    pub cfg: PhaseTimer,
    pub metrics: PhaseTimer,
    pub scoring: PhaseTimer,
    pub aggregate: PhaseTimer,
    pub report: PhaseTimer,
    files_processed: AtomicUsize,
    functions_processed: AtomicUsize,
}

impl TimingCollector {
    pub fn record_file(&self) {
        self.files_processed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_function(&self) {
        self.functions_processed.fetch_add(1, Ordering::Relaxed);
    }

    /// Snapshots the current totals into a serializable report.
    pub fn report(&self) -> TimingReport {
        TimingReport {
            phases: vec![
                PhaseTiming::new("discover", self.discover.total()),
                PhaseTiming::new("parse", self.parse.total()),
                PhaseTiming::new("cfg", self.cfg.total()),
                PhaseTiming::new("metrics", self.metrics.total()),
                PhaseTiming::new("scoring", self.scoring.total()),
                PhaseTiming::new("aggregate", self.aggregate.total()),
                PhaseTiming::new("report", self.report.total()),
            ],
            files_processed: self.files_processed.load(Ordering::Relaxed),
            functions_processed: self.functions_processed.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PhaseTiming {
    pub phase: &'static str,
    pub wall_ms: f64,
}

impl PhaseTiming {
    fn new(phase: &'static str, elapsed: Duration) -> Self {
        Self {
            phase,
            wall_ms: elapsed.as_secs_f64() * 1000.0,
        }
    }
}

/// A point-in-time snapshot of one `analyze` run's per-phase timing,
/// suitable for `--timing-report` JSON output.
#[derive(Debug, Serialize)]
pub struct TimingReport {
    pub phases: Vec<PhaseTiming>,
    pub files_processed: usize,
    pub functions_processed: usize,
}

/// Runs `f`, adding its wall-clock duration to `timer` when present.
/// A `None` timer costs one branch and nothing else.
pub fn timed<T>(timer: Option<&PhaseTimer>, f: impl FnOnce() -> T) -> T {
    match timer {
        Some(timer) => {
            let start = Instant::now();
            let result = f();
            timer.record(start.elapsed());
            result
        }
        None => f(),
    }
}
