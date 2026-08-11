//! Scroll and page-transition state helpers used by the main event loop.

use crate::{metrics, view};
use std::time::{Duration, Instant};

pub(crate) fn record_render(
    elapsed: Duration,
    frames: &mut u32,
    total: &mut Duration,
    maximum: &mut Duration,
) {
    *frames += 1;
    *total += elapsed;
    *maximum = (*maximum).max(elapsed);
}

pub(crate) fn average_render_ms(total: Duration, frames: u32) -> f64 {
    if frames == 0 {
        0.0
    } else {
        total.as_secs_f64() * 1000.0 / frames as f64
    }
}

pub(crate) fn apply_scroll(
    scroll: &mut [i32; 4],
    data: &metrics::SystemData,
    page: view::Page,
    viewport_height: u32,
    delta: i32,
) -> bool {
    let index = page.index();
    let maximum = view::max_scroll(data, page, viewport_height);
    let previous = scroll[index];
    scroll[index] = scroll[index].saturating_add(delta).clamp(0, maximum);
    scroll[index] != previous
}

pub(crate) struct SwipeAnimation {
    pub(crate) start_x: i32,
    pub(crate) end_x: i32,
    pub(crate) page_delta: i32,
    pub(crate) started: Instant,
    pub(crate) frames: u32,
    pub(crate) total: Duration,
    pub(crate) max: Duration,
}

pub(crate) fn master_swipe_easing(progress: f64) -> f64 {
    fn cubic(t: f64, first: f64, second: f64) -> f64 {
        let inverse = 1.0 - t;
        3.0 * inverse * inverse * t * first + 3.0 * inverse * t * t * second + t * t * t
    }
    let mut low = 0.0;
    let mut high = 1.0;
    for _ in 0..12 {
        let middle = (low + high) / 2.0;
        if cubic(middle, 0.25, 0.45) < progress {
            low = middle;
        } else {
            high = middle;
        }
    }
    cubic((low + high) / 2.0, 0.46, 0.94)
}
