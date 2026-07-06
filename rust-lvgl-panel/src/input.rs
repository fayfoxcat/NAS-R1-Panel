// Touch input via evdev.
// Reads touch events from /dev/input/ and forwards them to LVGL.

use std::fs::{self, File};
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::io::RawFd;
use std::time::Duration;

// Linux input subsystem constants (not in libc crate)
const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;
const EV_ABS: u16 = 0x03;
const ABS_X: u16 = 0x00;
const ABS_Y: u16 = 0x01;
const ABS_MT_POSITION_X: u16 = 0x35;
const ABS_MT_POSITION_Y: u16 = 0x36;
const BTN_TOUCH: u16 = 0x14a;
const SYN_REPORT: u16 = 0;

// EVIOCGBIT defined as fn below

#[repr(C)]
struct input_event {
    tv_sec: libc::time_t,
    tv_usec: libc::suseconds_t,
    type_: u16,
    code: u16,
    value: i32,
}

pub struct TouchInput {
    fd: File,
    last_x: i32,
    last_y: i32,
    touching: bool,
}

impl TouchInput {
    /// Find and open a touch input device.
    pub fn open() -> Option<Self> {
        if let Ok(entries) = fs::read_dir("/dev/input") {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name.starts_with("event") {
                    if let Ok(f) = File::open(&path) {
                        if has_abs_events(f.as_raw_fd()) {
                            log::info!("Touch device: {}", path.display());
                            // Set non-blocking
                            set_nonblocking(f.as_raw_fd());
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
    pub fn poll(&mut self, lvgl: &crate::ui::LvglHandle) {
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
    let ev_size = std::mem::size_of::<input_event>();

    for chunk in data.chunks(ev_size) {
        if chunk.len() < ev_size {
            break;
        }
        let ev: input_event = unsafe { std::ptr::read(chunk.as_ptr() as *const _) };
        match ev.type_ {
            EV_ABS => match ev.code {
                ABS_X | ABS_MT_POSITION_X => events.push(InputEvent::AbsX(ev.value)),
                ABS_Y | ABS_MT_POSITION_Y => events.push(InputEvent::AbsY(ev.value)),
                _ => {}
            },
            EV_KEY if ev.code == BTN_TOUCH => {
                if ev.value == 1 {
                    events.push(InputEvent::TouchDown);
                } else {
                    events.push(InputEvent::TouchUp);
                }
            }
            EV_SYN if ev.code == SYN_REPORT => {
                events.push(InputEvent::TouchMove);
            }
            _ => {}
        }
    }
    events
}

fn has_abs_events(fd: RawFd) -> bool {
    let mut absbits: [u8; 8] = [0; 8];
    let ret = unsafe {
        libc::ioctl(
            fd,
            EVIOCGBIT(EV_ABS as usize, std::mem::size_of::<[u8; 8]>()) as i32,
            &mut absbits,
        )
    };
    ret >= 0 && absbits[0] & 0x03 == 0x03 // ABS_X + ABS_Y
}

fn set_nonblocking(fd: RawFd) {
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL, 0);
        libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
    }
}

fn EVIOCGBIT(ev: usize, len: usize) -> u64 {
    // _IOC(_IOC_READ, 'E', 0x20 + ev, len)
    const IOC_READ: u64 = 0x80000000;
    let dir = IOC_READ;
    let ioc_type = b'E' as u64;
    let nr = 0x20 + ev as u64;
    let size = len as u64;
    (dir) | (size << 16) | (ioc_type << 8) | nr
}
