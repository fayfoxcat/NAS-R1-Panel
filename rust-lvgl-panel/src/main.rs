// HS-NAS-R1 Panel — Rust + LVGL rewrite
//
// Replaces: Go backend + cog browser + WPEWebProcess (~866 MB)
// With:     Single Rust binary + LVGL direct DRM rendering (~10 MB)
//
// Architecture:
//   main.rs  ─→  ui.rs (LVGL widgets)
//            ─→  metrics.rs (/proc + sysfs reading)
//            ─→  display.rs (DRM/KMS framebuffer)
//            ─→  input.rs (touch via evdev)

mod display;
mod input;
mod metrics;
mod ui;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

fn main() {
    env_logger::init();
    log::info!("r1-panel starting (rust+lvgl)");

    // 1. Open DRM display and get framebuffer
    let mut display = display::DrmDisplay::open().expect("Failed to open DRM display");
    log::info!(
        "Display: {}x{} ({} connector)",
        display.width(),
        display.height(),
        display.connector_name()
    );

    // 2. Initialize LVGL with the framebuffer
    let lvgl = ui::init_lvgl(
        display.width() as u32,
        display.height() as u32,
        display.fb_ptr(),
    );

    // 3. Open touch input device
    let mut touch = input::TouchInput::open();
    log::info!("Touch: {}", touch.is_some());

    // 4. Build UI panels
    ui::build_ui();

    // 5. Shared running flag (for SIGTERM/SIGINT)
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })
    .ok();

    // 6. Main event loop
    let mut last_metrics = std::time::Instant::now();
    let metrics_interval = std::time::Duration::from_secs(5);

    while running.load(Ordering::SeqCst) {
        // Process LVGL timer (~5ms tick)
        lvgl.tick(5);
        lvgl.task_handler();

        // Process touch events
        if let Some(ref mut t) = touch {
            t.poll(&lvgl);
        }

        // Refresh system metrics every 5s
        if last_metrics.elapsed() >= metrics_interval {
            let data = metrics::collect();
            ui::update(&data);
            last_metrics = std::time::Instant::now();
        }

        // Render frame and swap buffers
        display.page_flip();
        std::thread::sleep(std::time::Duration::from_millis(16)); // ~60fps cap
    }

    log::info!("r1-panel shutting down");
}
