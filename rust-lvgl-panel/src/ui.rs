// LVGL UI — 2-panel swipeable interface.
// Panel 0: Overview (CPU, Memory, Network, Disks)
// Panel 1: Detail (Services, Docker, VMs, Power actions)

use crate::metrics::SystemData;

// ── LVGL bootstrap ─────────────────────────────────────────

pub struct LvglHandle;

pub fn init_lvgl(width: u32, height: u32, fb_ptr: *mut u8) -> LvglHandle {
    // Initialize LVGL
    unsafe {
        lv_init();
    }

    // Set up display driver (software renderer writes to our DRM framebuffer)
    let mut disp_buf = LvDispBuf {
        buf1: fb_ptr as *mut lv_color_t,
        size: (width * height) as usize,
    };

    unsafe {
        lv_disp_drv_init(&mut DISP_DRV);
        DISP_DRV.hor_res = width as i32;
        DISP_DRV.ver_res = height as i32;
        DISP_DRV.flush_cb = Some(flush_cb);
        DISP_DRV.draw_buf = &mut disp_buf as *mut _ as *mut lv_disp_draw_buf_t;
        lv_disp_drv_register(&mut DISP_DRV);
    }

    // Set up theme
    set_dark_theme();

    log::info!("LVGL initialized: {}x{}", width, height);
    LvglHandle
}

impl LvglHandle {
    pub fn tick(&self, ms: u32) {
        unsafe { lv_tick_inc(ms); }
    }
    pub fn task_handler(&self) {
        unsafe { lv_task_handler(); }
    }
    pub fn send_touch(&self, x: i32, y: i32, pressed: bool) {
        unsafe {
            lv_mouse_set_cursor_pos(x as u16, y as u16);
            if pressed {
                lv_mouse_press();
            } else {
                lv_mouse_release();
            }
        }
    }
}

// ── UI Structure ───────────────────────────────────────────

static mut DISP_DRV: lv_disp_drv_t = lv_disp_drv_t::zeroed();

// Top-level containers
static mut SCREEN: *mut lv_obj_t = ptr::null_mut();
static mut PANELS_CONTAINER: *mut lv_obj_t = ptr::null_mut();
static mut PANEL0: *mut lv_obj_t = ptr::null_mut();
static mut PANEL1: *mut lv_obj_t = ptr::null_mut();
static mut DOT_INDICATOR: *mut lv_obj_t = ptr::null_mut();
static mut MODAL_BG: *mut lv_obj_t = ptr::null_mut();
static mut MODAL_MSG: *mut lv_obj_t = ptr::null_mut();
static mut MODAL_OK: *mut lv_obj_t = ptr::null_mut();
static mut MODAL_CANCEL: *mut lv_obj_t = ptr::null_mut();

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

static mut CURRENT_PANEL: i32 = 0;

// ── Build UI ───────────────────────────────────────────────

pub fn build_ui() {
    unsafe {
        SCREEN = lv_scr_act();

        // -- Panel container (2x screen width for swipe) --
        let pw = lv_disp_get_hor_res(ptr::null_mut()) as i32;
        let ph = lv_disp_get_ver_res(ptr::null_mut()) as i32;

        PANELS_CONTAINER = lv_obj_create(SCREEN);
        lv_obj_set_size(PANELS_CONTAINER, pw * 2, ph);
        lv_obj_set_pos(PANELS_CONTAINER, 0, 0);
        lv_obj_clear_flag(PANELS_CONTAINER, LV_OBJ_FLAG_SCROLLABLE as u32);
        lv_obj_set_style_bg_opa(PANELS_CONTAINER, 0, 0);
        lv_obj_set_style_border_width(PANELS_CONTAINER, 0, 0);
        lv_obj_set_style_pad_all(PANELS_CONTAINER, 0, 0);

        build_panel0(pw, ph);
        build_panel1(pw, ph);

        // -- Dot indicator --
        build_dots(pw);

        // -- Modal (hidden initially) --
        build_modal(pw, ph);

        // -- Swipe gesture on screen --
        lv_obj_add_event_cb(SCREEN, Some(swipe_cb), LV_EVENT_GESTURE as u32, ptr::null_mut());

        log::info!("UI built: {}x{}", pw, ph);
    }
}

unsafe fn build_panel0(pw: i32, ph: i32) {
    PANEL0 = lv_obj_create(PANELS_CONTAINER);
    lv_obj_set_size(PANEL0, pw, ph);
    lv_obj_set_pos(PANEL0, 0, 0);
    lv_obj_set_style_bg_color(PANEL0, lv_color_hex(0x0D1117), 0);
    lv_obj_set_style_border_width(PANEL0, 0, 0);
    lv_obj_set_style_pad_all(PANEL0, 12, 0);

    // -- Header --
    let header = lv_label_create(PANEL0);
    lv_label_set_text(header, c"📊 概况".as_ptr());
    lv_obj_align(header, LV_ALIGN_TOP_LEFT as u32, 0, 4);

    UP_LABEL = lv_label_create(PANEL0);
    lv_label_set_text(UP_LABEL, c"--".as_ptr());
    lv_obj_align(UP_LABEL, LV_ALIGN_TOP_RIGHT as u32, 0, 4);

    // -- CPU / Memory gauges (top row, 2 columns) --
    let gauge_w = (pw - 36) / 2;
    let gauge_y = 36;
    let gauge_h = 180;

    // CPU gauge
    let cpu_cont = lv_obj_create(PANEL0);
    lv_obj_set_size(cpu_cont, gauge_w, gauge_h);
    lv_obj_set_pos(cpu_cont, 12, gauge_y);
    lv_obj_set_style_bg_opa(cpu_cont, 0, 0);
    lv_obj_set_style_border_width(cpu_cont, 0, 0);
    lv_obj_set_style_pad_all(cpu_cont, 0, 0);

    CPU_ARC = lv_arc_create(cpu_cont);
    lv_obj_set_size(CPU_ARC, gauge_w - 20, gauge_h - 40);
    lv_obj_center(CPU_ARC);
    lv_arc_set_range(CPU_ARC, 0, 100);
    lv_arc_set_value(CPU_ARC, 0);
    lv_obj_set_style_arc_color(CPU_ARC, lv_color_hex(0x3FB950), 0);
    lv_obj_set_style_arc_color(CPU_ARC, lv_color_hex(0x3FB950), LV_PART_INDICATOR as u32);

    CPU_LABEL = lv_label_create(cpu_cont);
    lv_label_set_text(CPU_LABEL, c"--%".as_ptr());
    lv_obj_align(CPU_LABEL, LV_ALIGN_CENTER as u32, 0, -8);
    lv_obj_set_style_text_font(CPU_LABEL, &lv_font_montserrat_24 as *const _ as *const c_void, 0);

    let cpu_name = lv_label_create(cpu_cont);
    lv_label_set_text(cpu_name, c"CPU".as_ptr());
    lv_obj_align(cpu_name, LV_ALIGN_CENTER as u32, 0, 20);

    CPU_SUB = lv_label_create(cpu_cont);
    lv_label_set_text(CPU_SUB, c"--".as_ptr());
    lv_obj_align(CPU_SUB, LV_ALIGN_BOTTOM_MID as u32, 0, 0);

    // Memory gauge (same layout)
    let mem_cont = lv_obj_create(PANEL0);
    lv_obj_set_size(mem_cont, gauge_w, gauge_h);
    lv_obj_set_pos(mem_cont, 24 + gauge_w, gauge_y);
    lv_obj_set_style_bg_opa(mem_cont, 0, 0);
    lv_obj_set_style_border_width(mem_cont, 0, 0);
    lv_obj_set_style_pad_all(mem_cont, 0, 0);

    MEM_ARC = lv_arc_create(mem_cont);
    lv_obj_set_size(MEM_ARC, gauge_w - 20, gauge_h - 40);
    lv_obj_center(MEM_ARC);
    lv_arc_set_range(MEM_ARC, 0, 100);
    lv_arc_set_value(MEM_ARC, 0);
    lv_obj_set_style_arc_color(MEM_ARC, lv_color_hex(0xD29922), LV_PART_INDICATOR as u32);

    MEM_LABEL = lv_label_create(mem_cont);
    lv_label_set_text(MEM_LABEL, c"--%".as_ptr());
    lv_obj_align(MEM_LABEL, LV_ALIGN_CENTER as u32, 0, -8);
    lv_obj_set_style_text_font(MEM_LABEL, &lv_font_montserrat_24 as *const _ as *const c_void, 0);

    let mem_name = lv_label_create(mem_cont);
    lv_label_set_text(mem_name, c"内存".as_ptr());
    lv_obj_align(mem_name, LV_ALIGN_CENTER as u32, 0, 20);

    MEM_SUB = lv_label_create(mem_cont);
    lv_label_set_text(MEM_SUB, c"--".as_ptr());
    lv_obj_align(MEM_SUB, LV_ALIGN_BOTTOM_MID as u32, 0, 0);

    // -- Network section --
    let net_y = gauge_y + gauge_h + 8;
    let net = lv_obj_create(PANEL0);
    lv_obj_set_size(net, pw - 24, 70);
    lv_obj_set_pos(net, 12, net_y);
    lv_obj_set_style_bg_color(net, lv_color_hex(0x161B22), 0);
    lv_obj_set_style_border_width(net, 0, 0);
    lv_obj_set_style_radius(net, 8, 0);
    lv_obj_set_style_pad_all(net, 8, 0);

    NET_LABEL = lv_label_create(net);
    lv_label_set_text(NET_LABEL, c"🌐 --".as_ptr());
    lv_obj_align(NET_LABEL, LV_ALIGN_TOP_LEFT as u32, 0, 0);

    RX_LABEL = lv_label_create(net);
    lv_label_set_text(RX_LABEL, c"↓ 0".as_ptr());
    lv_obj_align(RX_LABEL, LV_ALIGN_TOP_LEFT as u32, 0, 22);

    TX_LABEL = lv_label_create(net);
    lv_label_set_text(TX_LABEL, c"↑ 0".as_ptr());
    lv_obj_align(TX_LABEL, LV_ALIGN_TOP_LEFT as u32, 120, 22);

    IP_LABEL = lv_label_create(net);
    lv_label_set_text(IP_LABEL, c"--".as_ptr());
    lv_obj_align(IP_LABEL, LV_ALIGN_BOTTOM_LEFT as u32, 0, 0);

    // -- Disk list --
    let disk_y = net_y + 78;
    DISK_LIST = lv_obj_create(PANEL0);
    lv_obj_set_size(DISK_LIST, pw - 24, ph - disk_y - 16);
    lv_obj_set_pos(DISK_LIST, 12, disk_y);
    lv_obj_set_style_bg_opa(DISK_LIST, 0, 0);
    lv_obj_set_style_border_width(DISK_LIST, 0, 0);
    lv_obj_set_style_pad_all(DISK_LIST, 0, 0);
    lv_obj_set_flex_flow(DISK_LIST, LV_FLEX_FLOW_COLUMN as u32);
    lv_obj_set_flex_align(DISK_LIST, LV_FLEX_ALIGN_START as u32, LV_FLEX_ALIGN_CENTER as u32, LV_FLEX_ALIGN_START as u32);
    lv_obj_set_style_pad_row(DISK_LIST, 4, 0);
}

unsafe fn build_panel1(pw: i32, ph: i32) {
    PANEL1 = lv_obj_create(PANELS_CONTAINER);
    lv_obj_set_size(PANEL1, pw, ph);
    lv_obj_set_pos(PANEL1, pw, 0); // offset to the right
    lv_obj_set_style_bg_color(PANEL1, lv_color_hex(0x0D1117), 0);
    lv_obj_set_style_border_width(PANEL1, 0, 0);
    lv_obj_set_style_pad_all(PANEL1, 12, 0);

    // -- Header --
    let header = lv_label_create(PANEL1);
    lv_label_set_text(header, c"⚙ 服务".as_ptr());
    lv_obj_align(header, LV_ALIGN_TOP_LEFT as u32, 0, 4);

    // -- Section: Core Services --
    let svc_hdr = lv_label_create(PANEL1);
    lv_label_set_text(svc_hdr, c"🛠 核心服务".as_ptr());
    lv_obj_align(svc_hdr, LV_ALIGN_TOP_LEFT as u32, 0, 30);

    SVC_LIST = lv_obj_create(PANEL1);
    lv_obj_set_size(SVC_LIST, pw - 24, 120);
    lv_obj_align_to(SVC_LIST, svc_hdr, LV_ALIGN_OUT_BOTTOM_LEFT as u32, 0, 4);
    lv_obj_set_style_bg_opa(SVC_LIST, 0, 0);
    lv_obj_set_style_border_width(SVC_LIST, 0, 0);
    lv_obj_set_style_pad_all(SVC_LIST, 0, 0);
    lv_obj_set_flex_flow(SVC_LIST, LV_FLEX_FLOW_COLUMN as u32);

    // -- Section: Docker --
    let docker_hdr = lv_label_create(PANEL1);
    lv_label_set_text(docker_hdr, c"🐳 Docker".as_ptr());
    lv_obj_align_to(docker_hdr, SVC_LIST, LV_ALIGN_OUT_BOTTOM_LEFT as u32, 0, 12);

    DOCKER_LIST = lv_obj_create(PANEL1);
    lv_obj_set_size(DOCKER_LIST, pw - 24, 160);
    lv_obj_align_to(DOCKER_LIST, docker_hdr, LV_ALIGN_OUT_BOTTOM_LEFT as u32, 0, 4);
    lv_obj_set_style_bg_opa(DOCKER_LIST, 0, 0);
    lv_obj_set_style_border_width(DOCKER_LIST, 0, 0);
    lv_obj_set_style_pad_all(DOCKER_LIST, 0, 0);
    lv_obj_set_flex_flow(DOCKER_LIST, LV_FLEX_FLOW_COLUMN as u32);

    // -- Section: VMs --
    let vm_hdr = lv_label_create(PANEL1);
    lv_label_set_text(vm_hdr, c"🖥 虚拟机".as_ptr());
    lv_obj_align_to(vm_hdr, DOCKER_LIST, LV_ALIGN_OUT_BOTTOM_LEFT as u32, 0, 12);

    VM_LIST = lv_obj_create(PANEL1);
    lv_obj_set_size(VM_LIST, pw - 24, 120);
    lv_obj_align_to(VM_LIST, vm_hdr, LV_ALIGN_OUT_BOTTOM_LEFT as u32, 0, 4);
    lv_obj_set_style_bg_opa(VM_LIST, 0, 0);
    lv_obj_set_style_border_width(VM_LIST, 0, 0);
    lv_obj_set_style_pad_all(VM_LIST, 0, 0);
    lv_obj_set_flex_flow(VM_LIST, LV_FLEX_FLOW_COLUMN as u32);

    // -- Power buttons --
    let reboot_btn = lv_btn_create(PANEL1);
    lv_obj_set_size(reboot_btn, pw - 24, 42);
    lv_obj_align(reboot_btn, LV_ALIGN_BOTTOM_MID as u32, -60, -8);
    lv_obj_set_style_bg_color(reboot_btn, lv_color_hex(0x21262D), 0);
    let reboot_label = lv_label_create(reboot_btn);
    lv_label_set_text(reboot_label, c"🔄 重启".as_ptr());
    lv_obj_center(reboot_label);
    lv_obj_add_event_cb(reboot_btn, Some(reboot_confirm_cb), LV_EVENT_CLICKED as u32, ptr::null_mut());

    let shutdown_btn = lv_btn_create(PANEL1);
    lv_obj_set_size(shutdown_btn, pw - 24, 42);
    lv_obj_align(shutdown_btn, LV_ALIGN_BOTTOM_MID as u32, -60, 44);
    lv_obj_set_style_bg_color(shutdown_btn, lv_color_hex(0xDA3633), 0);
    let shutdown_label = lv_label_create(shutdown_btn);
    lv_label_set_text(shutdown_label, c"⏻ 关机".as_ptr());
    lv_obj_center(shutdown_label);
    lv_obj_add_event_cb(shutdown_btn, Some(shutdown_confirm_cb), LV_EVENT_CLICKED as u32, ptr::null_mut());
}

unsafe fn build_dots(pw: i32) {
    DOT_INDICATOR = lv_obj_create(SCREEN);
    lv_obj_set_size(DOT_INDICATOR, pw, 14);
    lv_obj_align(DOT_INDICATOR, LV_ALIGN_BOTTOM_MID as u32, 0, 0);
    lv_obj_set_style_bg_opa(DOT_INDICATOR, 0, 0);
    lv_obj_set_style_border_width(DOT_INDICATOR, 0, 0);
    lv_obj_set_flex_flow(DOT_INDICATOR, LV_FLEX_FLOW_ROW as u32);
    lv_obj_set_flex_align(DOT_INDICATOR, LV_FLEX_ALIGN_CENTER as u32, LV_FLEX_ALIGN_CENTER as u32, LV_FLEX_ALIGN_CENTER as u32);
    lv_obj_set_style_pad_column(DOT_INDICATOR, 8, 0);

    // Two dots
    for i in 0..2u32 {
        let dot = lv_obj_create(DOT_INDICATOR);
        lv_obj_set_size(dot, 8, 8);
        lv_obj_set_style_radius(dot, 4, 0);
        lv_obj_set_style_bg_color(dot, if i == 0 { lv_color_hex(0x3FB950) } else { lv_color_hex(0x30363D) }, 0);
        lv_obj_set_style_border_width(dot, 0, 0);
    }
}

unsafe fn build_modal(pw: i32, ph: i32) {
    MODAL_BG = lv_obj_create(SCREEN);
    lv_obj_set_size(MODAL_BG, pw, ph);
    lv_obj_set_pos(MODAL_BG, 0, 0);
    lv_obj_set_style_bg_color(MODAL_BG, lv_color_hex(0x000000), 0);
    lv_obj_set_style_bg_opa(MODAL_BG, 180, 0);
    lv_obj_set_style_border_width(MODAL_BG, 0, 0);
    lv_obj_add_flag(MODAL_BG, LV_OBJ_FLAG_HIDDEN as u32);

    let modal_box = lv_obj_create(MODAL_BG);
    lv_obj_set_size(modal_box, 280, 160);
    lv_obj_center(modal_box);
    lv_obj_set_style_bg_color(modal_box, lv_color_hex(0x161B22), 0);
    lv_obj_set_style_radius(modal_box, 12, 0);
    lv_obj_set_style_border_width(modal_box, 0, 0);
    lv_obj_set_style_pad_all(modal_box, 16, 0);

    MODAL_MSG = lv_label_create(modal_box);
    lv_label_set_text(MODAL_MSG, c"".as_ptr());
    lv_obj_align(MODAL_MSG, LV_ALIGN_TOP_MID as u32, 0, 8);

    MODAL_CANCEL = lv_btn_create(modal_box);
    lv_obj_set_size(MODAL_CANCEL, 100, 36);
    lv_obj_align(MODAL_CANCEL, LV_ALIGN_BOTTOM_LEFT as u32, 8, -8);
    let cancel_lbl = lv_label_create(MODAL_CANCEL);
    lv_label_set_text(cancel_lbl, c"取消".as_ptr());
    lv_obj_center(cancel_lbl);
    lv_obj_add_event_cb(MODAL_CANCEL, Some(hide_modal_cb), LV_EVENT_CLICKED as u32, ptr::null_mut());

    MODAL_OK = lv_btn_create(modal_box);
    lv_obj_set_size(MODAL_OK, 100, 36);
    lv_obj_align(MODAL_OK, LV_ALIGN_BOTTOM_RIGHT as u32, -8, -8);
    lv_obj_set_style_bg_color(MODAL_OK, lv_color_hex(0xDA3633), 0);
    let ok_lbl = lv_label_create(MODAL_OK);
    lv_label_set_text(ok_lbl, c"确认".as_ptr());
    lv_obj_center(ok_lbl);
    lv_obj_add_event_cb(MODAL_OK, Some(exec_action_cb), LV_EVENT_CLICKED as u32, ptr::null_mut());
}

// ── Data Update ────────────────────────────────────────────

pub fn update(data: &SystemData) {
    unsafe {
        // Uptime
        let uptime_c = std::ffi::CString::new(data.uptime.str.as_str()).unwrap();
        lv_label_set_text(UP_LABEL, uptime_c.as_ptr());

        // CPU gauge
        let cpu_pct = data.cpu.percent as i32;
        lv_arc_set_value(CPU_ARC, cpu_pct);
        let cpu_text = format!("{}%", cpu_pct);
        let cpu_text_c = std::ffi::CString::new(cpu_text).unwrap();
        lv_label_set_text(CPU_LABEL, cpu_text_c.as_ptr());

        let cpu_sub = format!(
            "{}{}",
            data.cpu.temperature_c.map_or("--".to_string(), |t| format!("{:.0}℃", t)),
            data.cpu.freq_mhz.map_or(String::new(), |f| format!(" · {:.1}G", f / 1000.0))
        );
        let cpu_sub_c = std::ffi::CString::new(cpu_sub).unwrap();
        lv_label_set_text(CPU_SUB, cpu_sub_c.as_ptr());

        // Memory gauge
        let mem_pct = data.memory.percent as i32;
        lv_arc_set_value(MEM_ARC, mem_pct);
        let mem_text = format!("{}%", mem_pct);
        let mem_text_c = std::ffi::CString::new(mem_text).unwrap();
        lv_label_set_text(MEM_LABEL, mem_text_c.as_ptr());

        let mem_sub = format!(
            "{:.1}G / {:.1}G",
            data.memory.used_gb, data.memory.total_gb
        );
        let mem_sub_c = std::ffi::CString::new(mem_sub).unwrap();
        lv_label_set_text(MEM_SUB, mem_sub_c.as_ptr());

        // Network — pick primary interface
        if let Some(net) = pick_net(&data.network) {
            let net_text = format!("🌐 {}", net.name);
            let c = std::ffi::CString::new(net_text).unwrap();
            lv_label_set_text(NET_LABEL, c.as_ptr());

            let rx = format!("↓ {}", format_speed(net.rx_speed));
            let c = std::ffi::CString::new(rx).unwrap();
            lv_label_set_text(RX_LABEL, c.as_ptr());

            let tx = format!("↑ {}", format_speed(net.tx_speed));
            let c = std::ffi::CString::new(tx).unwrap();
            lv_label_set_text(TX_LABEL, c.as_ptr());

            let ip = net.ipv4.join(", ");
            let ip = if ip.is_empty() { "--".to_string() } else { ip };
            let c = std::ffi::CString::new(ip).unwrap();
            lv_label_set_text(IP_LABEL, c.as_ptr());
        }

        // Disks — rebuild list if data changed
        update_disk_list(&data.disks);

        // Docker / VMs / Services — rebuild lists
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
    for disk in disks {
        let card = lv_obj_create(DISK_LIST);
        lv_obj_set_size(card, lv_obj_get_width(DISK_LIST) - 8, 72);
        lv_obj_set_style_bg_color(card, lv_color_hex(0x161B22), 0);
        lv_obj_set_style_radius(card, 6, 0);
        lv_obj_set_style_border_width(card, 0, 0);
        lv_obj_set_style_pad_all(card, 6, 0);

        let name = std::ffi::CString::new(format!("{}  {}", disk.model, disk.size)).unwrap();
        let label = lv_label_create(card);
        lv_label_set_text(label, name.as_ptr());
        lv_obj_align(label, LV_ALIGN_TOP_LEFT as u32, 0, 0);

        let mut meta = String::new();
        if let Some(h) = disk.power_on_hours {
            meta.push_str(&format!("{:.0}h · ", h));
        }
        if let Some(t) = disk.temperature {
            meta.push_str(&format!("{:.0}℃ · ", t));
        }
        if let Some(w) = disk.percent_used {
            meta.push_str(&format!("损耗{:.1}%", w));
        }
        let meta_c = std::ffi::CString::new(meta).unwrap();
        let sub = lv_label_create(card);
        lv_label_set_text(sub, meta_c.as_ptr());
        lv_obj_align(sub, LV_ALIGN_BOTTOM_LEFT as u32, 0, 0);

        // Usage bar for first mount
        if let Some(mt) = disk.mounts.first() {
            let bar = lv_bar_create(card);
            lv_obj_set_size(bar, lv_obj_get_width(card) - 16, 8);
            lv_obj_align(bar, LV_ALIGN_BOTTOM_RIGHT as u32, 0, -20);
            lv_bar_set_range(bar, 0, 100);
            lv_bar_set_value(bar, mt.percent as i32, LV_ANIM_OFF as u32);
            lv_obj_set_style_bg_color(bar, lv_color_hex(0x30363D), 0);
            if mt.percent >= 90.0 {
                lv_obj_set_style_bg_color(bar, lv_color_hex(0xDA3633), LV_PART_INDICATOR as u32);
            } else if mt.percent >= 75.0 {
                lv_obj_set_style_bg_color(bar, lv_color_hex(0xD29922), LV_PART_INDICATOR as u32);
            } else {
                lv_obj_set_style_bg_color(bar, lv_color_hex(0x3FB950), LV_PART_INDICATOR as u32);
            }
        }
    }
}

unsafe fn update_item_list<T>(
    list: *mut lv_obj_t,
    items: &[T],
    display: impl Fn(&T) -> (String, String, bool),
) {
    lv_obj_clean(list);
    for item in items {
        let (name, detail, active) = display(item);
        let row = lv_obj_create(list);
        lv_obj_set_size(row, lv_obj_get_width(list) - 8, 32);
        lv_obj_set_style_bg_opa(row, 0, 0);
        lv_obj_set_style_border_width(row, 0, 0);
        lv_obj_set_style_pad_all(row, 0, 0);
        lv_obj_set_flex_flow(row, LV_FLEX_FLOW_ROW as u32);
        lv_obj_set_flex_align(row, LV_FLEX_ALIGN_SPACE_BETWEEN as u32, LV_FLEX_ALIGN_CENTER as u32, LV_FLEX_ALIGN_CENTER as u32);

        let name_c = std::ffi::CString::new(name).unwrap();
        let nl = lv_label_create(row);
        lv_label_set_text(nl, name_c.as_ptr());

        let status = if active { "运行" } else { "停止" };
        let badge = lv_btn_create(row);
        lv_obj_set_size(badge, 56, 24);
        lv_obj_set_style_bg_color(
            badge,
            if active { lv_color_hex(0x1B3D1F) } else { lv_color_hex(0x3D1B1B) },
            0,
        );
        lv_obj_set_style_radius(badge, 4, 0);
        let bl = lv_label_create(badge);
        let status_c = std::ffi::CString::new(status).unwrap();
        lv_label_set_text(bl, status_c.as_ptr());
        lv_obj_center(bl);
    }
}

unsafe fn update_svc_list(svcs: &[crate::metrics::ServiceStatus]) {
    lv_obj_clean(SVC_LIST);
    for svc in svcs {
        let row = lv_obj_create(SVC_LIST);
        lv_obj_set_size(row, lv_obj_get_width(SVC_LIST) - 8, 28);
        lv_obj_set_style_bg_opa(row, 0, 0);
        lv_obj_set_style_border_width(row, 0, 0);
        lv_obj_set_style_pad_all(row, 0, 0);
        lv_obj_set_flex_flow(row, LV_FLEX_FLOW_ROW as u32);
        lv_obj_set_flex_align(row, LV_FLEX_ALIGN_SPACE_BETWEEN as u32, LV_FLEX_ALIGN_CENTER as u32, LV_FLEX_ALIGN_CENTER as u32);

        let name_c = std::ffi::CString::new(svc.name.as_str()).unwrap();
        let nl = lv_label_create(row);
        lv_label_set_text(nl, name_c.as_ptr());

        let status = if svc.active { "活跃" } else { "停用" };
        let badge = lv_obj_create(row);
        lv_obj_set_size(badge, 48, 20);
        lv_obj_set_style_radius(badge, 4, 0);
        lv_obj_set_style_bg_color(badge, if svc.active { lv_color_hex(0x1B3D1F) } else { lv_color_hex(0x3D1B1B) }, 0);
        lv_obj_set_style_border_width(badge, 0, 0);
        let bl = lv_label_create(badge);
        let status_c = std::ffi::CString::new(status).unwrap();
        lv_label_set_text(bl, status_c.as_ptr());
        lv_obj_center(bl);
    }
}

// ── Helpers ────────────────────────────────────────────────

fn pick_net(nets: &[crate::metrics::NetworkIface]) -> Option<&crate::metrics::NetworkIface> {
    nets.iter()
        .filter(|n| {
            n.is_up
                && !n.ipv4.is_empty()
                && !n.name.starts_with("lo")
                && !n.name.starts_with("vnet")
                && !n.name.contains("ovs")
        })
        .max_by_key(|n| n.rx_bytes + n.tx_bytes)
}

fn format_speed(bytes_per_sec: f64) -> String {
    if bytes_per_sec < 1.0 {
        "0".to_string()
    } else if bytes_per_sec >= 1e9 {
        format!("{:.1}G", bytes_per_sec / 1e9)
    } else if bytes_per_sec >= 1e6 {
        format!("{:.1}M", bytes_per_sec / 1e6)
    } else if bytes_per_sec >= 1e3 {
        format!("{:.0}K", bytes_per_sec / 1e3)
    } else {
        format!("{:.0}B", bytes_per_sec)
    }
}

// ── Event Callbacks ────────────────────────────────────────

static mut PENDING_ACTION: Action = Action::None;

#[derive(Clone, Copy, PartialEq)]
enum Action {
    None,
    Reboot,
    Shutdown,
}

unsafe extern "C" fn swipe_cb(e: *mut lv_event_t) {
    let dir = lv_indev_get_gesture_dir(lv_event_get_indev(e));
    if dir == LV_DIR_LEFT as u32 && CURRENT_PANEL == 0 {
        CURRENT_PANEL = 1;
        animate_panel(-1);
    } else if dir == LV_DIR_RIGHT as u32 && CURRENT_PANEL == 1 {
        CURRENT_PANEL = 0;
        animate_panel(1);
    }
}

unsafe fn animate_panel(dir: i32) {
    let pw = lv_disp_get_hor_res(ptr::null_mut()) as i32;
    let target_x = if dir < 0 { -pw } else { 0 };

    let anim = lv_anim_t::zeroed();
    // Simplified: directly set position
    lv_obj_set_x(PANELS_CONTAINER, target_x);

    // Update dot indicators
    let dot_count = lv_obj_get_child_cnt(DOT_INDICATOR);
    for i in 0..dot_count {
        if let Some(dot) = lv_obj_get_child(DOT_INDICATOR, i) {
            lv_obj_set_style_bg_color(
                dot,
                if i == CURRENT_PANEL as u32 { lv_color_hex(0x3FB950) } else { lv_color_hex(0x30363D) },
                0,
            );
        }
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
    let c = std::ffi::CString::new(msg).unwrap();
    lv_label_set_text(MODAL_MSG, c.as_ptr());
    lv_obj_clear_flag(MODAL_BG, LV_OBJ_FLAG_HIDDEN as u32);
}

unsafe extern "C" fn hide_modal_cb(_e: *mut lv_event_t) {
    lv_obj_add_flag(MODAL_BG, LV_OBJ_FLAG_HIDDEN as u32);
    PENDING_ACTION = Action::None;
}

unsafe extern "C" fn exec_action_cb(_e: *mut lv_event_t) {
    match PENDING_ACTION {
        Action::Reboot => {
            lv_label_set_text(MODAL_MSG, c"重启中...".as_ptr());
            lv_obj_add_flag(MODAL_CANCEL, LV_OBJ_FLAG_HIDDEN as u32);
            lv_obj_add_flag(MODAL_OK, LV_OBJ_FLAG_HIDDEN as u32);
            std::process::Command::new("sync").output().ok();
            std::thread::sleep(std::time::Duration::from_millis(200));
            std::process::Command::new("systemctl").args(["reboot", "--force"]).output().ok();
        }
        Action::Shutdown => {
            lv_label_set_text(MODAL_MSG, c"关机中...".as_ptr());
            lv_obj_add_flag(MODAL_CANCEL, LV_OBJ_FLAG_HIDDEN as u32);
            lv_obj_add_flag(MODAL_OK, LV_OBJ_FLAG_HIDDEN as u32);
            std::process::Command::new("sync").output().ok();
            std::thread::sleep(std::time::Duration::from_millis(200));
            std::process::Command::new("systemctl").args(["poweroff", "--force"]).output().ok();
        }
        Action::None => {}
    }
}

// ── LVGL C FFI declarations ────────────────────────────────

use std::ffi::c_void;
use std::ptr;

// Type aliases
type lv_obj_t = c_void;
type lv_event_t = c_void;
type lv_color_t = u32;
type lv_anim_t = c_void;

// Display driver
#[repr(C)]
struct lv_disp_drv_t {
    hor_res: i32,
    ver_res: i32,
    flush_cb: Option<unsafe extern "C" fn(*mut lv_disp_drv_t, *const lv_area_t, *mut lv_color_t)>,
    draw_buf: *mut lv_disp_draw_buf_t,
    // ... many fields, zeroed for simplicity
}

impl lv_disp_drv_t {
    const fn zeroed() -> Self {
        lv_disp_drv_t {
            hor_res: 0,
            ver_res: 0,
            flush_cb: None,
            draw_buf: ptr::null_mut(),
        }
    }
}

#[repr(C)]
struct lv_disp_draw_buf_t {
    buf1: *mut lv_color_t,
    size: usize,
}

type LvDispBuf = lv_disp_draw_buf_t;

#[repr(C)]
struct lv_area_t {
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
}

// Constants
const LV_ALIGN_TOP_LEFT: u32 = 0;
const LV_ALIGN_TOP_RIGHT: u32 = 1;
const LV_ALIGN_TOP_MID: u32 = 2;
const LV_ALIGN_BOTTOM_LEFT: u32 = 4;
const LV_ALIGN_BOTTOM_MID: u32 = 5;
const LV_ALIGN_BOTTOM_RIGHT: u32 = 6;
const LV_ALIGN_CENTER: u32 = 9;
const LV_ALIGN_OUT_BOTTOM_LEFT: u32 = 16;
const LV_EVENT_CLICKED: u32 = 7;
const LV_EVENT_GESTURE: u32 = 42;
const LV_OBJ_FLAG_HIDDEN: u32 = 1;
const LV_OBJ_FLAG_SCROLLABLE: u32 = 2;
const LV_PART_INDICATOR: u32 = 1;
const LV_DIR_LEFT: u32 = 1;
const LV_DIR_RIGHT: u32 = 2;
const LV_FLEX_FLOW_ROW: u32 = 0;
const LV_FLEX_FLOW_COLUMN: u32 = 1;
const LV_FLEX_ALIGN_START: u32 = 0;
const LV_FLEX_ALIGN_CENTER: u32 = 1;
const LV_FLEX_ALIGN_SPACE_BETWEEN: u32 = 3;
const LV_ANIM_OFF: u32 = 0;

// External C functions from LVGL
extern "C" {
    fn lv_init();
    fn lv_tick_inc(ms: u32);
    fn lv_task_handler() -> u32;

    // Display
    fn lv_disp_drv_init(drv: *mut lv_disp_drv_t);
    fn lv_disp_drv_register(drv: *mut lv_disp_drv_t) -> *mut c_void;
    fn lv_disp_get_hor_res(disp: *mut c_void) -> i32;
    fn lv_disp_get_ver_res(disp: *mut c_void) -> i32;

    // Screen
    fn lv_scr_act() -> *mut lv_obj_t;

    // Objects
    fn lv_obj_create(parent: *mut lv_obj_t) -> *mut lv_obj_t;
    fn lv_obj_set_size(obj: *mut lv_obj_t, w: i32, h: i32);
    fn lv_obj_set_pos(obj: *mut lv_obj_t, x: i32, y: i32);
    fn lv_obj_set_x(obj: *mut lv_obj_t, x: i32);
    fn lv_obj_align(obj: *mut lv_obj_t, align: u32, x_ofs: i32, y_ofs: i32);
    fn lv_obj_align_to(obj: *mut lv_obj_t, base: *const lv_obj_t, align: u32, x_ofs: i32, y_ofs: i32);
    fn lv_obj_center(obj: *mut lv_obj_t);
    fn lv_obj_add_flag(obj: *mut lv_obj_t, flag: u32);
    fn lv_obj_clear_flag(obj: *mut lv_obj_t, flag: u32);
    fn lv_obj_get_width(obj: *const lv_obj_t) -> i32;
    fn lv_obj_get_child_cnt(obj: *const lv_obj_t) -> u32;
    fn lv_obj_get_child(obj: *const lv_obj_t, idx: u32) -> *mut lv_obj_t;
    fn lv_obj_clean(obj: *mut lv_obj_t);
    fn lv_obj_set_style_bg_color(obj: *mut lv_obj_t, color: lv_color_t, selector: u32);
    fn lv_obj_set_style_bg_opa(obj: *mut lv_obj_t, opa: u32, selector: u32);
    fn lv_obj_set_style_border_width(obj: *mut lv_obj_t, width: i32, selector: u32);
    fn lv_obj_set_style_pad_all(obj: *mut lv_obj_t, pad: i32, selector: u32);
    fn lv_obj_set_style_pad_column(obj: *mut lv_obj_t, pad: i32, selector: u32);
    fn lv_obj_set_style_pad_row(obj: *mut lv_obj_t, pad: i32, selector: u32);
    fn lv_obj_set_style_radius(obj: *mut lv_obj_t, radius: i32, selector: u32);
    fn lv_obj_set_style_arc_color(obj: *mut lv_obj_t, color: lv_color_t, selector: u32);
    fn lv_obj_set_style_text_font(obj: *mut lv_obj_t, font: *const c_void, selector: u32);
    fn lv_obj_set_flex_flow(obj: *mut lv_obj_t, flow: u32);
    fn lv_obj_set_flex_align(
        obj: *mut lv_obj_t,
        main: u32,
        cross: u32,
        track: u32,
    );
    fn lv_obj_add_event_cb(
        obj: *mut lv_obj_t,
        cb: Option<unsafe extern "C" fn(*mut lv_event_t)>,
        event: u32,
        user_data: *mut c_void,
    );

    // Labels
    fn lv_label_create(parent: *mut lv_obj_t) -> *mut lv_obj_t;
    fn lv_label_set_text(label: *mut lv_obj_t, text: *const u8);

    // Buttons
    fn lv_btn_create(parent: *mut lv_obj_t) -> *mut lv_obj_t;

    // Arc
    fn lv_arc_create(parent: *mut lv_obj_t) -> *mut lv_obj_t;
    fn lv_arc_set_range(arc: *mut lv_obj_t, min: i32, max: i32);
    fn lv_arc_set_value(arc: *mut lv_obj_t, value: i32);

    // Bar
    fn lv_bar_create(parent: *mut lv_obj_t) -> *mut lv_obj_t;
    fn lv_bar_set_range(bar: *mut lv_obj_t, min: i32, max: i32);
    fn lv_bar_set_value(bar: *mut lv_obj_t, value: i32, anim: u32);

    // Input
    fn lv_indev_get_gesture_dir(indev: *mut c_void) -> u32;
    fn lv_mouse_set_cursor_pos(x: u16, y: u16);
    fn lv_mouse_press();
    fn lv_mouse_release();

    // Events
    fn lv_event_get_indev(e: *mut lv_event_t) -> *mut c_void;

    // Colors
    fn lv_color_hex(hex: u32) -> lv_color_t;

    // Fonts
    static lv_font_montserrat_24: c_void;

    // Theme
    fn set_dark_theme();
}

// ── Display flush callback ─────────────────────────────────

unsafe extern "C" fn flush_cb(
    _drv: *mut lv_disp_drv_t,
    _area: *const lv_area_t,
    _color_p: *mut lv_color_t,
) {
    // Framebuffer is memory-mapped DRM dumb buffer — LVGL draws
    // directly into it. No flush needed for simple scanout.
    lv_disp_flush_ready(_drv);
}

extern "C" {
    fn lv_disp_flush_ready(drv: *mut lv_disp_drv_t);
}
