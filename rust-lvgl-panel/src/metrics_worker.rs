//! Background metric scheduling.
//!
//! Samples are produced by bounded workers so a slow smartctl/docker/virsh/
//! systemctl probe can never delay touch input or the 56 Hz display loop.

use crate::metrics::{self, FastData, SlowData};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

pub struct MetricUpdates {
    pub fast: Receiver<FastData>,
    pub slow: Receiver<SlowData>,
}

pub fn spawn(running: Arc<AtomicBool>) -> MetricUpdates {
    let (fast_tx, fast) = mpsc::sync_channel(1);
    let (slow_tx, slow) = mpsc::sync_channel(1);
    thread::spawn({
        let running = running.clone();
        move || {
            worker(
                running,
                fast_tx,
                Duration::from_millis(500),
                metrics::collect_fast,
            )
        }
    });
    thread::spawn(move || {
        worker(
            running,
            slow_tx,
            Duration::from_secs(5),
            metrics::collect_slow,
        )
    });
    MetricUpdates { fast, slow }
}

/// Sample at `interval` on a capacity-1 channel: a stale sample is dropped
/// rather than queued, and a full channel never blocks the collector.
fn worker<T>(
    running: Arc<AtomicBool>,
    sender: SyncSender<T>,
    interval: Duration,
    sample: impl Fn() -> T,
) {
    let mut next_sample = Instant::now();
    while running.load(Ordering::SeqCst) {
        match sender.try_send(sample()) {
            Ok(()) | Err(TrySendError::Full(_)) => {}
            Err(TrySendError::Disconnected(_)) => break,
        }
        next_sample += interval;
        // Sleep in small steps so a Ctrl-C stop is noticed promptly even
        // when a slow probe has drifted past the next sample time.
        while Instant::now() < next_sample && running.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(100));
        }
    }
}
