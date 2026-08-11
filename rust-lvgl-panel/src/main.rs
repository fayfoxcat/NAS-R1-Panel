// HS-NAS-R1 Panel — Rust native DRM/KMS panel.
//
// The event loop coordinates the focused components in this crate:
// display (DRM/KMS), input (evdev), metrics (system data), metrics_worker
// (bounded refresh workers), render (software rasterizer), and view (layout).

mod display;
mod input;
mod interaction;
mod metrics;
mod metrics_worker;
mod render;
mod view;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

fn main() {
    env_logger::init();
    log::info!("r1-panel starting (Rust framebuffer renderer)");

    if let Some(path) = screenshot_path() {
        render_screenshot(path);
        return;
    }

    // 1. Open DRM display and get framebuffer
    let mut display = display::DrmDisplay::open().expect("Failed to open DRM display");
    log::info!(
        "Display: {}x{} ({} connector)",
        display.width(),
        display.height(),
        display.connector_name()
    );

    // 2. Create pure-Rust renderer (no LVGL)
    let mut render = render::Renderer::new(
        display.fb_ptr(),
        display.width(),
        display.height(),
        display.stride(),
    );
    log::info!("Pure Rust renderer ready");

    // 3. Open touch input device
    let mut touch = input::TouchInput::open();
    log::info!("Touch: {}", touch.is_some());

    let running = Arc::new(AtomicBool::new(true));
    let signal_flag = running.clone();
    ctrlc::set_handler(move || signal_flag.store(false, Ordering::SeqCst)).ok();

    // 4. Main event loop
    let refresh_kick = Arc::new(AtomicBool::new(false));
    display.spawn_link_refresher(running.clone(), refresh_kick.clone());

    log::info!("Entering main loop");

    // Draw initial frame
    let mut data = metrics::collect();
    let metric_updates = metrics_worker::spawn(running.clone());
    let mut page = view::Page::Overview;
    let mut scroll = [0i32; 4];
    let mut overlay = view::Overlay::None;
    let mut scroll_velocity = 0.0f64;
    let mut scroll_fraction = 0.0f64;
    let mut last_scroll_event: Option<std::time::Instant> = None;
    let mut last_frame = std::time::Instant::now();
    let mut inertia_active = false;
    let mut touch_active = false;
    let mut full_redraw_pending = false;
    let mut dynamic_redraw_pending = false;
    let mut gesture_render_frames = 0u32;
    let mut gesture_render_total = std::time::Duration::ZERO;
    let mut gesture_render_max = std::time::Duration::ZERO;
    let mut swipe_anim: Option<interaction::SwipeAnimation> = None;
    render.set_framebuffer(display.fb_ptr());
    view::draw(&render, &data, page, scroll[page.index()], overlay);
    display.present();
    display.poll_flip_events();
    capture_frame(&render);

    while running.load(Ordering::SeqCst) {
        let loop_started = std::time::Instant::now();
        let frame_now = std::time::Instant::now();
        let frame_seconds = frame_now
            .duration_since(last_frame)
            .as_secs_f64()
            .clamp(0.001, 0.05);
        last_frame = frame_now;
        let mut direct_scroll = false;
        if let Some(ref mut input) = touch {
            let update = input.poll(display.width() as i32, display.height() as i32);
            touch_active = update.touching;
            if update.touch_started {
                refresh_kick.store(true, Ordering::SeqCst);
                gesture_render_frames = 0;
                gesture_render_total = std::time::Duration::ZERO;
                gesture_render_max = std::time::Duration::ZERO;
                if swipe_anim.is_some() {
                    // A new finger during the page animation interrupts it and
                    // snaps to the final state (WebKit-style cancellation).
                    let anim = swipe_anim.take().unwrap();
                    log::debug!("Swipe animation cancelled by new touch");
                    if anim.page_delta != 0 {
                        page = page.shifted(anim.page_delta);
                        log::debug!("Page changed to {:?}", page);
                    }
                    render.set_framebuffer(display.fb_ptr());
                    view::draw(&render, &data, page, scroll[page.index()], overlay);
                    display.present();
                    display.poll_flip_events();
                }
            }
            if overlay == view::Overlay::None {
                if let Some(swipe_x) = update.swipe_x {
                    inertia_active = false;
                    scroll_velocity = 0.0;
                    scroll_fraction = 0.0;
                    last_scroll_event = None;
                    if update.swipe_finished {
                        log::debug!(
                            "Swipe release: page={:?}, dx={}px, page_delta={}, render={} frames avg={:.1}ms max={:.1}ms",
                            page,
                            swipe_x,
                            update.page_delta,
                            gesture_render_frames,
                            interaction::average_render_ms(gesture_render_total, gesture_render_frames),
                            gesture_render_max.as_secs_f64() * 1000.0
                        );
                        let target_x = if update.page_delta == 0 {
                            0
                        } else if swipe_x < 0 {
                            -(display.width() as i32)
                        } else {
                            display.width() as i32
                        };
                        swipe_anim = Some(interaction::SwipeAnimation {
                            start_x: swipe_x,
                            end_x: target_x,
                            page_delta: update.page_delta,
                            started: std::time::Instant::now(),
                            frames: 0,
                            total: std::time::Duration::ZERO,
                            max: std::time::Duration::ZERO,
                        });
                    } else {
                        let render_started = std::time::Instant::now();
                        render.set_framebuffer(display.fb_ptr());
                        view::draw_swipe(&render, &data, page, scroll, swipe_x);
                        display.present();
                        display.poll_flip_events();
                        interaction::record_render(
                            render_started.elapsed(),
                            &mut gesture_render_frames,
                            &mut gesture_render_total,
                            &mut gesture_render_max,
                        );
                    }
                } else if update.scroll_y != 0 {
                    direct_scroll = true;
                    inertia_active = false;
                    let now = std::time::Instant::now();
                    let sample_seconds = last_scroll_event
                        .map(|last| now.duration_since(last).as_secs_f64().clamp(0.004, 0.1))
                        .unwrap_or(0.016);
                    let sample_velocity = update.scroll_y as f64 / sample_seconds;
                    scroll_velocity = if last_scroll_event.is_some() {
                        scroll_velocity * 0.55 + sample_velocity * 0.45
                    } else {
                        sample_velocity
                    };
                    last_scroll_event = Some(now);
                    if interaction::apply_scroll(
                        &mut scroll,
                        &data,
                        page,
                        display.height(),
                        update.scroll_y,
                    ) {
                        let render_started = std::time::Instant::now();
                        render.set_framebuffer(display.fb_ptr());
                        view::draw(&render, &data, page, scroll[page.index()], overlay);
                        display.present();
                        display.poll_flip_events();
                        interaction::record_render(
                            render_started.elapsed(),
                            &mut gesture_render_frames,
                            &mut gesture_render_total,
                            &mut gesture_render_max,
                        );
                    }
                }
                if update.scroll_finished {
                    inertia_active = scroll_velocity.abs() >= 120.0;
                    scroll_fraction = 0.0;
                    last_scroll_event = None;
                    log::debug!(
                        "Scroll release: page={:?}, scroll={}/{}, velocity={:.0}px/s, inertia={}, render={} frames avg={:.1}ms max={:.1}ms",
                        page,
                        scroll[page.index()],
                        view::max_scroll(&data, page, display.height()),
                        scroll_velocity,
                        inertia_active,
                        gesture_render_frames,
                        interaction::average_render_ms(gesture_render_total, gesture_render_frames),
                        gesture_render_max.as_secs_f64() * 1000.0
                    );
                } else if update.touch_started {
                    // A new finger contact immediately stops the previous fling.
                    inertia_active = false;
                    scroll_velocity = 0.0;
                    scroll_fraction = 0.0;
                    last_scroll_event = None;
                }
            }

            if let Some((x, y)) = update.tap {
                inertia_active = false;
                scroll_velocity = 0.0;
                scroll_fraction = 0.0;
                let mut power_action = None;
                if let Some(target) =
                    view::hit_test(&data, page, scroll[page.index()], overlay, x, y)
                {
                    match target {
                        view::HitTarget::Reboot => {
                            overlay = view::Overlay::Confirm(view::PowerAction::Reboot)
                        }
                        view::HitTarget::Shutdown => {
                            overlay = view::Overlay::Confirm(view::PowerAction::Shutdown)
                        }
                        view::HitTarget::Cancel => overlay = view::Overlay::None,
                        view::HitTarget::Confirm(action) => {
                            overlay = view::Overlay::Executing(action);
                            power_action = Some(action);
                        }
                    }
                    render.set_framebuffer(display.fb_ptr());
                    view::draw(&render, &data, page, scroll[page.index()], overlay);
                    // The last scroll frame may still be flipping; wait briefly
                    // so the overlay frame is queued now. A dropped frame while
                    // a modal is open used to freeze the panel, because no
                    // further present is attempted in that state.
                    let flip_deadline =
                        std::time::Instant::now() + std::time::Duration::from_millis(60);
                    while !display.present() && std::time::Instant::now() < flip_deadline {
                        display.poll_flip_events();
                        std::thread::sleep(std::time::Duration::from_millis(2));
                    }
                    display.poll_flip_events();
                    if let Some(action) = power_action {
                        execute_power(action);
                    }
                }
            }
        }

        if overlay == view::Overlay::None {
            if let Some(anim) = swipe_anim.as_mut() {
                // Page-swap / bounce animation, driven one frame per loop
                // iteration so touch input keeps being polled throughout.
                let elapsed = anim.started.elapsed();
                let progress = (elapsed.as_secs_f64() / 0.28).clamp(0.0, 1.0);
                let eased = interaction::master_swipe_easing(progress);
                let x = anim.start_x as f64 + (anim.end_x - anim.start_x) as f64 * eased;
                let render_started = std::time::Instant::now();
                render.set_framebuffer(display.fb_ptr());
                view::draw_swipe(&render, &data, page, scroll, x.round() as i32);
                display.present();
                display.poll_flip_events();
                interaction::record_render(
                    render_started.elapsed(),
                    &mut anim.frames,
                    &mut anim.total,
                    &mut anim.max,
                );
                if progress >= 1.0 {
                    let anim = swipe_anim.take().unwrap();
                    log::debug!(
                        "Swipe animation: {} frames avg={:.1}ms max={:.1}ms elapsed={}ms",
                        anim.frames,
                        interaction::average_render_ms(anim.total, anim.frames),
                        anim.max.as_secs_f64() * 1000.0,
                        anim.started.elapsed().as_millis()
                    );
                    if anim.page_delta != 0 {
                        page = page.shifted(anim.page_delta);
                        log::debug!("Page changed to {:?}", page);
                    }
                    render.set_framebuffer(display.fb_ptr());
                    view::draw(&render, &data, page, scroll[page.index()], overlay);
                    display.present();
                    display.poll_flip_events();
                }
            }
        } else {
            swipe_anim = None;
        }

        if overlay == view::Overlay::None && inertia_active && !direct_scroll && !touch_active {
            let distance = scroll_velocity * frame_seconds + scroll_fraction;
            let delta = distance.trunc() as i32;
            scroll_fraction = distance - delta as f64;
            if delta != 0 {
                if interaction::apply_scroll(&mut scroll, &data, page, display.height(), delta) {
                    render.set_framebuffer(display.fb_ptr());
                    view::draw(&render, &data, page, scroll[page.index()], overlay);
                    display.present();
                    display.poll_flip_events();
                } else {
                    inertia_active = false;
                }
            }
            scroll_velocity *= 0.90f64.powf(frame_seconds * 60.0);
            if scroll_velocity.abs() < 20.0 {
                inertia_active = false;
                scroll_velocity = 0.0;
                scroll_fraction = 0.0;
            }
        }

        // Fast and slow samples arrive through bounded channels. Fast updates
        // only repaint dynamic cards; slow inventory changes request a full
        // layout pass because row heights may change.
        while let Ok(fast) = metric_updates.fast.try_recv() {
            data.apply_fast(fast);
            dynamic_redraw_pending = true;
        }
        while let Ok(slow) = metric_updates.slow.try_recv() {
            data.apply_slow(slow);
            for (index, item) in scroll.iter_mut().enumerate() {
                let item_page = match index {
                    0 => view::Page::Overview,
                    1 => view::Page::Services,
                    2 => view::Page::Vms,
                    _ => view::Page::Power,
                };
                *item = (*item).min(view::max_scroll(&data, item_page, display.height()));
            }
            full_redraw_pending = true;
            dynamic_redraw_pending = false;
        }
        if !touch_active && !inertia_active && swipe_anim.is_none() {
            render.set_framebuffer(display.fb_ptr());
            if full_redraw_pending {
                // Full redraws re-paint the overlay, so they keep running while
                // a modal is open; they also re-queue a frame whose page flip
                // was dropped, so the panel can never freeze on a lost frame.
                view::draw(&render, &data, page, scroll[page.index()], overlay);
                full_redraw_pending = false;
                dynamic_redraw_pending = false;
                display.present();
                display.poll_flip_events();
            } else if dynamic_redraw_pending && overlay == view::Overlay::None {
                // Dynamic patches only repaint page regions; they must not run
                // under a modal or the dim overlay would flicker in those spots.
                view::draw_dynamic(&render, &data, page, scroll[page.index()]);
                dynamic_redraw_pending = false;
                display.present();
                display.poll_flip_events();
            }
        }

        // Keep the full active frame (rendering included) close to the panel's
        // 56 Hz refresh period. A shorter idle cadence reduces touch-down
        // latency without continually redrawing the screen.
        let target_loop = if touch_active || inertia_active || swipe_anim.is_some() {
            std::time::Duration::from_millis(18)
        } else {
            std::time::Duration::from_millis(8)
        };
        if let Some(remaining) = target_loop.checked_sub(loop_started.elapsed()) {
            std::thread::sleep(remaining);
        }
    }

    log::info!("r1-panel shutting down");
}

fn screenshot_path() -> Option<PathBuf> {
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--screenshot" {
            return Some(PathBuf::from(
                args.next().expect("--screenshot requires an output path"),
            ));
        }
    }
    None
}

fn render_screenshot(path: PathBuf) {
    const WIDTH: u32 = 376;
    const HEIGHT: u32 = 960;
    let mut framebuffer = vec![0u8; (WIDTH * HEIGHT * 4) as usize];
    let renderer = render::Renderer::new(framebuffer.as_mut_ptr(), WIDTH, HEIGHT, WIDTH * 4);

    // The second collection can calculate network throughput for the preview.
    let _ = metrics::collect();
    std::thread::sleep(std::time::Duration::from_millis(250));
    let data = metrics::collect();
    let scroll_y = std::env::var("R1_PANEL_SCROLL")
        .ok()
        .and_then(|value| value.parse::<i32>().ok())
        .unwrap_or(0);
    let page = match std::env::var("R1_PANEL_PAGE").as_deref() {
        Ok("services") => view::Page::Services,
        Ok("vms") => view::Page::Vms,
        Ok("power") => view::Page::Power,
        _ => view::Page::Overview,
    };
    let overlay = match std::env::var("R1_PANEL_MODAL").as_deref() {
        Ok("reboot") => view::Overlay::Confirm(view::PowerAction::Reboot),
        Ok("shutdown") => view::Overlay::Confirm(view::PowerAction::Shutdown),
        _ => view::Overlay::None,
    };
    view::draw(&renderer, &data, page, scroll_y, overlay);
    renderer
        .save_bmp(&path)
        .unwrap_or_else(|error| panic!("failed to write screenshot {}: {}", path.display(), error));
    println!("Screenshot written to {}", path.display());
}

fn execute_power(action: view::PowerAction) {
    // Flush all pending writes first, then hand over to systemd's graceful
    // shutdown sequence: running services get SIGTERM (and time to flush
    // their own state), then every filesystem is unmounted cleanly before
    // the reboot/poweroff. No --force is passed: a stuck service is killed
    // by systemd's stop timeout instead of skipping the clean unmount.
    std::thread::spawn(move || {
        std::process::Command::new("sync").status().ok();
        std::thread::sleep(std::time::Duration::from_millis(200));
        let command = match action {
            view::PowerAction::Reboot => "reboot",
            view::PowerAction::Shutdown => "poweroff",
        };
        std::process::Command::new("systemctl")
            .arg(command)
            .status()
            .ok();
    });
}

fn capture_frame(renderer: &render::Renderer) {
    let Some(path) = std::env::var_os("R1_PANEL_CAPTURE") else {
        return;
    };
    let path = PathBuf::from(path);
    match renderer.save_bmp(&path) {
        Ok(()) => log::info!("Captured first frame to {}", path.display()),
        Err(error) => log::error!("Failed to capture {}: {}", path.display(), error),
    }
}
