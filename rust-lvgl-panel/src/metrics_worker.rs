//! Background metric scheduling.
//!
//! Fast samples are intentionally produced by their own bounded worker so a
//! slow smartctl/docker/virsh/systemctl probe can never delay touch input or
//! the 56 Hz display loop.

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

    spawn_fast_worker(running.clone(), fast_tx);
    spawn_slow_worker(running, slow_tx);

    MetricUpdates { fast, slow }
}

fn spawn_fast_worker(running: Arc<AtomicBool>, sender: SyncSender<FastData>) {
    thread::spawn(move || {
        let interval = Duration::from_millis(500);
        let mut next_sample = Instant::now();
        while running.load(Ordering::SeqCst) {
            let sample = metrics::collect_fast();
            match sender.try_send(sample) {
                Ok(()) | Err(TrySendError::Full(_)) => {}
                Err(TrySendError::Disconnected(_)) => break,
            }
            next_sample += interval;
            let wait = next_sample.saturating_duration_since(Instant::now());
            if !wait.is_zero() {
                thread::sleep(wait);
            } else {
                next_sample = Instant::now();
            }
        }
    });
}

fn spawn_slow_worker(running: Arc<AtomicBool>, sender: SyncSender<SlowData>) {
    thread::spawn(move || {
        let interval = Duration::from_secs(5);
        while running.load(Ordering::SeqCst) {
            let sample = metrics::collect_slow();
            match sender.try_send(sample) {
                Ok(()) | Err(TrySendError::Full(_)) => {}
                Err(TrySendError::Disconnected(_)) => break,
            }
            let mut slept = Duration::ZERO;
            while slept < interval && running.load(Ordering::SeqCst) {
                let step = (interval - slept).min(Duration::from_millis(100));
                thread::sleep(step);
                slept += step;
            }
        }
    });
}
