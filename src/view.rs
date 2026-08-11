use crate::metrics::{DiskHealth, NetworkIface, SystemData};
use crate::render::Renderer;

const BG: u32 = 0x000A0E14;
const CARD: u32 = 0x00151B26;
const CARD_ALT: u32 = 0x001C2532;
const LINE: u32 = 0x002A3441;
const TEXT: u32 = 0x00E6EDF3;
const DIM: u32 = 0x007D8896;
const GREEN: u32 = 0x003FB950;
const YELLOW: u32 = 0x00E3B341;
const ORANGE: u32 = 0x00F0883E;
const RED: u32 = 0x00F85149;
const BLUE: u32 = 0x0058A6FF;
const PURPLE: u32 = 0x00BC8CFF;
const CYAN: u32 = 0x0039D2C0;

const PAGE_PADDING_X: i32 = 14;
const PAGE_WIDTH: i32 = 348;
const HEADER_Y: i32 = 16;
const GAUGES_Y: i32 = 92;
const GAUGE_HEIGHT: i32 = 204;
const NETWORK_Y: i32 = 310;
const NETWORK_HEIGHT: i32 = 174;
const NETWORK_SPEED_Y: i32 = 59;
const NETWORK_SPEED_HEIGHT: i32 = 46;
const NETWORK_DIVIDER_Y: i32 = 115;
const NETWORK_IP_Y: i32 = 124;
const NETWORK_IP_HEIGHT: i32 = 34;
const STORAGE_Y: i32 = 498;
const DISK_ROW_WITH_USAGE: i32 = 182;
const DISK_ROW_WITHOUT_USAGE: i32 = 130;

const SERVICES_Y: i32 = 92;
const BLOCK_BASE_HEIGHT: i32 = 80;
const SERVICE_ROW_HEIGHT: i32 = 64;
const DOCKER_ROW_HEIGHT: i32 = 93;
const VM_ROW_HEIGHT: i32 = 93;
const POWER_BLOCK_HEIGHT: i32 = 294;
const BLOCK_GAP: i32 = 14;

const MODAL_BUTTON_Y: i32 = 498;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Overview,
    Services,
    Vms,
    Power,
}

impl Page {
    pub fn shifted(self, delta: i32) -> Self {
        const PAGES: [Page; 4] = [Page::Overview, Page::Services, Page::Vms, Page::Power];
        if delta == 0 {
            self
        } else {
            PAGES[(self.index() as i32 + delta).rem_euclid(PAGES.len() as i32) as usize]
        }
    }

    pub fn index(self) -> usize {
        match self {
            Self::Overview => 0,
            Self::Services => 1,
            Self::Vms => 2,
            Self::Power => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerAction {
    Reboot,
    Shutdown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overlay {
    None,
    Confirm(PowerAction),
    Executing(PowerAction),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitTarget {
    Reboot,
    Shutdown,
    Cancel,
    Confirm(PowerAction),
}

pub fn draw(renderer: &Renderer, data: &SystemData, page: Page, scroll_y: i32, overlay: Overlay) {
    renderer.set_origin_x(0);
    renderer.rect(0, 0, renderer.width(), renderer.height(), BG);
    draw_page(renderer, data, page, scroll_y);
    draw_page_dots(renderer, page);
    if overlay != Overlay::None {
        draw_overlay(renderer, overlay);
    }
    renderer.present();
}

/// Refresh only values that can change in the fast 500 ms sample. Static
/// layout remains in the renderer's retained buffer, so the panel never shows
/// a blank full-screen intermediate frame while gauges, network values, or
/// storage bars are updated.
pub fn draw_dynamic(renderer: &Renderer, data: &SystemData, page: Page, scroll_y: i32) {
    renderer.set_origin_x(0);
    let offset = -scroll_y.clamp(0, max_scroll(data, page, renderer.height()));
    if visible(renderer, HEADER_Y + offset, 62) {
        clear_region(renderer, 0, HEADER_Y + offset, renderer.width() as i32, 62);
        draw_header(renderer, data, page, offset);
    }
    if page == Page::Overview {
        if visible(renderer, GAUGES_Y + offset, GAUGE_HEIGHT) {
            draw_gauge(
                renderer,
                PAGE_PADDING_X,
                GAUGES_Y + offset,
                data.cpu.percent,
                "CPU",
                &format!(
                    "{}℃ · {:.1}G",
                    data.cpu.temperature_c.unwrap_or(0.0).round(),
                    data.cpu.freq_mhz.unwrap_or(0.0) / 1000.0
                ),
                BLUE,
            );
            draw_gauge(
                renderer,
                194,
                GAUGES_Y + offset,
                data.memory.percent,
                "内存",
                &format!("{:.1}G / {:.1}G", data.memory.used_gb, data.memory.total_gb),
                PURPLE,
            );
        }
        if visible(renderer, NETWORK_Y + offset, NETWORK_HEIGHT) {
            draw_network(renderer, data, NETWORK_Y + offset);
        }
        let storage_y = STORAGE_Y + offset;
        if visible(renderer, storage_y, storage_height(data)) {
            draw_storage(renderer, data, storage_y);
        }
    }
    renderer.present();
}

fn clear_region(renderer: &Renderer, x: i32, y: i32, width: i32, height: i32) {
    let top = y.max(0);
    let bottom = (y + height).min(renderer.height() as i32);
    if bottom > top {
        renderer.rect(x, top, width.max(0) as u32, (bottom - top) as u32, BG);
    }
}

pub fn draw_swipe(
    renderer: &Renderer,
    data: &SystemData,
    page: Page,
    scroll: [i32; 4],
    drag_x: i32,
) {
    let width = renderer.width() as i32;
    let drag_x = drag_x.clamp(-width, width);
    if drag_x == 0 {
        draw(renderer, data, page, scroll[page.index()], Overlay::None);
        return;
    }
    let direction = if drag_x < 0 { 1 } else { -1 };
    let next = page.shifted(direction);
    let next_x = if drag_x < 0 {
        width + drag_x
    } else {
        -width + drag_x
    };

    renderer.set_origin_x(0);
    renderer.rect(0, 0, renderer.width(), renderer.height(), BG);
    renderer.set_origin_x(drag_x);
    draw_page(renderer, data, page, scroll[page.index()]);
    renderer.set_origin_x(next_x);
    draw_page(renderer, data, next, scroll[next.index()]);
    renderer.set_origin_x(0);
    draw_page_dots(renderer, page);
    renderer.present();
}

pub fn max_scroll(data: &SystemData, page: Page, viewport_height: u32) -> i32 {
    (content_height(data, page) - viewport_height as i32).max(0)
}

pub fn hit_test(
    _data: &SystemData,
    page: Page,
    scroll_y: i32,
    overlay: Overlay,
    x: i32,
    y: i32,
) -> Option<HitTarget> {
    if let Overlay::Confirm(action) = overlay {
        if inside(x, y, 46, MODAL_BUTTON_Y, 136, 84) {
            return Some(HitTarget::Cancel);
        }
        if inside(x, y, 195, MODAL_BUTTON_Y, 135, 84) {
            return Some(HitTarget::Confirm(action));
        }
        return None;
    }
    if overlay != Overlay::None || page != Page::Power {
        return None;
    }
    let power_y = SERVICES_Y - scroll_y;
    if inside(x, y, 30, power_y + 59, 316, 96) {
        Some(HitTarget::Reboot)
    } else if inside(x, y, 30, power_y + 167, 316, 96) {
        Some(HitTarget::Shutdown)
    } else {
        None
    }
}

fn draw_page(renderer: &Renderer, data: &SystemData, page: Page, scroll_y: i32) {
    let scroll_y = scroll_y.clamp(0, max_scroll(data, page, renderer.height()));
    let offset = -scroll_y;
    match page {
        Page::Overview => draw_overview(renderer, data, offset),
        Page::Services => draw_services_page(renderer, data, offset),
        Page::Vms => draw_vms_page(renderer, data, offset),
        Page::Power => draw_power_page(renderer, data, offset),
    }
}

fn draw_header(renderer: &Renderer, data: &SystemData, page: Page, offset: i32) {
    let (icon, title) = match page {
        Page::Overview => (UiIcon::Overview, "概况"),
        Page::Services => (UiIcon::Settings, "服务"),
        Page::Vms => (UiIcon::Monitor, "虚拟机"),
        Page::Power => (UiIcon::Warning, "电源操作"),
    };
    draw_icon(
        renderer,
        icon,
        PAGE_PADDING_X,
        HEADER_Y + 4 + offset,
        44,
        CYAN,
    );
    renderer.text_bold(74, HEADER_Y + 1 + offset, title, 8, CYAN);
    if page == Page::Overview {
        let uptime = format!(
            "{}天 {}时 {}分",
            data.uptime.days, data.uptime.hours, data.uptime.minutes
        );
        renderer.text_right(362, HEADER_Y + 14 + offset, &uptime, 4, DIM);
    }
    renderer.rect(PAGE_PADDING_X, 75 + offset, PAGE_WIDTH as u32, 2, LINE);
}

fn draw_overview(renderer: &Renderer, data: &SystemData, offset: i32) {
    if visible(renderer, HEADER_Y + offset, 62) {
        draw_header(renderer, data, Page::Overview, offset);
    }
    if visible(renderer, GAUGES_Y + offset, GAUGE_HEIGHT) {
        draw_gauge(
            renderer,
            PAGE_PADDING_X,
            GAUGES_Y + offset,
            data.cpu.percent,
            "CPU",
            &format!(
                "{}℃ · {:.1}G",
                data.cpu.temperature_c.unwrap_or(0.0).round(),
                data.cpu.freq_mhz.unwrap_or(0.0) / 1000.0
            ),
            BLUE,
        );
        draw_gauge(
            renderer,
            194,
            GAUGES_Y + offset,
            data.memory.percent,
            "内存",
            &format!("{:.1}G / {:.1}G", data.memory.used_gb, data.memory.total_gb),
            PURPLE,
        );
    }
    if visible(renderer, NETWORK_Y + offset, NETWORK_HEIGHT) {
        draw_network(renderer, data, NETWORK_Y + offset);
    }
    let storage_y = STORAGE_Y + offset;
    if visible(renderer, storage_y, storage_height(data)) {
        draw_storage(renderer, data, storage_y);
    }
}

fn draw_gauge(
    renderer: &Renderer,
    x: i32,
    y: i32,
    percent: f64,
    label: &str,
    detail: &str,
    color: u32,
) {
    card(renderer, x, y, 169, GAUGE_HEIGHT as u32, LINE);
    let center = x + 84;
    renderer.ring(center, y + 88, 70, 12, 100.0, LINE);
    renderer.ring(center, y + 88, 70, 12, percent, color);
    renderer.text_center_bold(
        center,
        y + 62,
        &format!("{:.0}", percent.clamp(0.0, 100.0)),
        5,
        TEXT,
    );
    renderer.text_center_bold(center, y + 101, label, 1, DIM);
    renderer.text_center(center, y + 171, detail, 1, DIM);
}

fn draw_network(renderer: &Renderer, data: &SystemData, y: i32) {
    card(
        renderer,
        PAGE_PADDING_X,
        y,
        PAGE_WIDTH as u32,
        NETWORK_HEIGHT as u32,
        LINE,
    );
    draw_icon(renderer, UiIcon::Network, 31, y + 18, 30, BLUE);
    renderer.text_bold(73, y + 15, "网络", 5, BLUE);

    let network = pick_net(&data.network);
    if let Some(network) = network {
        renderer.text_bold(
            137,
            y + 15,
            &fit_text(renderer, &network.name, 5, 205),
            5,
            BLUE,
        );
    }
    renderer.rounded_rect(
        30,
        y + NETWORK_SPEED_Y,
        152,
        NETWORK_SPEED_HEIGHT as u32,
        12,
        CARD_ALT,
    );
    renderer.rounded_rect(
        194,
        y + NETWORK_SPEED_Y,
        152,
        NETWORK_SPEED_HEIGHT as u32,
        12,
        CARD_ALT,
    );
    let (rx, tx, ip) = if let Some(network) = network {
        (
            network.rx_speed,
            network.tx_speed,
            network.ipv4.first().map(String::as_str).unwrap_or("--"),
        )
    } else {
        (0.0, 0.0, "--")
    };
    // Arrow fixed at the left, unit fixed at the right; the number is
    // right-aligned towards the unit so the arrow never shifts with the
    // width of the value.
    draw_speed(
        renderer,
        30,
        y + NETWORK_SPEED_Y,
        152,
        NETWORK_SPEED_HEIGHT,
        "↓",
        rx,
        GREEN,
    );
    draw_speed(
        renderer,
        194,
        y + NETWORK_SPEED_Y,
        152,
        NETWORK_SPEED_HEIGHT,
        "↑",
        tx,
        ORANGE,
    );
    renderer.rect(30, y + NETWORK_DIVIDER_Y, 316, 1, LINE);
    renderer.rounded_rect(30, y + NETWORK_IP_Y, 38, NETWORK_IP_HEIGHT as u32, 6, CYAN);
    renderer.text_center_bold_in_rect(49, y + NETWORK_IP_Y, NETWORK_IP_HEIGHT, "IP", 2, BG);
    renderer.text_in_rect(
        78,
        y + NETWORK_IP_Y,
        NETWORK_IP_HEIGHT,
        &fit_text(renderer, ip, 4, 260),
        4,
        TEXT,
    );
}

fn draw_speed(
    renderer: &Renderer,
    box_x: i32,
    box_y: i32,
    box_width: i32,
    box_height: i32,
    arrow: &str,
    speed: f64,
    arrow_color: u32,
) {
    // The ↑/↓ glyphs render much narrower than their font advance, so the
    // arrow width is a measured constant instead of text_width().
    const ARROW_W: i32 = 16;
    const GAP: i32 = 10;
    const VALUE_SCALE: u32 = 3;
    let (number, unit) = split_speed(speed);
    // Arrow stays fixed at the left of the box.
    let arrow_x = box_x + 12;
    renderer.text_in_rect(arrow_x, box_y, box_height, arrow, 7, arrow_color);
    // Unit is fixed at the right of the box, the number right-aligned to it.
    let unit_w = renderer.text_width(&unit, VALUE_SCALE) as i32;
    let unit_x = box_x + box_width - 10 - unit_w;
    let number_w = renderer.text_width(&number, VALUE_SCALE) as i32;
    let number_x = (unit_x - GAP - number_w).max(arrow_x + ARROW_W + GAP);
    // Scale 3 is two steps smaller than the former scale 5. The extra top
    // offset centers the smaller value/unit pair with the larger arrow glyph.
    renderer.text_bold_in_rect(number_x, box_y, box_height, &number, VALUE_SCALE, TEXT);
    renderer.text_in_rect(unit_x, box_y, box_height, &unit, VALUE_SCALE, TEXT);
}

fn draw_storage(renderer: &Renderer, data: &SystemData, y: i32) {
    let height = storage_height(data);
    card(
        renderer,
        PAGE_PADDING_X,
        y,
        PAGE_WIDTH as u32,
        height as u32,
        LINE,
    );
    draw_icon(renderer, UiIcon::Storage, 31, y + 18, 30, BLUE);
    renderer.text_bold(73, y + 15, "存储", 5, BLUE);
    let mut row_y = y + 59;
    if data.disks.is_empty() {
        renderer.text_center(188, row_y + 40, "无数据", 5, DIM);
        return;
    }
    for (index, disk) in data.disks.iter().enumerate() {
        let row_height = disk_row_height(disk);
        if visible(renderer, row_y, row_height) {
            draw_disk(renderer, disk, row_y, index > 0);
        }
        row_y += row_height;
    }
}

fn draw_disk(renderer: &Renderer, disk: &DiskHealth, y: i32, separator: bool) {
    if separator {
        renderer.rect(30, y, 316, 1, LINE);
    }
    let title = if disk.model.trim().is_empty() {
        disk.name.as_str()
    } else {
        disk.model.as_str()
    };
    renderer.text_bold_in_rect(30, y + 1, 38, &fit_text(renderer, title, 3, 316), 3, TEXT);

    let (type_label, type_bg, type_fg) = if disk.disk_type == "emmc" {
        ("eMMC", 0x001A2A3A, BLUE)
    } else if disk.role == "ssd" || disk.disk_type == "nvme" || disk.disk_type == "ssd" {
        ("SSD", 0x001A3A2A, GREEN)
    } else if disk.role == "hdd" {
        ("HDD", 0x003A2A1A, ORANGE)
    } else {
        ("DISK", 0x001A3A2A, GREEN)
    };
    let mut badge_x = 30;
    badge_x += badge(
        renderer,
        badge_x,
        y + 49,
        &format!("{} {}", type_label, disk.size),
        type_bg,
        type_fg,
    );
    if let Some(health) = &disk.health {
        let (label, bg, fg) = match health.to_ascii_uppercase().as_str() {
            "PASSED" => ("正常", 0x0014331C, GREEN),
            "WARNING" => ("注意", 0x003A3417, YELLOW),
            _ => ("告警", 0x003A1717, RED),
        };
        badge_x += badge(renderer, badge_x, y + 49, label, bg, fg);
    }
    if disk.role == "system" {
        badge(renderer, badge_x, y + 49, "系统", 0x001A2A3A, CYAN);
    }

    let mut meta = Vec::new();
    if let Some(hours) = disk.power_on_hours {
        meta.push(format!("{:.0}h", hours));
    }
    if let Some(temperature) = disk.temperature {
        meta.push(format!("{:.0}℃", temperature));
    }
    if let Some(sectors) = disk.reallocated_sectors {
        meta.push(format!("坏道{}", sectors));
    }
    if let Some((low, high)) = disk.life_range {
        // eMMC: EXT_CSD life_time bucket, e.g. 10~20% of rated life used.
        meta.push(format!("已耗{}~{}%", low, high));
    } else if let Some(wear) = disk.percent_used {
        meta.push(format!("损耗{:.1}%", wear));
    }
    renderer.text_in_rect(
        30,
        y + 75,
        38,
        &fit_text(renderer, &meta.join(" · "), 3, 316),
        3,
        DIM,
    );

    if !disk.mounts.is_empty() {
        let (percent, usage) = aggregate_usage(disk);
        renderer.rounded_rect(30, y + 126, 316, 40, 10, LINE);
        let fill = (316.0 * percent.clamp(0.0, 100.0) / 100.0).round() as u32;
        renderer.rounded_rect(30, y + 126, fill.max(5), 40, 10, bar_color(percent));
        renderer.text_right_bold_in_rect(332, y + 126, 40, &usage, 2, TEXT);
    }
}

fn draw_services_page(renderer: &Renderer, data: &SystemData, offset: i32) {
    if visible(renderer, HEADER_Y + offset, 62) {
        draw_header(renderer, data, Page::Services, offset);
    }
    let (services_y, docker_y) = service_layout(data);
    let services_height =
        BLOCK_BASE_HEIGHT + data.services.len().max(1) as i32 * SERVICE_ROW_HEIGHT;
    let docker_height = BLOCK_BASE_HEIGHT + data.docker.len().max(1) as i32 * DOCKER_ROW_HEIGHT;
    if visible(renderer, services_y + offset, services_height) {
        draw_service_block(renderer, data, services_y + offset);
    }
    if visible(renderer, docker_y + offset, docker_height) {
        draw_docker_block(renderer, data, docker_y + offset);
    }
}

fn draw_vms_page(renderer: &Renderer, data: &SystemData, offset: i32) {
    if visible(renderer, HEADER_Y + offset, 62) {
        draw_header(renderer, data, Page::Vms, offset);
    }
    let vms_height = BLOCK_BASE_HEIGHT + data.vms.len().max(1) as i32 * VM_ROW_HEIGHT;
    if visible(renderer, SERVICES_Y + offset, vms_height) {
        draw_vm_block(renderer, data, SERVICES_Y + offset);
    }
}

fn draw_power_page(renderer: &Renderer, data: &SystemData, offset: i32) {
    if visible(renderer, HEADER_Y + offset, 62) {
        draw_header(renderer, data, Page::Power, offset);
    }
    if visible(renderer, SERVICES_Y + offset, POWER_BLOCK_HEIGHT) {
        draw_power_block(renderer, SERVICES_Y + offset);
    }
}

fn draw_service_block(renderer: &Renderer, data: &SystemData, y: i32) {
    let height = BLOCK_BASE_HEIGHT + data.services.len().max(1) as i32 * SERVICE_ROW_HEIGHT;
    card(
        renderer,
        PAGE_PADDING_X,
        y,
        PAGE_WIDTH as u32,
        height as u32,
        LINE,
    );
    block_header(renderer, y, UiIcon::Core, "核心服务");
    if data.services.is_empty() {
        renderer.text_center(188, y + 88, "无数据", 5, DIM);
        return;
    }
    for (index, service) in data.services.iter().enumerate() {
        let row_y = y + 59 + index as i32 * SERVICE_ROW_HEIGHT;
        if !visible(renderer, row_y, SERVICE_ROW_HEIGHT) {
            continue;
        }
        if index > 0 {
            renderer.rect(30, row_y, 316, 1, LINE);
        }
        renderer.text_bold_in_rect(
            30,
            row_y + 1,
            SERVICE_ROW_HEIGHT - 2,
            &fit_text(renderer, &service.name, 5, 222),
            5,
            TEXT,
        );
        status_tag(
            renderer,
            row_y + 12,
            if service.active { "活跃" } else { "停用" },
            service.active,
        );
    }
}

fn draw_docker_block(renderer: &Renderer, data: &SystemData, y: i32) {
    let height = BLOCK_BASE_HEIGHT + data.docker.len().max(1) as i32 * DOCKER_ROW_HEIGHT;
    card(
        renderer,
        PAGE_PADDING_X,
        y,
        PAGE_WIDTH as u32,
        height as u32,
        LINE,
    );
    block_header(renderer, y, UiIcon::Docker, "Docker");
    if data.docker.is_empty() {
        renderer.text_center(188, y + 88, "无容器", 5, DIM);
        return;
    }
    for (index, container) in data.docker.iter().enumerate() {
        let row_y = y + 59 + index as i32 * DOCKER_ROW_HEIGHT;
        if !visible(renderer, row_y, DOCKER_ROW_HEIGHT) {
            continue;
        }
        let running = container.state.eq_ignore_ascii_case("running");
        if index > 0 {
            renderer.rect(30, row_y, 316, 1, LINE);
        }
        renderer.text_bold_in_rect(
            30,
            row_y,
            DOCKER_ROW_HEIGHT,
            &fit_text(renderer, &container.names, 5, 222),
            5,
            TEXT,
        );
        status_tag(
            renderer,
            row_y + (DOCKER_ROW_HEIGHT - 38) / 2,
            if running { "运行" } else { "停止" },
            running,
        );
    }
}

fn draw_vm_block(renderer: &Renderer, data: &SystemData, y: i32) {
    let height = BLOCK_BASE_HEIGHT + data.vms.len().max(1) as i32 * VM_ROW_HEIGHT;
    card(
        renderer,
        PAGE_PADDING_X,
        y,
        PAGE_WIDTH as u32,
        height as u32,
        LINE,
    );
    block_header(renderer, y, UiIcon::Monitor, "虚拟机");
    if data.vms.is_empty() {
        renderer.text_center(188, y + 88, "无虚拟机", 5, DIM);
        return;
    }
    for (index, vm) in data.vms.iter().enumerate() {
        let row_y = y + 59 + index as i32 * VM_ROW_HEIGHT;
        if !visible(renderer, row_y, VM_ROW_HEIGHT) {
            continue;
        }
        let running = vm.state.eq_ignore_ascii_case("running");
        if index > 0 {
            renderer.rect(30, row_y, 316, 1, LINE);
        }
        renderer.text_bold_in_rect(
            30,
            row_y + 1,
            38,
            &fit_text(renderer, &vm.name, 5, 222),
            5,
            TEXT,
        );
        renderer.text_in_rect(30, row_y + 39, 38, &format!("ID {}", vm.id), 2, DIM);
        status_tag(
            renderer,
            row_y + 18,
            if running { "运行" } else { &vm.state },
            running,
        );
    }
}

fn draw_power_block(renderer: &Renderer, y: i32) {
    card(
        renderer,
        PAGE_PADDING_X,
        y,
        PAGE_WIDTH as u32,
        POWER_BLOCK_HEIGHT as u32,
        RED,
    );
    block_header(renderer, y, UiIcon::Warning, "电源操作");
    renderer.rounded_rect(30, y + 59, 316, 96, 16, ORANGE);
    draw_icon(renderer, UiIcon::Restart, 126, y + 88, 34, 0x00DDE5EC);
    renderer.text_center_bold(220, y + 86, "重启", 7, 0x00000000);
    renderer.rounded_rect(30, y + 167, 316, 96, 16, RED);
    draw_icon(renderer, UiIcon::Power, 130, y + 197, 30, TEXT);
    renderer.text_center_bold(218, y + 194, "关机", 7, TEXT);
}

fn block_header(renderer: &Renderer, y: i32, icon: UiIcon, label: &str) {
    draw_icon(renderer, icon, 31, y + 18, 30, BLUE);
    renderer.text_bold(73, y + 15, label, 5, BLUE);
}

#[derive(Clone, Copy)]
enum UiIcon {
    Overview,
    Settings,
    Network,
    Storage,
    Core,
    Docker,
    Monitor,
    Warning,
    Restart,
    Power,
}

fn draw_icon(renderer: &Renderer, icon: UiIcon, x: i32, y: i32, size: i32, color: u32) {
    let bitmap: Option<(u32, u32, &[u8])> = match icon {
        UiIcon::Overview => Some((
            50,
            47,
            include_bytes!("../assets/emoji/overview-50x47.rgba"),
        )),
        UiIcon::Settings => Some((
            50,
            47,
            include_bytes!("../assets/emoji/settings-50x47.rgba"),
        )),
        UiIcon::Network => Some((35, 33, include_bytes!("../assets/emoji/network-35x33.rgba"))),
        UiIcon::Storage => Some((35, 33, include_bytes!("../assets/emoji/storage-35x33.rgba"))),
        UiIcon::Core => Some((35, 33, include_bytes!("../assets/emoji/core-35x33.rgba"))),
        UiIcon::Docker => Some((35, 33, include_bytes!("../assets/emoji/docker-35x33.rgba"))),
        UiIcon::Monitor => Some((35, 33, include_bytes!("../assets/emoji/monitor-35x33.rgba"))),
        UiIcon::Warning => Some((35, 33, include_bytes!("../assets/emoji/warning-35x33.rgba"))),
        UiIcon::Restart => Some((45, 42, include_bytes!("../assets/emoji/restart-45x42.rgba"))),
        UiIcon::Power => None,
    };
    if let Some((width, height, pixels)) = bitmap {
        renderer.rgba_image(x, y, width, height, pixels);
        return;
    }

    let center_x = x + size / 2;
    let center_y = y + size / 2;
    match icon {
        UiIcon::Overview => {
            renderer.rounded_rect(x, y, size as u32, size as u32, 2, 0x00E5EAF0);
            renderer.rect(
                x + 4,
                y + 5,
                (size - 8) as u32,
                (size - 10) as u32,
                0x00F8FAFC,
            );
            renderer.rect(x + 8, y + 22, 5, 12, GREEN);
            renderer.rect(x + 17, y + 14, 5, 20, BLUE);
            renderer.rect(x + 26, y + 18, 5, 16, RED);
            renderer.rect(x + 5, y + 7, (size - 10) as u32, 3, DIM);
        }
        UiIcon::Settings => {
            let gear = 0x00AFC7D8;
            renderer.ring(center_x, center_y, size * 3 / 10, size / 7, 100.0, gear);
            renderer.ring(center_x, center_y, size / 9, size / 10, 100.0, BG);
            let spoke = (size / 7).max(4) as u32;
            renderer.rect(
                center_x - spoke as i32 / 2,
                y,
                spoke,
                (size / 5) as u32,
                gear,
            );
            renderer.rect(
                center_x - spoke as i32 / 2,
                y + size * 4 / 5,
                spoke,
                (size / 5) as u32,
                gear,
            );
            renderer.rect(
                x,
                center_y - spoke as i32 / 2,
                (size / 5) as u32,
                spoke,
                gear,
            );
            renderer.rect(
                x + size * 4 / 5,
                center_y - spoke as i32 / 2,
                (size / 5) as u32,
                spoke,
                gear,
            );
            let corner = (size / 5).max(7) as u32;
            let far = x + size - corner as i32 - 3;
            let low = y + 3;
            let high = y + size - corner as i32 - 3;
            renderer.rounded_rect(x + 3, low, corner, corner, 2, gear);
            renderer.rounded_rect(far, low, corner, corner, 2, gear);
            renderer.rounded_rect(x + 3, high, corner, corner, 2, gear);
            renderer.rounded_rect(far, high, corner, corner, 2, gear);
        }
        UiIcon::Network => {
            renderer.ring(center_x, center_y, size / 2 - 1, 2, 100.0, color);
            renderer.ring(center_x, center_y, size / 4, 2, 100.0, color);
            renderer.rect(x + 1, center_y - 1, (size - 2) as u32, 2, color);
            renderer.rect(center_x - 1, y + 1, 2, (size - 2) as u32, color);
        }
        UiIcon::Storage => {
            renderer.rounded_rect(x + 3, y, (size - 6) as u32, size as u32, 2, 0x00E5EAF0);
            renderer.rect(x + 7, y + 3, (size - 14) as u32, (size / 3) as u32, DIM);
            renderer.rect(x + 8, y + size * 2 / 3, (size - 16) as u32, 3, BLUE);
            renderer.rounded_rect(x + size - 9, y + size - 8, 3, 3, 1, GREEN);
        }
        UiIcon::Core => {
            for step in 0..size - 6 {
                renderer.rect(x + 3 + step, y + 3 + step, 4, 4, color);
                renderer.rect(x + size - 7 - step, y + 3 + step, 4, 4, 0x00AFC7D8);
            }
            renderer.rounded_rect(x, y, 10, 10, 5, color);
            renderer.rounded_rect(x + size - 10, y, 10, 10, 2, 0x00AFC7D8);
        }
        UiIcon::Docker => {
            renderer.rounded_rect(
                x + 3,
                y + size / 2,
                (size - 6) as u32,
                (size / 3) as u32,
                size / 6,
                color,
            );
            for row in 0..2 {
                for col in 0..3 {
                    renderer.rect(x + 7 + col * 6, y + 4 + row * 6, 4, 4, color);
                }
            }
            renderer.rect(x + size - 7, y + size / 2 - 3, 6, 3, color);
        }
        UiIcon::Monitor => {
            renderer.rounded_rect(x, y, size as u32, (size * 3 / 4) as u32, 2, 0x00CFD8DF);
            renderer.rect(
                x + 3,
                y + 3,
                (size - 6) as u32,
                (size * 3 / 4 - 6) as u32,
                0x00203848,
            );
            renderer.rect(center_x - 2, y + size * 3 / 4, 4, (size / 6) as u32, color);
            renderer.rect(x + size / 4, y + size - 4, (size / 2) as u32, 3, color);
        }
        UiIcon::Warning => {
            for row in 0..size - 2 {
                let width = (row * (size - 2) / (size - 2)).max(1);
                renderer.rect(center_x - width / 2, y + row, width as u32, 1, YELLOW);
            }
            renderer.rect(center_x - 1, y + size / 3, 3, (size / 3) as u32, BG);
            renderer.rounded_rect(center_x - 1, y + size * 3 / 4, 3, 3, 1, BG);
        }
        UiIcon::Restart => {
            renderer.rounded_rect(x, y, size as u32, size as u32, 4, 0x00DDE5EC);
            renderer.rounded_rect(x + 3, y + 3, (size - 6) as u32, (size - 6) as u32, 3, BLUE);
            renderer.ring(center_x, center_y, size / 2 - 6, 3, 82.0, color);
            renderer.rect(x + size - 12, y + 5, 7, 4, color);
            renderer.rect(x + size - 8, y + 5, 4, 8, color);
        }
        UiIcon::Power => {
            renderer.ring(center_x, center_y + 2, size / 2 - 2, 4, 82.0, color);
            renderer.rect(center_x - 2, y, 4, (size / 2 + 2) as u32, color);
            renderer.rounded_rect(center_x - 4, y - 1, 8, 8, 4, RED);
            renderer.rect(center_x - 2, y, 4, (size / 2) as u32, color);
        }
    }
}

fn draw_page_dots(renderer: &Renderer, page: Page) {
    for index in 0..4 {
        let active = index == page.index();
        let width = if active { 30 } else { 12 };
        let x = 146 + index as i32 * 18;
        renderer.rounded_rect(x, 934, width, 12, 6, if active { CYAN } else { LINE });
    }
}

fn draw_overlay(renderer: &Renderer, overlay: Overlay) {
    renderer.set_origin_x(0);
    renderer.blend_rect(0, 0, renderer.width(), renderer.height(), 0x00000000, 204);
    renderer.rounded_rect(20, 348, 336, 266, 20, RED);
    renderer.rounded_rect(22, 350, 332, 262, 18, CARD);
    let (line1, line2) = match overlay {
        Overlay::Confirm(PowerAction::Reboot) => ("确定重启 NAS?", "服务将暂时中断。"),
        Overlay::Confirm(PowerAction::Shutdown) => ("确定关闭 NAS?", "需手动开机恢复。"),
        Overlay::Executing(PowerAction::Reboot) => ("重启中...", ""),
        Overlay::Executing(PowerAction::Shutdown) => ("关机中...", ""),
        Overlay::None => return,
    };
    renderer.text_center(188, 386, line1, 6, TEXT);
    renderer.text_center(188, 435, line2, 6, TEXT);
    if matches!(overlay, Overlay::Confirm(_)) {
        renderer.rounded_rect(46, MODAL_BUTTON_Y, 136, 84, 16, CARD_ALT);
        renderer.text_center_bold_in_rect(114, MODAL_BUTTON_Y, 84, "取消", 6, TEXT);
        renderer.rounded_rect(195, MODAL_BUTTON_Y, 135, 84, 16, RED);
        renderer.text_center_bold_in_rect(262, MODAL_BUTTON_Y, 84, "确认", 6, TEXT);
    }
}

fn card(renderer: &Renderer, x: i32, y: i32, width: u32, height: u32, border: u32) {
    renderer.rounded_rect(x, y, width, height, 16, border);
    renderer.rounded_rect(x + 1, y + 1, width - 2, height - 2, 15, CARD);
}

fn badge(renderer: &Renderer, x: i32, y: i32, text: &str, background: u32, foreground: u32) -> i32 {
    let width = renderer.text_width(text, 1) as i32 + 20;
    renderer.rounded_rect(x, y, width as u32, 30, 8, background);
    renderer.text_center_bold_in_rect(x + width / 2, y, 30, text, 1, foreground);
    width + 5
}

fn status_tag(renderer: &Renderer, y: i32, text: &str, active: bool) {
    let width = renderer.text_width(text, 3) as i32 + 28;
    let x = 346 - width;
    renderer.rounded_rect(
        x,
        y,
        width as u32,
        38,
        12,
        if active { 0x0014331C } else { 0x003A1717 },
    );
    renderer.text_center_bold_in_rect(
        x + width / 2,
        y,
        38,
        text,
        3,
        if active { GREEN } else { RED },
    );
}

fn storage_height(data: &SystemData) -> i32 {
    80 + data.disks.iter().map(disk_row_height).sum::<i32>()
}

fn disk_row_height(disk: &DiskHealth) -> i32 {
    if disk.mounts.is_empty() {
        DISK_ROW_WITHOUT_USAGE
    } else {
        DISK_ROW_WITH_USAGE
    }
}

fn service_layout(data: &SystemData) -> (i32, i32) {
    let services_height =
        BLOCK_BASE_HEIGHT + data.services.len().max(1) as i32 * SERVICE_ROW_HEIGHT;
    (SERVICES_Y, SERVICES_Y + services_height + BLOCK_GAP)
}

fn vms_block_height(data: &SystemData) -> i32 {
    BLOCK_BASE_HEIGHT + data.vms.len().max(1) as i32 * VM_ROW_HEIGHT
}

fn content_height(data: &SystemData, page: Page) -> i32 {
    match page {
        Page::Overview => STORAGE_Y + storage_height(data) + 70,
        Page::Services => {
            service_layout(data).1
                + BLOCK_BASE_HEIGHT
                + data.docker.len().max(1) as i32 * DOCKER_ROW_HEIGHT
                + 70
        }
        Page::Vms => SERVICES_Y + vms_block_height(data) + 70,
        Page::Power => SERVICES_Y + POWER_BLOCK_HEIGHT + 70,
    }
}

fn aggregate_usage(disk: &DiskHealth) -> (f64, String) {
    let total: f64 = disk.mounts.iter().map(|mount| mount.total_gb).sum();
    let used: f64 = disk.mounts.iter().map(|mount| mount.used_gb).sum();
    let percent = if total > 0.0 {
        used / total * 100.0
    } else {
        0.0
    };
    (
        percent,
        format!(
            "{} / {}  {:.1}%",
            format_capacity(used),
            format_capacity((total - used).max(0.0)),
            percent
        ),
    )
}

fn format_capacity(value: f64) -> String {
    if value >= 1000.0 {
        format!("{:.1}T", value / 1000.0)
    } else {
        format!("{:.1}G", value)
    }
}

fn pick_net(networks: &[NetworkIface]) -> Option<&NetworkIface> {
    networks
        .iter()
        .filter(|item| {
            item.is_up
                && !item.ipv4.is_empty()
                && !item.name.starts_with("lo")
                && !item.name.starts_with("vnet")
                && !item.name.contains("ovs")
        })
        .max_by_key(|item| item.rx_bytes + item.tx_bytes)
}

fn split_speed(value: f64) -> (String, String) {
    if value >= 1_000_000_000.0 {
        (
            format!("{:.1}", value / 1_000_000_000.0),
            "GB/s".to_string(),
        )
    } else if value >= 1_000_000.0 {
        (format!("{:.1}", value / 1_000_000.0), "MB/s".to_string())
    } else if value >= 1_000.0 {
        (
            format!("{:.0}", (value / 1_000.0).round()),
            "KB/s".to_string(),
        )
    } else if value >= 1.0 {
        (format!("{:.0}", value.round()), "B/s".to_string())
    } else {
        ("0".to_string(), "B/s".to_string())
    }
}

fn bar_color(percent: f64) -> u32 {
    if percent >= 90.0 {
        RED
    } else if percent >= 75.0 {
        ORANGE
    } else {
        YELLOW
    }
}

fn fit_text(renderer: &Renderer, value: &str, scale: u32, maximum_width: u32) -> String {
    if renderer.text_width(value, scale) <= maximum_width {
        return value.to_string();
    }
    let suffix = "...";
    let suffix_width = renderer.text_width(suffix, scale);
    let mut result = String::new();
    for ch in value.chars() {
        let mut candidate = result.clone();
        candidate.push(ch);
        if renderer.text_width(&candidate, scale) + suffix_width > maximum_width {
            break;
        }
        result.push(ch);
    }
    result.push_str(suffix);
    result
}

fn inside(x: i32, y: i32, left: i32, top: i32, width: i32, height: i32) -> bool {
    x >= left && x < left + width && y >= top && y < top + height
}

fn visible(renderer: &Renderer, y: i32, height: i32) -> bool {
    y < renderer.height() as i32 && y.saturating_add(height) > 0
}

#[cfg(test)]
mod tests {
    use super::{hit_test, split_speed, HitTarget, Overlay, Page, PowerAction, SERVICES_Y};
    use crate::metrics::SystemData;

    #[test]
    fn pages_wrap_in_both_directions() {
        assert_eq!(Page::Overview.shifted(1), Page::Services);
        assert_eq!(Page::Services.shifted(1), Page::Vms);
        assert_eq!(Page::Vms.shifted(1), Page::Power);
        assert_eq!(Page::Power.shifted(1), Page::Overview);
        assert_eq!(Page::Overview.shifted(-1), Page::Power);
        assert_eq!(Page::Services.shifted(-1), Page::Overview);
        assert_eq!(Page::Power.shifted(0), Page::Power);
    }

    #[test]
    fn modal_buttons_are_the_only_confirm_targets() {
        let data = SystemData::default();
        let overlay = Overlay::Confirm(PowerAction::Reboot);
        assert_eq!(
            hit_test(&data, Page::Power, 0, overlay, 262, 540),
            Some(HitTarget::Confirm(PowerAction::Reboot))
        );
        assert_eq!(
            hit_test(&data, Page::Power, 0, overlay, 114, 540),
            Some(HitTarget::Cancel)
        );
        assert_eq!(hit_test(&data, Page::Power, 0, overlay, 188, 450), None);
    }

    #[test]
    fn power_buttons_match_master_layout() {
        let data = SystemData::default();
        let power_y = SERVICES_Y;
        assert_eq!(
            hit_test(&data, Page::Power, 0, Overlay::None, 188, power_y + 90),
            Some(HitTarget::Reboot)
        );
        assert_eq!(
            hit_test(&data, Page::Power, 0, Overlay::None, 188, power_y + 210),
            Some(HitTarget::Shutdown)
        );
        assert_eq!(
            hit_test(&data, Page::Vms, 0, Overlay::None, 188, power_y + 90),
            None
        );
    }

    #[test]
    fn speed_units_are_explicit_and_stable() {
        assert_eq!(split_speed(0.0), ("0".into(), "B/s".into()));
        assert_eq!(split_speed(850.0), ("850".into(), "B/s".into()));
        assert_eq!(split_speed(12_500.0), ("13".into(), "KB/s".into()));
        assert_eq!(split_speed(2_500_000.0), ("2.5".into(), "MB/s".into()));
    }
}
