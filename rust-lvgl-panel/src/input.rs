// Touch input via evdev with vertical scrolling, horizontal paging, and taps.

use std::fs::{self, File};
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::io::RawFd;

const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;
const EV_ABS: u16 = 0x03;
const ABS_X: u16 = 0x00;
const ABS_Y: u16 = 0x01;
const ABS_MT_POSITION_X: u16 = 0x35;
const ABS_MT_POSITION_Y: u16 = 0x36;
const ABS_MT_TRACKING_ID: u16 = 0x39;
const BTN_TOUCH: u16 = 0x14a;
const SYN_REPORT: u16 = 0;
const AXIS_LOCK_DISTANCE: i32 = 8;
const AXIS_UNLOCK_MARGIN: i32 = 40;
const PAGE_SWIPE_PERCENT: i32 = 22;
const TAP_SLOP: i32 = 12;

#[repr(C)]
struct input_event {
    tv_sec: libc::c_long,
    tv_usec: libc::c_long,
    type_: u16,
    code: u16,
    value: i32,
}

#[repr(C)]
struct input_absinfo {
    value: i32,
    minimum: i32,
    maximum: i32,
    fuzz: i32,
    flat: i32,
    resolution: i32,
}

pub struct TouchInput {
    fd: File,
    last_x: i32,
    last_y: i32,
    touching: bool,
    start_x: i32,
    start_y: i32,
    last_report_y: i32,
    gesture_axis: u8,
    needs_origin: bool,
    x_min: i32,
    x_max: i32,
    y_min: i32,
    y_max: i32,
}

#[derive(Debug, Default)]
pub struct TouchUpdate {
    pub scroll_y: i32,
    pub scroll_finished: bool,
    pub page_delta: i32,
    pub swipe_x: Option<i32>,
    pub swipe_finished: bool,
    pub tap: Option<(i32, i32)>,
    pub touched: bool,
    pub touch_started: bool,
    pub touching: bool,
}

impl TouchInput {
    pub fn open() -> Option<Self> {
        if let Ok(entries) = fs::read_dir("/dev/input") {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("");
                if !name.starts_with("event") {
                    continue;
                }
                let Ok(file) = File::open(&path) else {
                    continue;
                };
                if !has_abs_events(file.as_raw_fd()) {
                    continue;
                }

                let (x_min, x_max) = read_axis_range(file.as_raw_fd(), ABS_MT_POSITION_X, ABS_X)
                    .unwrap_or((0, 4095));
                let (y_min, y_max) = read_axis_range(file.as_raw_fd(), ABS_MT_POSITION_Y, ABS_Y)
                    .unwrap_or((0, 4095));
                log::info!(
                    "Touch device: {} (X {}..{}, Y {}..{})",
                    path.display(),
                    x_min,
                    x_max,
                    y_min,
                    y_max
                );
                set_nonblocking(file.as_raw_fd());
                return Some(Self {
                    fd: file,
                    last_x: 0,
                    last_y: 0,
                    touching: false,
                    start_x: 0,
                    start_y: 0,
                    last_report_y: 0,
                    gesture_axis: 0,
                    needs_origin: false,
                    x_min,
                    x_max,
                    y_min,
                    y_max,
                });
            }
        }
        log::warn!("No touch input device found");
        None
    }

    pub fn poll(&mut self, viewport_width: i32, viewport_height: i32) -> TouchUpdate {
        let mut update = TouchUpdate::default();
        let mut buffer = [0u8; 1024];
        loop {
            match self.fd.read(&mut buffer) {
                Ok(size) if size > 0 => {
                    for event in parse_events(&buffer[..size]) {
                        match event {
                            InputEvent::AbsX(value) => self.last_x = value,
                            InputEvent::AbsY(value) => self.last_y = value,
                            InputEvent::TouchDown => {
                                if self.touching {
                                    continue;
                                }
                                self.touching = true;
                                self.start_x = self.last_x;
                                self.start_y = self.last_y;
                                self.last_report_y = self.last_y;
                                self.gesture_axis = 0;
                                self.needs_origin = true;
                                update.touched = true;
                                update.touch_started = true;
                            }
                            InputEvent::TouchUp => {
                                if self.touching {
                                    self.finish_gesture(
                                        viewport_width,
                                        viewport_height,
                                        &mut update,
                                    );
                                }
                            }
                            InputEvent::TouchMove if self.touching => {
                                if self.needs_origin {
                                    self.start_x = self.last_x;
                                    self.start_y = self.last_y;
                                    self.last_report_y = self.last_y;
                                    self.needs_origin = false;
                                    update.touched = true;
                                    continue;
                                }
                                let total_x = self.scale_x(self.last_x - self.start_x);
                                let total_y = self.scale_y(self.last_y - self.start_y);
                                // Match the master web UI: vertical motion starts
                                // immediately. Only horizontal motion is locked,
                                // and it may take over after a small vertical lead.
                                if should_lock_horizontal(self.gesture_axis, total_x, total_y) {
                                    self.gesture_axis = 2;
                                } else if self.gesture_axis == 2
                                    && should_unlock_horizontal(total_x, total_y)
                                {
                                    // The gesture was locked horizontal early (e.g.
                                    // small finger jitter on touch-down) but the
                                    // user is clearly scrolling vertically: hand
                                    // control back to the vertical axis so the
                                    // scroll is not swallowed.
                                    self.gesture_axis = 1;
                                    self.last_report_y = self.start_y;
                                }
                                if self.gesture_axis == 2 {
                                    update.swipe_x =
                                        Some(total_x.clamp(-viewport_width, viewport_width));
                                } else {
                                    update.scroll_y +=
                                        self.scale_y(self.last_report_y - self.last_y);
                                    if total_y.abs() >= TAP_SLOP {
                                        self.gesture_axis = 1;
                                    }
                                }
                                self.last_report_y = self.last_y;
                                update.touched = true;
                            }
                            InputEvent::TouchMove => {}
                        }
                    }
                }
                Ok(_) => break,
                Err(ref error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) => {
                    log::warn!("Touch read error: {}", error);
                    break;
                }
            }
        }

        update.scroll_y = update
            .scroll_y
            .clamp(-viewport_height / 2, viewport_height / 2);
        update.touching = self.touching;
        update
    }

    fn finish_gesture(&mut self, width: i32, height: i32, update: &mut TouchUpdate) {
        if self.touching {
            if self.needs_origin {
                self.start_x = self.last_x;
                self.start_y = self.last_y;
            }
            let dx = self.scale_x(self.last_x - self.start_x);
            let dy = self.scale_y(self.last_y - self.start_y);
            if self.gesture_axis == 2 {
                update.swipe_x = Some(dx.clamp(-width, width));
                update.swipe_finished = true;
                if dx.abs() >= width * PAGE_SWIPE_PERCENT / 100 {
                    update.page_delta = if dx < 0 { 1 } else { -1 };
                }
            } else if dx.abs() < TAP_SLOP && dy.abs() < TAP_SLOP {
                update.tap = Some((
                    scale_position(self.last_x, self.x_min, self.x_max, width),
                    scale_position(self.last_y, self.y_min, self.y_max, height),
                ));
            } else {
                self.gesture_axis = 1;
                update.scroll_finished = true;
            }
            log::debug!(
                "Touch gesture: axis={}, dx={}, dy={}, page_delta={}, tap={}",
                self.gesture_axis,
                dx,
                dy,
                update.page_delta,
                update.tap.is_some()
            );
        }
        self.touching = false;
        self.gesture_axis = 0;
        self.needs_origin = false;
        update.touched = true;
    }

    fn scale_x(&self, value: i32) -> i32 {
        scale_delta(value, self.x_min, self.x_max, 376)
    }

    fn scale_y(&self, value: i32) -> i32 {
        scale_delta(value, self.y_min, self.y_max, 960)
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
    let event_size = std::mem::size_of::<input_event>();
    for chunk in data.chunks(event_size) {
        if chunk.len() < event_size {
            break;
        }
        let event: input_event = unsafe { std::ptr::read_unaligned(chunk.as_ptr().cast()) };
        match event.type_ {
            EV_ABS => match event.code {
                ABS_X | ABS_MT_POSITION_X => events.push(InputEvent::AbsX(event.value)),
                ABS_Y | ABS_MT_POSITION_Y => events.push(InputEvent::AbsY(event.value)),
                ABS_MT_TRACKING_ID => events.push(if event.value < 0 {
                    InputEvent::TouchUp
                } else {
                    InputEvent::TouchDown
                }),
                _ => {}
            },
            EV_KEY if event.code == BTN_TOUCH => {
                events.push(if event.value == 1 {
                    InputEvent::TouchDown
                } else {
                    InputEvent::TouchUp
                });
            }
            EV_SYN if event.code == SYN_REPORT => events.push(InputEvent::TouchMove),
            _ => {}
        }
    }
    events
}

fn has_abs_events(fd: RawFd) -> bool {
    let mut bits = [0u8; 8];
    let result = unsafe {
        libc::ioctl(
            fd,
            eviocgbit(EV_ABS as usize, bits.len()) as libc::Ioctl,
            &mut bits,
        )
    };
    let classic = has_bit(&bits, ABS_X) && has_bit(&bits, ABS_Y);
    let multitouch = has_bit(&bits, ABS_MT_POSITION_X) && has_bit(&bits, ABS_MT_POSITION_Y);
    result >= 0 && (classic || multitouch)
}

fn has_bit(bits: &[u8], code: u16) -> bool {
    bits.get(code as usize / 8)
        .is_some_and(|value| value & (1 << (code as usize % 8)) != 0)
}

fn read_axis_range(fd: RawFd, multitouch_axis: u16, classic_axis: u16) -> Option<(i32, i32)> {
    for axis in [multitouch_axis, classic_axis] {
        let mut info: input_absinfo = unsafe { std::mem::zeroed() };
        let result = unsafe { libc::ioctl(fd, eviocgabs(axis) as libc::Ioctl, &mut info) };
        if result >= 0 && info.maximum > info.minimum {
            return Some((info.minimum, info.maximum));
        }
    }
    None
}

fn scale_delta(value: i32, minimum: i32, maximum: i32, extent: i32) -> i32 {
    let range = maximum.saturating_sub(minimum).max(1);
    let scaled = value.saturating_mul(extent) / range;
    if scaled == 0 && value != 0 {
        value.signum()
    } else {
        scaled
    }
}

fn scale_position(value: i32, minimum: i32, maximum: i32, extent: i32) -> i32 {
    scale_delta(value.saturating_sub(minimum), 0, maximum - minimum, extent)
        .clamp(0, extent.saturating_sub(1))
}

fn should_lock_horizontal(current_axis: u8, dx: i32, dy: i32) -> bool {
    current_axis != 2 && dx.abs() > AXIS_LOCK_DISTANCE && dx.abs() > dy.abs()
}

fn should_unlock_horizontal(dx: i32, dy: i32) -> bool {
    dy.abs() > dx.abs() + AXIS_UNLOCK_MARGIN
}

fn set_nonblocking(fd: RawFd) {
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL, 0);
        libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
    }
}

fn eviocgbit(event_type: usize, length: usize) -> u64 {
    0x80000000 | ((length as u64) << 16) | ((b'E' as u64) << 8) | (0x20 + event_type as u64)
}

fn eviocgabs(axis: u16) -> u64 {
    0x80000000
        | ((std::mem::size_of::<input_absinfo>() as u64) << 16)
        | ((b'E' as u64) << 8)
        | (0x40 + axis as u64)
}

#[cfg(test)]
mod tests {
    use super::{
        input_event, parse_events, scale_delta, scale_position, should_lock_horizontal,
        should_unlock_horizontal, InputEvent, ABS_MT_TRACKING_ID, EV_ABS,
    };

    #[test]
    fn scales_touch_coordinates() {
        assert_eq!(scale_position(0, 0, 960, 960), 0);
        assert_eq!(scale_position(960, 0, 960, 960), 959);
        assert_eq!(scale_delta(-240, 0, 960, 960), -240);
    }

    #[test]
    fn parses_multitouch_tracking_lifecycle() {
        for (value, down) in [(4, true), (-1, false)] {
            let event = input_event {
                tv_sec: 0,
                tv_usec: 0,
                type_: EV_ABS,
                code: ABS_MT_TRACKING_ID,
                value,
            };
            let bytes = unsafe {
                std::slice::from_raw_parts(
                    (&event as *const input_event).cast::<u8>(),
                    std::mem::size_of::<input_event>(),
                )
            };
            let parsed = parse_events(bytes);
            assert!(
                matches!(
                    parsed.as_slice(),
                    [InputEvent::TouchDown] if down
                ) || matches!(parsed.as_slice(), [InputEvent::TouchUp] if !down)
            );
        }
    }

    #[test]
    fn horizontal_motion_can_take_over_after_vertical_motion() {
        assert!(!should_lock_horizontal(0, 7, 12));
        assert!(!should_lock_horizontal(1, 40, 97));
        assert!(should_lock_horizontal(1, -200, 97));
        assert!(!should_lock_horizontal(2, -240, 30));
    }

    #[test]
    fn horizontal_lock_yields_to_clearly_vertical_motion() {
        // Real device log: dx=-46, dy=913 (locked horizontal by early jitter,
        // then an almost purely vertical swipe) must hand back to vertical.
        assert!(should_unlock_horizontal(-46, 913));
        assert!(should_unlock_horizontal(-10, 100));
        // Genuine horizontal swipes with vertical wobble stay horizontal.
        assert!(!should_unlock_horizontal(277, 221));
        assert!(!should_unlock_horizontal(-118, 85));
        assert!(!should_unlock_horizontal(371, 242));
        assert!(!should_unlock_horizontal(100, 120));
    }
}
