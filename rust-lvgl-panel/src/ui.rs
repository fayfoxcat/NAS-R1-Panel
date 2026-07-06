// LVGL UI — 2-panel swipeable overview + detail.
//
// Key: uses raw FFI to LVGL C library (compiled via build.rs).
// LVGL 9.6 API: lv_ prefix, display driver with flush callback,
// flex layout, dark theme.

use crate::metrics::SystemData;
use std::ffi::{c_void, CStr, CString};
use std::ptr;

// ── Type aliases for LVGL opaque pointers ──────────────────

type lv_obj_t = c_void;
type lv_event_t = c_void;
type lv_disp_t = c_void;
type lv_indev_t = c_void;
type lv_anim_t = c_void;

// LVGL color: ARGB8888 packed into u32
type lv_color_t = u32;
type lv_font_t = c_void;

// ── LVGL FFI declarations (LVGL 9.x) ──────────────────────

#[link(name = "lvgl", kind = "static")]
extern "C" {
    // Init
    fn lv_init();
    fn lv_tick_inc(ms: u32);

    // Display
    fn lv_display_create(hor_res: i32, ver_res: i32) -> *mut lv_disp_t;
    fn lv_display_set_default(disp: *mut lv_disp_t);
    fn lv_display_set_flush_cb(disp: *mut lv_disp_t, cb: Option<unsafe extern "C" fn(*mut lv_disp_t, *const lv_area_t, *mut u8, *mut c_void)>);
    fn lv_display_set_draw_buffers(disp: *mut lv_disp_t, buf1: *mut c_void, buf2: *mut c_void, buf_size: u32, render_mode: u32);
    fn lv_display_flush_ready(disp: *mut lv_disp_t);
    fn lv_display_get_horizontal_resolution(disp: *const lv_disp_t) -> i32;
    fn lv_display_get_vertical_resolution(disp: *const lv_disp_t) -> i32;

    // Timer handler (called in loop)
    fn lv_timer_handler() -> u32;
    fn lv_timer_handler_run_in_period(ms: u32) -> u32;

    // Screen
    fn lv_screen_active() -> *mut lv_obj_t;

    // Objects
    fn lv_obj_create(parent: *mut lv_obj_t) -> *mut lv_obj_t;
    fn lv_obj_set_size(obj: *mut lv_obj_t, w: i32, h: i32);
    fn lv_obj_set_pos(obj: *mut lv_obj_t, x: i32, y: i32);
    fn lv_obj_set_x(obj: *mut lv_obj_t, x: i32);
    fn lv_obj_align(obj: *mut lv_obj_t, align: i32, x_ofs: i32, y_ofs: i32);
    fn lv_obj_align_to(obj: *mut lv_obj_t, base: *const lv_obj_t, align: i32, x_ofs: i32, y_ofs: i32);
    fn lv_obj_center(obj: *mut lv_obj_t);
    fn lv_obj_add_flag(obj: *mut lv_obj_t, flag: u32);
    fn lv_obj_remove_flag(obj: *mut lv_obj_t, flag: u32);
    fn lv_obj_has_flag(obj: *const lv_obj_t, flag: u32) -> bool;
    fn lv_obj_get_width(obj: *const lv_obj_t) -> i32;
    fn lv_obj_get_height(obj: *const lv_obj_t) -> i32;
    fn lv_obj_get_child_count(obj: *const lv_obj_t) -> u32;
    fn lv_obj_get_child(obj: *const lv_obj_t, idx: i32) -> *mut lv_obj_t;
    fn lv_obj_clean(obj: *mut lv_obj_t);
    fn lv_obj_delete(obj: *mut lv_obj_t);
    fn lv_obj_add_event_cb(obj: *mut lv_obj_t, cb: Option<unsafe extern "C" fn(*mut lv_event_t)>, event_code: i32, user_data: *mut c_void) -> *mut c_void;

    // Styles (set on specific part)
    fn r1_lv_obj_set_style_bg_color(obj: *mut lv_obj_t, color: lv_color_t, selector: u32);
    fn r1_lv_obj_set_style_bg_opa(obj: *mut lv_obj_t, opa: u32, selector: u32);
    fn r1_lv_obj_set_style_border_width(obj: *mut lv_obj_t, width: i32, selector: u32);
    fn r1_lv_obj_set_style_pad_all(obj: *mut lv_obj_t, pad: i32, selector: u32);
    fn r1_lv_obj_set_style_pad_column(obj: *mut lv_obj_t, pad: i32, selector: u32);
    fn r1_lv_obj_set_style_pad_row(obj: *mut lv_obj_t, pad: i32, selector: u32);
    fn r1_lv_obj_set_style_radius(obj: *mut lv_obj_t, radius: i32, selector: u32);
    fn r1_lv_obj_set_style_arc_color(obj: *mut lv_obj_t, color: lv_color_t, selector: u32);
    fn r1_lv_obj_set_style_text_color(obj: *mut lv_obj_t, color: lv_color_t, selector: u32);
    fn r1_lv_obj_set_style_text_font(obj: *mut lv_obj_t, font: *const lv_font_t, selector: u32);
    fn r1_lv_obj_set_flex_flow(obj: *mut lv_obj_t, flow: u32);
    fn r1_lv_obj_set_flex_align(obj: *mut lv_obj_t, main_place: u32, cross_place: u32, track_cross_place: u32);

    // Labels
    fn lv_label_create(parent: *mut lv_obj_t) -> *mut lv_obj_t;
    fn lv_label_set_text(label: *mut lv_obj_t, text: *const std::os::raw::c_char);

    // Buttons
    fn lv_button_create(parent: *mut lv_obj_t) -> *mut lv_obj_t;

    // Arc
    fn lv_arc_create(parent: *mut lv_obj_t) -> *mut lv_obj_t;
    fn lv_arc_set_range(arc: *mut lv_obj_t, min: i32, max: i32);
    fn lv_arc_set_value(arc: *mut lv_obj_t, value: i32);

    // Bar
    fn lv_bar_create(parent: *mut lv_obj_t) -> *mut lv_obj_t;
    fn lv_bar_set_range(bar: *mut lv_obj_t, min: i32, max: i32);
    fn lv_bar_set_value(bar: *mut lv_obj_t, value: i32, anim: i32);

    // Input device
    fn lv_indev_get_gesture_dir(indev: *mut lv_indev_t) -> u32;
    fn lv_indev_create() -> *mut lv_indev_t;
    fn lv_indev_set_type(indev: *mut lv_indev_t, itype: i32);
    fn lv_indev_set_cursor_pos(indev: *mut lv_indev_t, x: i32, y: i32);
    fn lv_indev_set_button_state(indev: *mut lv_indev_t, btn: i32, state: bool);

    // Event
    fn lv_event_get_indev(e: *mut lv_event_t) -> *mut lv_indev_t;
    fn lv_event_get_target(e: *mut lv_event_t) -> *mut lv_obj_t;

    // Color helper
    fn r1_lv_color_hex(hex: u32) -> lv_color_t;

    // Fonts (global static)
    static lv_font_montserrat_24: lv_font_t;

    // Theme
    fn lv_theme_default_init(
        disp: *mut lv_disp_t,
        color_primary: lv_color_t,
        color_secondary: lv_color_t,
        dark: bool,
        font: *const lv_font_t,
    ) -> *mut c_void;
}

// ── LVGL constants ─────────────────────────────────────────

// Alignment
const LV_ALIGN_TOP_LEFT: i32 = 5;
const LV_ALIGN_TOP_RIGHT: i32 = 7;
const LV_ALIGN_TOP_MID: i32 = 6;
const LV_ALIGN_BOTTOM_LEFT: i32 = 9;
const LV_ALIGN_BOTTOM_MID: i32 = 10;
const LV_ALIGN_BOTTOM_RIGHT: i32 = 11;
const LV_ALIGN_CENTER: i32 = 1;
const LV_ALIGN_OUT_BOTTOM_LEFT: i32 = 21;
const LV_ALIGN_OUT_BOTTOM_MID: i32 = 22;

// Flags
const LV_OBJ_FLAG_HIDDEN: u32 = 1 << 0;
const LV_OBJ_FLAG_CLICKABLE: u32 = 1 << 3;
const LV_OBJ_FLAG_SCROLLABLE: u32 = 1 << 1;
const LV_OBJ_FLAG_GESTURE_BUBBLE: u32 = 1 << 11;

// Events
const LV_EVENT_CLICKED: i32 = 7;
const LV_EVENT_GESTURE: i32 = 42;

// Style parts
const LV_PART_MAIN: u32 = 0;
const LV_PART_INDICATOR: u32 = 1;

// Gesture directions
const LV_DIR_LEFT: u32 = 1;
const LV_DIR_RIGHT: u32 = 2;

// Flex
const LV_FLEX_FLOW_ROW: u32 = 0;
const LV_FLEX_FLOW_COLUMN: u32 = 1;
const LV_FLEX_ALIGN_START: u32 = 0;
const LV_FLEX_ALIGN_CENTER: u32 = 1;
const LV_FLEX_ALIGN_SPACE_BETWEEN: u32 = 2;

// Misc
const LV_ZOOM_NONE: u32 = 256;
const LV_ANIM_OFF: i32 = 0;

// Display area for flush
#[repr(C)]
struct lv_area_t {
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
}

// ── LVGL Handle ────────────────────────────────────────────

pub struct LvglHandle {
    disp: *mut lv_disp_t,
    screen: *mut lv_obj_t,
}

// ── Static widget pointers ─────────────────────────────────

static mut DISPLAY: *mut lv_disp_t = ptr::null_mut();
static mut SCREEN: *mut lv_obj_t = ptr::null_mut();

// Panel container (2x screen width)
static mut PANELS_CONTAINER: *mut lv_obj_t = ptr::null_mut();
static mut CURRENT_PANEL: i32 = 0;
static mut SWIPE_BASE_X: i32 = 0;

// Panel 0 widgets
static mut CPU_ARC: *mut lv_obj_t = ptr::null_mut();
static mut CPU_LABEL: *mut lv_obj_t = ptr::null_mut();
static mut CPU_SUB: *mut lv_obj_t = ptr::null_mut();
static mut MEM_ARC: *mut lv_obj_t = ptr::null_mut();
static mut MEM_LABEL: *mut lv_obj_t = ptr::null_mut();
static mut MEM_SUB: *mut lv_obj_t = ptr::null_mut();
static mut NET_LABEL: *mut lv_obj_t = ptr::null_mut();
static mut RX_LABEL: *mut lv_obj_t = ptr::null_mut();
static mut TX_LABEL: *mut lv_obj_t = ptr::null_mut();
static mut IP_LABEL: *mut lv_obj_t = ptr::null_mut();
static mut DISK_LIST: *mut lv_obj_t = ptr::null_mut();
static mut UP_LABEL: *mut lv_obj_t = ptr::null_mut();

// Panel 1 widgets
static mut SVC_LIST: *mut lv_obj_t = ptr::null_mut();
static mut DOCKER_LIST: *mut lv_obj_t = ptr::null_mut();
static mut VM_LIST: *mut lv_obj_t = ptr::null_mut();

// Modal
static mut MODAL_BG: *mut lv_obj_t = ptr::null_mut();
static mut MODAL_MSG: *mut lv_obj_t = ptr::null_mut();
static mut MODAL_OK: *mut lv_obj_t = ptr::null_mut();
static mut MODAL_CANCEL: *mut lv_obj_t = ptr::null_mut();
static mut PENDING_ACTION: Action = Action::None;

#[derive(Clone, Copy, PartialEq)]
enum Action {
    None,
    Reboot,
    Shutdown,
}

// ── Init ───────────────────────────────────────────────────

pub fn init_lvgl(width: u32, height: u32, fb_ptr: *mut u8) -> LvglHandle {
    unsafe {
        lv_init();

        // Create display
        let disp = lv_display_create(width as i32, height as i32);
        lv_display_set_default(disp);

        // Set up draw buffer (single buffer, direct framebuffer)
        lv_display_set_draw_buffers(
            disp,
            fb_ptr as *mut c_void,
            ptr::null_mut(),
            (width * height * 4) as u32,
            0, // LV_DISPLAY_RENDER_MODE_DIRECT
        );

        // Flush callback: framebuffer is always visible (dumb buffer scanout)
        lv_display_set_flush_cb(disp, Some(flush_cb));

        DISPLAY = disp;

        // Dark theme
        let primary = r1_lv_color_hex(0x3FB950);
        let secondary = r1_lv_color_hex(0x161B22);
        lv_theme_default_init(disp, primary, secondary, true, &lv_font_montserrat_24);

        let screen = lv_screen_active();
        SCREEN = screen;

        log::info!("LVGL initialized: {}x{}", width, height);
        LvglHandle { disp, screen }
    }
}

impl LvglHandle {
    pub fn tick(&self, ms: u32) {
        unsafe { lv_tick_inc(ms); }
    }
    pub fn task_handler(&self) {
        unsafe { lv_timer_handler_run_in_period(5); }
    }
    pub fn send_touch(&self, x: i32, y: i32, pressed: bool) {
        // Touch input handled by LVGL indev in main loop
        _ = (x, y, pressed);
    }
}

// ── Flush callback (no-op: framebuffer is direct dumb buffer) ──

unsafe extern "C" fn flush_cb(
    disp: *mut lv_disp_t,
    _area: *const lv_area_t,
    _buf: *mut u8,
    _user_data: *mut c_void,
) {
    lv_display_flush_ready(disp);
}

// ── Build UI ───────────────────────────────────────────────

// Safe cstr! macro: holds CString alive for the enclosing statement
macro_rules! cstr {
    ($s:expr) => {{
        let _cstr_guard = CString::new($s.as_bytes()).unwrap();
        _cstr_guard.as_ptr()
    }};
}

macro_rules! set_label {
    ($label:expr, $fmt:expr $(, $arg:expr)*) => {{
        let _s = format!($fmt $(, $arg)*);
        let _c = CString::new(_s.as_bytes()).unwrap();
        lv_label_set_text($label, _c.as_ptr());
    }};
}

macro_rules! set_label_str {
    ($label:expr, $s:expr) => {{
        let _c = CString::new($s.as_bytes()).unwrap();
        lv_label_set_text($label, _c.as_ptr());
    }};
}

pub fn build_ui() {
    unsafe {
        let screen = lv_screen_active();
        let pw = lv_display_get_horizontal_resolution(DISPLAY);
        let ph = lv_display_get_vertical_resolution(DISPLAY);

        // -- Panel container (2x screen) --
        PANELS_CONTAINER = lv_obj_create(screen);
        lv_obj_set_size(PANELS_CONTAINER, pw * 2, ph);
        lv_obj_set_pos(PANELS_CONTAINER, 0, 0);
        lv_obj_remove_flag(PANELS_CONTAINER, LV_OBJ_FLAG_SCROLLABLE);
        r1_lv_obj_set_style_bg_opa(PANELS_CONTAINER, 0, LV_PART_MAIN);
        r1_lv_obj_set_style_border_width(PANELS_CONTAINER, 0, LV_PART_MAIN);
        r1_lv_obj_set_style_pad_all(PANELS_CONTAINER, 0, LV_PART_MAIN);

        build_panel0(pw, ph);
        build_panel1(pw, ph);

        // Gesture on screen for swipe
        lv_obj_add_event_cb(screen, Some(swipe_cb), LV_EVENT_GESTURE, ptr::null_mut());

        // Modal
        build_modal(pw, ph);

        log::info!("UI built: {}x{}", pw, ph);
    }
}

unsafe fn build_panel0(pw: i32, ph: i32) {
    let bg = r1_lv_color_hex(0x0D1117);
    let card_bg = r1_lv_color_hex(0x161B22);
    let green = r1_lv_color_hex(0x3FB950);
    let amber = r1_lv_color_hex(0xD29922);
    let grey = r1_lv_color_hex(0x30363D);

    let p0 = lv_obj_create(PANELS_CONTAINER);
    lv_obj_set_size(p0, pw, ph);
    lv_obj_set_pos(p0, 0, 0);
    r1_lv_obj_set_style_bg_color(p0, bg, LV_PART_MAIN);
    r1_lv_obj_set_style_border_width(p0, 0, LV_PART_MAIN);
    r1_lv_obj_set_style_pad_all(p0, 12, LV_PART_MAIN);

    // Header
    let header = lv_label_create(p0);
    set_label_str!(header, "📊 概况");
    lv_obj_align(header, LV_ALIGN_TOP_LEFT, 0, 4);

    UP_LABEL = lv_label_create(p0);
    set_label_str!(UP_LABEL, "--");
    lv_obj_align(UP_LABEL, LV_ALIGN_TOP_RIGHT, 0, 4);

    // -- CPU / Memory gauges --
    let gauge_w = (pw - 36) / 2;
    let gauge_y = 36;

    // CPU
    CPU_ARC = lv_arc_create(p0);
    lv_obj_set_size(CPU_ARC, gauge_w, gauge_w);
    lv_obj_set_pos(CPU_ARC, 12, gauge_y);
    lv_arc_set_range(CPU_ARC, 0, 100);
    lv_arc_set_value(CPU_ARC, 0);
    r1_lv_obj_set_style_arc_color(CPU_ARC, green, LV_PART_INDICATOR);
    lv_obj_remove_flag(CPU_ARC, LV_OBJ_FLAG_CLICKABLE);

    CPU_LABEL = lv_label_create(p0);
    set_label_str!(CPU_LABEL, "--%");
    lv_obj_align_to(CPU_LABEL, CPU_ARC, LV_ALIGN_CENTER, 0, -12);

    let cpu_name = lv_label_create(p0);
    set_label_str!(cpu_name, "CPU");
    lv_obj_align_to(cpu_name, CPU_ARC, LV_ALIGN_CENTER, 0, 16);

    CPU_SUB = lv_label_create(p0);
    set_label_str!(CPU_SUB, "--");
    lv_obj_align_to(CPU_SUB, CPU_ARC, LV_ALIGN_OUT_BOTTOM_MID, 0, 4);

    // Memory
    MEM_ARC = lv_arc_create(p0);
    lv_obj_set_size(MEM_ARC, gauge_w, gauge_w);
    lv_obj_set_pos(MEM_ARC, 24 + gauge_w, gauge_y);
    lv_arc_set_range(MEM_ARC, 0, 100);
    lv_arc_set_value(MEM_ARC, 0);
    r1_lv_obj_set_style_arc_color(MEM_ARC, amber, LV_PART_INDICATOR);
    lv_obj_remove_flag(MEM_ARC, LV_OBJ_FLAG_CLICKABLE);

    MEM_LABEL = lv_label_create(p0);
    set_label_str!(MEM_LABEL, "--%");
    lv_obj_align_to(MEM_LABEL, MEM_ARC, LV_ALIGN_CENTER, 0, -12);

    let mem_name = lv_label_create(p0);
    set_label_str!(mem_name, "内存");
    lv_obj_align_to(mem_name, MEM_ARC, LV_ALIGN_CENTER, 0, 16);

    MEM_SUB = lv_label_create(p0);
    set_label_str!(MEM_SUB, "--");
    lv_obj_align_to(MEM_SUB, MEM_ARC, LV_ALIGN_OUT_BOTTOM_MID, 0, 4);

    // -- Network --
    let net_y = gauge_y + gauge_w + 12;
    let net = lv_obj_create(p0);
    lv_obj_set_size(net, pw - 24, 72);
    lv_obj_set_pos(net, 12, net_y);
    r1_lv_obj_set_style_bg_color(net, card_bg, LV_PART_MAIN);
    r1_lv_obj_set_style_border_width(net, 0, LV_PART_MAIN);
    r1_lv_obj_set_style_radius(net, 8, LV_PART_MAIN);
    r1_lv_obj_set_style_pad_all(net, 8, LV_PART_MAIN);

    NET_LABEL = lv_label_create(net);
    set_label_str!(NET_LABEL, "🌐 --");
    lv_obj_align(NET_LABEL, LV_ALIGN_TOP_LEFT, 0, 0);

    RX_LABEL = lv_label_create(net);
    set_label_str!(RX_LABEL, "↓ 0");
    lv_obj_align(RX_LABEL, LV_ALIGN_TOP_LEFT, 0, 22);

    TX_LABEL = lv_label_create(net);
    set_label_str!(TX_LABEL, "↑ 0");
    lv_obj_align(TX_LABEL, LV_ALIGN_TOP_LEFT, 120, 22);

    IP_LABEL = lv_label_create(net);
    set_label_str!(IP_LABEL, "--");
    lv_obj_align(IP_LABEL, LV_ALIGN_BOTTOM_LEFT, 0, 0);

    // -- Disk list --
    let disk_y = net_y + 80;
    DISK_LIST = lv_obj_create(p0);
    lv_obj_set_size(DISK_LIST, pw - 24, ph - disk_y - 12);
    lv_obj_set_pos(DISK_LIST, 12, disk_y);
    r1_lv_obj_set_style_bg_opa(DISK_LIST, 0, LV_PART_MAIN);
    r1_lv_obj_set_style_border_width(DISK_LIST, 0, LV_PART_MAIN);
    r1_lv_obj_set_style_pad_all(DISK_LIST, 0, LV_PART_MAIN);
    r1_lv_obj_set_flex_flow(DISK_LIST, LV_FLEX_FLOW_COLUMN);
    r1_lv_obj_set_flex_align(DISK_LIST, LV_FLEX_ALIGN_START, LV_FLEX_ALIGN_CENTER, LV_FLEX_ALIGN_START);
    r1_lv_obj_set_style_pad_row(DISK_LIST, 4, LV_PART_MAIN);
}

unsafe fn build_panel1(pw: i32, ph: i32) {
    let bg = r1_lv_color_hex(0x0D1117);
    let card_bg = r1_lv_color_hex(0x161B22);

    let p1 = lv_obj_create(PANELS_CONTAINER);
    lv_obj_set_size(p1, pw, ph);
    lv_obj_set_pos(p1, pw, 0); // offset to right
    r1_lv_obj_set_style_bg_color(p1, bg, LV_PART_MAIN);
    r1_lv_obj_set_style_border_width(p1, 0, LV_PART_MAIN);
    r1_lv_obj_set_style_pad_all(p1, 12, LV_PART_MAIN);

    let header = lv_label_create(p1);
    set_label_str!(header, "⚙ 服务");
    lv_obj_align(header, LV_ALIGN_TOP_LEFT, 0, 4);

    // Services
    let svc_hdr = lv_label_create(p1);
    set_label_str!(svc_hdr, "🛠 核心服务");
    lv_obj_align(svc_hdr, LV_ALIGN_TOP_LEFT, 0, 30);

    SVC_LIST = lv_obj_create(p1);
    lv_obj_set_size(SVC_LIST, pw - 24, 100);
    lv_obj_align_to(SVC_LIST, svc_hdr, LV_ALIGN_OUT_BOTTOM_LEFT, 0, 4);
    r1_lv_obj_set_style_bg_opa(SVC_LIST, 0, LV_PART_MAIN);
    r1_lv_obj_set_style_border_width(SVC_LIST, 0, LV_PART_MAIN);
    r1_lv_obj_set_flex_flow(SVC_LIST, LV_FLEX_FLOW_COLUMN);

    // Docker
    let docker_hdr = lv_label_create(p1);
    set_label_str!(docker_hdr, "🐳 Docker");
    lv_obj_align_to(docker_hdr, SVC_LIST, LV_ALIGN_OUT_BOTTOM_LEFT, 0, 12);

    DOCKER_LIST = lv_obj_create(p1);
    lv_obj_set_size(DOCKER_LIST, pw - 24, 140);
    lv_obj_align_to(DOCKER_LIST, docker_hdr, LV_ALIGN_OUT_BOTTOM_LEFT, 0, 4);
    r1_lv_obj_set_style_bg_opa(DOCKER_LIST, 0, LV_PART_MAIN);
    r1_lv_obj_set_style_border_width(DOCKER_LIST, 0, LV_PART_MAIN);
    r1_lv_obj_set_flex_flow(DOCKER_LIST, LV_FLEX_FLOW_COLUMN);

    // VMs
    let vm_hdr = lv_label_create(p1);
    set_label_str!(vm_hdr, "🖥 虚拟机");
    lv_obj_align_to(vm_hdr, DOCKER_LIST, LV_ALIGN_OUT_BOTTOM_LEFT, 0, 12);

    VM_LIST = lv_obj_create(p1);
    lv_obj_set_size(VM_LIST, pw - 24, 100);
    lv_obj_align_to(VM_LIST, vm_hdr, LV_ALIGN_OUT_BOTTOM_LEFT, 0, 4);
    r1_lv_obj_set_style_bg_opa(VM_LIST, 0, LV_PART_MAIN);
    r1_lv_obj_set_style_border_width(VM_LIST, 0, LV_PART_MAIN);
    r1_lv_obj_set_flex_flow(VM_LIST, LV_FLEX_FLOW_COLUMN);

    // Power buttons
    let reboot_btn = lv_button_create(p1);
    lv_obj_set_size(reboot_btn, pw - 24, 44);
    lv_obj_align(reboot_btn, LV_ALIGN_BOTTOM_MID, 0, -56);
    r1_lv_obj_set_style_bg_color(reboot_btn, r1_lv_color_hex(0x21262D), LV_PART_MAIN);
    lv_obj_add_event_cb(reboot_btn, Some(reboot_confirm_cb), LV_EVENT_CLICKED, ptr::null_mut());
    let rl = lv_label_create(reboot_btn);
    set_label_str!(rl, "🔄 重启");
    lv_obj_center(rl);

    let shutdown_btn = lv_button_create(p1);
    lv_obj_set_size(shutdown_btn, pw - 24, 44);
    lv_obj_align(shutdown_btn, LV_ALIGN_BOTTOM_MID, 0, -4);
    r1_lv_obj_set_style_bg_color(shutdown_btn, r1_lv_color_hex(0xDA3633), LV_PART_MAIN);
    lv_obj_add_event_cb(shutdown_btn, Some(shutdown_confirm_cb), LV_EVENT_CLICKED, ptr::null_mut());
    let sl = lv_label_create(shutdown_btn);
    set_label_str!(sl, "⏻ 关机");
    lv_obj_center(sl);
}

unsafe fn build_modal(pw: i32, ph: i32) {
    MODAL_BG = lv_obj_create(SCREEN);
    lv_obj_set_size(MODAL_BG, pw, ph);
    lv_obj_set_pos(MODAL_BG, 0, 0);
    r1_lv_obj_set_style_bg_color(MODAL_BG, r1_lv_color_hex(0x000000), LV_PART_MAIN);
    r1_lv_obj_set_style_bg_opa(MODAL_BG, 180, LV_PART_MAIN);
    r1_lv_obj_set_style_border_width(MODAL_BG, 0, LV_PART_MAIN);
    lv_obj_add_flag(MODAL_BG, LV_OBJ_FLAG_HIDDEN);

    let box_w = 280;
    let box_h = 160;
    let modal = lv_obj_create(MODAL_BG);
    lv_obj_set_size(modal, box_w, box_h);
    lv_obj_center(modal);
    r1_lv_obj_set_style_bg_color(modal, r1_lv_color_hex(0x161B22), LV_PART_MAIN);
    r1_lv_obj_set_style_radius(modal, 12, LV_PART_MAIN);
    r1_lv_obj_set_style_border_width(modal, 0, LV_PART_MAIN);
    r1_lv_obj_set_style_pad_all(modal, 16, LV_PART_MAIN);

    MODAL_MSG = lv_label_create(modal);
    set_label_str!(MODAL_MSG, "");
    lv_obj_align(MODAL_MSG, LV_ALIGN_TOP_MID, 0, 8);

    MODAL_CANCEL = lv_button_create(modal);
    lv_obj_set_size(MODAL_CANCEL, 100, 36);
    lv_obj_align(MODAL_CANCEL, LV_ALIGN_BOTTOM_LEFT, 8, -8);
    lv_obj_add_event_cb(MODAL_CANCEL, Some(hide_modal_cb), LV_EVENT_CLICKED, ptr::null_mut());
    let cl = lv_label_create(MODAL_CANCEL);
    set_label_str!(cl, "取消");
    lv_obj_center(cl);

    MODAL_OK = lv_button_create(modal);
    lv_obj_set_size(MODAL_OK, 100, 36);
    lv_obj_align(MODAL_OK, LV_ALIGN_BOTTOM_RIGHT, -8, -8);
    r1_lv_obj_set_style_bg_color(MODAL_OK, r1_lv_color_hex(0xDA3633), LV_PART_MAIN);
    lv_obj_add_event_cb(MODAL_OK, Some(exec_action_cb), LV_EVENT_CLICKED, ptr::null_mut());
    let ol = lv_label_create(MODAL_OK);
    set_label_str!(ol, "确认");
    lv_obj_center(ol);
}

// ── Update ─────────────────────────────────────────────────

pub fn update(data: &SystemData) {
    unsafe {
        // Uptime
        let uptime = cstr!(data.uptime.str.as_str());
        lv_label_set_text(UP_LABEL, uptime);

        // CPU
        let cpu_pct = data.cpu.percent as i32;
        lv_arc_set_value(CPU_ARC, cpu_pct);
        let cpu_text = cstr!(format!("{}%", cpu_pct));
        lv_label_set_text(CPU_LABEL, cpu_text);
        let sub = format!(
            "{}{}",
            data.cpu.temperature_c.map_or("--".into(), |t| format!("{:.0}℃", t)),
            data.cpu.freq_mhz.map_or(String::new(), |f| format!(" · {:.1}G", f / 1000.0))
        );
        let sub = cstr!(sub);
        lv_label_set_text(CPU_SUB, sub);

        // Memory
        let mem_pct = data.memory.percent as i32;
        lv_arc_set_value(MEM_ARC, mem_pct);
        let mem_text = cstr!(format!("{}%", mem_pct));
        lv_label_set_text(MEM_LABEL, mem_text);
        let mem_sub = cstr!(format!("{:.1}G / {:.1}G", data.memory.used_gb, data.memory.total_gb));
        lv_label_set_text(MEM_SUB, mem_sub);

        // Network
        if let Some(net) = pick_net(&data.network) {
            let txt = cstr!(format!("🌐 {}", net.name));
            lv_label_set_text(NET_LABEL, txt);
            let rx = cstr!(format!("↓ {}", format_speed(net.rx_speed)));
            lv_label_set_text(RX_LABEL, rx);
            let tx = cstr!(format!("↑ {}", format_speed(net.tx_speed)));
            lv_label_set_text(TX_LABEL, tx);
            let ip = if net.ipv4.is_empty() { "--".into() } else { net.ipv4.join(", ") };
            let ip = cstr!(ip);
            lv_label_set_text(IP_LABEL, ip);
        }

        // Disks / Docker / VMs / Services
        update_disk_list(&data.disks);
        update_item_list(DOCKER_LIST, &data.docker, |c| {
            (c.names.clone(), c.status.clone(), c.state == "running")
        });
        update_item_list(VM_LIST, &data.vms, |v| {
            (v.name.clone(), format!("ID {}", v.id), v.state == "running")
        });
        update_svc_list(&data.services);
    }
}

unsafe fn update_disk_list(disks: &[crate::metrics::DiskHealth]) {
    lv_obj_clean(DISK_LIST);
    let green = r1_lv_color_hex(0x3FB950);
    let amber = r1_lv_color_hex(0xD29922);
    let red = r1_lv_color_hex(0xDA3633);
    let card_bg = r1_lv_color_hex(0x161B22);

    for disk in disks {
        let card = lv_obj_create(DISK_LIST);
        lv_obj_set_size(card, lv_obj_get_width(DISK_LIST) - 8, 68);
        r1_lv_obj_set_style_bg_color(card, card_bg, LV_PART_MAIN);
        r1_lv_obj_set_style_radius(card, 6, LV_PART_MAIN);
        r1_lv_obj_set_style_border_width(card, 0, LV_PART_MAIN);
        r1_lv_obj_set_style_pad_all(card, 6, LV_PART_MAIN);

        // Name + size
        let name = cstr!(format!("{}  {}", disk.model, disk.size));
        let label = lv_label_create(card);
        lv_label_set_text(label, name);
        lv_obj_align(label, LV_ALIGN_TOP_LEFT, 0, 0);

        // Meta info
        let mut meta = String::new();
        if let Some(h) = disk.power_on_hours { meta.push_str(&format!("{:.0}h · ", h)); }
        if let Some(t) = disk.temperature { meta.push_str(&format!("{:.0}℃ · ", t)); }
        if let Some(w) = disk.percent_used { meta.push_str(&format!("损耗{:.1}%", w)); }
        let meta = cstr!(meta);
        let sub = lv_label_create(card);
        lv_label_set_text(sub, meta);
        lv_obj_align(sub, LV_ALIGN_BOTTOM_LEFT, 0, -20);

        // Usage bar
        if let Some(mt) = disk.mounts.first() {
            let bar = lv_bar_create(card);
            lv_obj_set_size(bar, lv_obj_get_width(card) - 16, 8);
            lv_obj_align(bar, LV_ALIGN_BOTTOM_MID, 0, -4);
            lv_bar_set_range(bar, 0, 100);
            lv_bar_set_value(bar, mt.percent as i32, LV_ANIM_OFF);
            let bar_color = if mt.percent >= 90.0 { red } else if mt.percent >= 75.0 { amber } else { green };
            r1_lv_obj_set_style_bg_color(bar, bar_color, LV_PART_INDICATOR);
        }
    }
}

unsafe fn update_item_list<T>(
    list: *mut lv_obj_t,
    items: &[T],
    display: impl Fn(&T) -> (String, String, bool),
) {
    lv_obj_clean(list);
    let green_bg = r1_lv_color_hex(0x1B3D1F);
    let red_bg = r1_lv_color_hex(0x3D1B1B);

    for item in items {
        let (name, _detail, active) = display(item);
        let row = lv_obj_create(list);
        lv_obj_set_size(row, lv_obj_get_width(list) - 8, 32);
        r1_lv_obj_set_style_bg_opa(row, 0, LV_PART_MAIN);
        r1_lv_obj_set_style_border_width(row, 0, LV_PART_MAIN);
        r1_lv_obj_set_flex_flow(row, LV_FLEX_FLOW_ROW);
        r1_lv_obj_set_flex_align(row, LV_FLEX_ALIGN_SPACE_BETWEEN, LV_FLEX_ALIGN_CENTER, LV_FLEX_ALIGN_CENTER);

        let nl = lv_label_create(row);
        set_label_str!(nl, name);

        let status = if active { "运行" } else { "停止" };
        let badge = lv_obj_create(row);
        lv_obj_set_size(badge, 56, 24);
        r1_lv_obj_set_style_bg_color(badge, if active { green_bg } else { red_bg }, LV_PART_MAIN);
        r1_lv_obj_set_style_radius(badge, 4, LV_PART_MAIN);
        r1_lv_obj_set_style_border_width(badge, 0, LV_PART_MAIN);
        let bl = lv_label_create(badge);
        set_label_str!(bl, status);
        lv_obj_center(bl);
    }
}

unsafe fn update_svc_list(svcs: &[crate::metrics::ServiceStatus]) {
    lv_obj_clean(SVC_LIST);
    let green_bg = r1_lv_color_hex(0x1B3D1F);
    let red_bg = r1_lv_color_hex(0x3D1B1B);

    for svc in svcs {
        let row = lv_obj_create(SVC_LIST);
        lv_obj_set_size(row, lv_obj_get_width(SVC_LIST) - 8, 28);
        r1_lv_obj_set_style_bg_opa(row, 0, LV_PART_MAIN);
        r1_lv_obj_set_style_border_width(row, 0, LV_PART_MAIN);
        r1_lv_obj_set_flex_flow(row, LV_FLEX_FLOW_ROW);
        r1_lv_obj_set_flex_align(row, LV_FLEX_ALIGN_SPACE_BETWEEN, LV_FLEX_ALIGN_CENTER, LV_FLEX_ALIGN_CENTER);

        let nl = lv_label_create(row);
        set_label_str!(nl, svc.name.as_str());

        let status = if svc.active { "活跃" } else { "停用" };
        let badge = lv_obj_create(row);
        lv_obj_set_size(badge, 48, 20);
        r1_lv_obj_set_style_radius(badge, 4, LV_PART_MAIN);
        r1_lv_obj_set_style_bg_color(badge, if svc.active { green_bg } else { red_bg }, LV_PART_MAIN);
        r1_lv_obj_set_style_border_width(badge, 0, LV_PART_MAIN);
        let bl = lv_label_create(badge);
        set_label_str!(bl, status);
        lv_obj_center(bl);
    }
}

// ── Helpers ────────────────────────────────────────────────

fn pick_net(nets: &[crate::metrics::NetworkIface]) -> Option<&crate::metrics::NetworkIface> {
    nets.iter()
        .filter(|n| n.is_up && !n.ipv4.is_empty() && !n.name.starts_with("lo") && !n.name.starts_with("vnet") && !n.name.contains("ovs"))
        .max_by_key(|n| n.rx_bytes + n.tx_bytes)
}

fn format_speed(bps: f64) -> String {
    if bps < 1.0 { "0".into() }
    else if bps >= 1e9 { format!("{:.1}G", bps / 1e9) }
    else if bps >= 1e6 { format!("{:.1}M", bps / 1e6) }
    else if bps >= 1e3 { format!("{:.0}K", bps / 1e3) }
    else { format!("{:.0}B", bps) }
}

// ── Event callbacks ────────────────────────────────────────

unsafe extern "C" fn swipe_cb(e: *mut lv_event_t) {
    let indev = lv_event_get_indev(e);
    let dir = lv_indev_get_gesture_dir(indev);
    let pw = lv_display_get_horizontal_resolution(DISPLAY);

    if dir == LV_DIR_LEFT && CURRENT_PANEL == 0 {
        CURRENT_PANEL = 1;
        lv_obj_set_x(PANELS_CONTAINER, -pw);
    } else if dir == LV_DIR_RIGHT && CURRENT_PANEL == 1 {
        CURRENT_PANEL = 0;
        lv_obj_set_x(PANELS_CONTAINER, 0);
    }
}

unsafe extern "C" fn reboot_confirm_cb(_e: *mut lv_event_t) {
    PENDING_ACTION = Action::Reboot;
    show_modal("确定重启 NAS？\n服务将暂时中断。");
}

unsafe extern "C" fn shutdown_confirm_cb(_e: *mut lv_event_t) {
    PENDING_ACTION = Action::Shutdown;
    show_modal("确定关闭 NAS？\n需手动开机恢复。");
}

unsafe fn show_modal(msg: &str) {
    set_label_str!(MODAL_MSG, msg);
    lv_obj_remove_flag(MODAL_BG, LV_OBJ_FLAG_HIDDEN);
}

unsafe extern "C" fn hide_modal_cb(_e: *mut lv_event_t) {
    lv_obj_add_flag(MODAL_BG, LV_OBJ_FLAG_HIDDEN);
    PENDING_ACTION = Action::None;
}

unsafe extern "C" fn exec_action_cb(_e: *mut lv_event_t) {
    match PENDING_ACTION {
        Action::Reboot => {
            set_label_str!(MODAL_MSG, "重启中...");
            lv_obj_add_flag(MODAL_CANCEL, LV_OBJ_FLAG_HIDDEN);
            lv_obj_add_flag(MODAL_OK, LV_OBJ_FLAG_HIDDEN);
            std::process::Command::new("sync").output().ok();
            std::thread::sleep(std::time::Duration::from_millis(200));
            std::process::Command::new("systemctl").args(["reboot", "--force"]).output().ok();
        }
        Action::Shutdown => {
            set_label_str!(MODAL_MSG, "关机中...");
            lv_obj_add_flag(MODAL_CANCEL, LV_OBJ_FLAG_HIDDEN);
            lv_obj_add_flag(MODAL_OK, LV_OBJ_FLAG_HIDDEN);
            std::process::Command::new("sync").output().ok();
            std::thread::sleep(std::time::Duration::from_millis(200));
            std::process::Command::new("systemctl").args(["poweroff", "--force"]).output().ok();
        }
        Action::None => {}
    }
}
