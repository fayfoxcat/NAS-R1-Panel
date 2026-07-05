// Touch input via evdev.
// Reads touch events from /dev/input/ and forwards them to LVGL.

use std::fs::{self, File};
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::io::RawFd;
use std::path::Path;

pub struct TouchInput {
    fd: File,
    last_x: i32,
    last_y: i32,
    touching: bool,
}

impl TouchInput {
    /// Find and open a touch input device.
    pub fn open() -> Option<Self> {
        // Search /dev/input/event* for touch-capable devices
        if let Ok(entries) = fs::read_dir("/dev/input") {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name.starts_with("event") {
                    if let Ok(f) = File::open(&path) {
                        // Check if this device supports ABS_X and ABS_Y (touchscreen)
                        if has_abs_events(f.as_raw_fd()) {
                            log::info!("Touch device: {}", path.display());
                            // Re-open in non-blocking mode for poll()
                            let f = File::open(&path).ok()?;
                            return Some(TouchInput {
                                fd: f,
                                last_x: 0,
                                last_y: 0,
                                touching: false,
                            });
                        }
                    }
                }
            }
        }
        log::warn!("No touch input device found");
        None
    }

    /// Poll for new events and forward to LVGL.
    /// This is called in the main loop.
    pub fn poll(&mut self, lvgl: &LvglHandle) {
        let mut buf = [0u8; 256];
        match self.fd.read(&mut buf) {
            Ok(n) if n > 0 => {
                let events = parse_events(&buf[..n]);
                for ev in events {
                    match ev {
                        InputEvent::AbsX(x) => self.last_x = x,
                        InputEvent::AbsY(y) => self.last_y = y,
                        InputEvent::TouchDown => {
                            self.touching = true;
                            lvgl.send_touch(self.last_x, self.last_y, true);
                        }
                        InputEvent::TouchUp => {
                            self.touching = false;
                            lvgl.send_touch(self.last_x, self.last_y, false);
                        }
                        InputEvent::TouchMove => {
                            if self.touching {
                                lvgl.send_touch(self.last_x, self.last_y, true);
                            }
                        }
                    }
                }
            }
            Ok(_) => {}
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => log::warn!("Touch read error: {}", e),
        }
    }
}

#[derive(Debug)]
enum InputEvent {
    AbsX(i32),
    AbsY(i32),
    TouchDown,
    TouchUp,
    TouchMove,
}

fn parse_events(data: &[u8]) -> Vec<InputEvent> {
    let mut events = Vec::new();
    let ev_size = std::mem::size_of::<libc::input_event>();

    for chunk in data.chunks(ev_size) {
        if chunk.len() < ev_size {
            break;
        }
        let ev: libc::input_event = unsafe { std::ptr::read(chunk.as_ptr() as *const _) };
        match ev.type_ as u32 {
            libc::EV_ABS => match ev.code as u32 {
                libc::ABS_X => events.push(InputEvent::AbsX(ev.value)),
                libc::ABS_Y => events.push(InputEvent::AbsY(ev.value)),
                libc::ABS_MT_POSITION_X => events.push(InputEvent::AbsX(ev.value)),
                libc::ABS_MT_POSITION_Y => events.push(InputEvent::AbsY(ev.value)),
                _ => {}
            },
            libc::EV_KEY => match ev.code as u32 {
                libc::BTN_TOUCH => {
                    if ev.value == 1 {
                        events.push(InputEvent::TouchDown);
                    } else {
                        events.push(InputEvent::TouchUp);
                    }
                }
                _ => {}
            },
            libc::EV_SYN => {
                // SYN_REPORT = end of touch frame; check if touch moved
                if ev.code as u32 == libc::SYN_REPORT {
                    events.push(InputEvent::TouchMove);
                }
            }
            _ => {}
        }
    }
    events
}

fn has_abs_events(fd: RawFd) -> bool {
    let mut absbits: [u8; 8] = [0; 8]; // ABS_X = 0x00, ABS_Y = 0x01
    unsafe {
        if libc::ioctl(
            fd,
            libc::EVIOCGBIT(libc::EV_ABS as usize, std::mem::size_of::<[u8; 8]>()),
            &mut absbits,
        ) >= 0
        {
            // Check bit 0 (ABS_X) and bit 1 (ABS_Y)
            absbits[0] & 0x03 == 0x03
        } else {
            false
        }
    }
}

/// Minimal LVGL interface — will be fleshed out in ui.rs.
/// This trait-like pattern allows the input module to feed
/// touch events to LVGL without circular dependencies.
pub struct LvglHandle;

impl LvglHandle {
    pub fn send_touch(&self, x: i32, y: i32, pressed: bool) {
        // Forward to LVGL's indev
        #[allow(unused_unsafe)]
        unsafe {
            // lvgl-sys: lv_indev_set_button_value / lv_indev_set_cursor_pos
            // Will be implemented once LVGL init is wired up.
            log::trace!("Touch: ({}, {}) pressed={}", x, y, pressed);
        }
    }
}
