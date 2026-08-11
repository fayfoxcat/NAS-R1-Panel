// System metric collection — reads /proc, /sys, and runs commands.
// Equivalent to the Go internal/*.go files, but in Rust.

use std::collections::HashMap;
use std::fs;
use std::process::Command;
use std::sync::{Mutex, OnceLock};

use serde::Deserialize;

// ── Public data structures ─────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct SystemData {
    pub cpu: CpuData,
    pub memory: MemoryData,
    pub gpu: GpuData,
    pub disks: Vec<DiskHealth>,
    pub network: Vec<NetworkIface>,
    pub docker: Vec<DockerContainer>,
    pub vms: Vec<VirtualMachine>,
    pub services: Vec<ServiceStatus>,
    pub uptime: UptimeData,
    pub hostname: String,
}

/// Values that are cheap to sample and should remain responsive on screen.
/// This payload is intentionally separate from the command-heavy inventory
/// data so the UI can refresh every 500 ms without invoking smartctl/docker/
/// virsh/systemctl on the render path.
#[derive(Debug, Clone, Default)]
pub struct FastData {
    pub cpu: CpuData,
    pub memory: MemoryData,
    pub gpu: GpuData,
    pub network: Vec<NetworkIface>,
    pub uptime: UptimeData,
}

/// Values that change slowly and require external commands or filesystem
/// walks. These are refreshed independently every few seconds.
#[derive(Debug, Clone, Default)]
pub struct SlowData {
    pub disks: Vec<DiskHealth>,
    pub docker: Vec<DockerContainer>,
    pub vms: Vec<VirtualMachine>,
    pub services: Vec<ServiceStatus>,
    pub hostname: String,
}

impl SystemData {
    pub fn apply_fast(&mut self, fast: FastData) {
        self.cpu = fast.cpu;
        self.memory = fast.memory;
        self.gpu = fast.gpu;
        self.network = fast.network;
        self.uptime = fast.uptime;
    }

    pub fn apply_slow(&mut self, slow: SlowData) {
        self.disks = slow.disks;
        self.docker = slow.docker;
        self.vms = slow.vms;
        self.services = slow.services;
        self.hostname = slow.hostname;
    }
}

#[derive(Debug, Clone, Default)]
pub struct CpuData {
    pub percent: f64,
    pub freq_mhz: Option<f64>,
    pub temperature_c: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct MemoryData {
    pub total_gb: f64,
    pub used_gb: f64,
    pub percent: f64,
}

#[derive(Debug, Clone, Default)]
pub struct GpuData {
    pub freq_mhz: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct DiskHealth {
    pub name: String,
    pub size: String,
    pub model: String,
    pub health: Option<String>,
    pub temperature: Option<f64>,
    pub power_on_hours: Option<f64>,
    pub percent_used: Option<f64>,
    /// eMMC only: percentage of rated life already consumed, as a range
    /// derived from the EXT_CSD life_time estimate.
    pub life_range: Option<(u32, u32)>,
    /// HDD only: raw value of SMART attribute 5 (Reallocated_Sector_Ct).
    pub reallocated_sectors: Option<u64>,
    pub disk_type: String,
    pub role: String,
    pub mounts: Vec<DiskMount>,
}

#[derive(Debug, Clone)]
pub struct DiskMount {
    pub mount: String,
    pub total_gb: f64,
    pub used_gb: f64,
}

#[derive(Debug, Clone)]
pub struct NetworkIface {
    pub name: String,
    pub is_up: bool,
    pub rx_bytes: i64,
    pub tx_bytes: i64,
    pub rx_speed: f64,
    pub tx_speed: f64,
    pub ipv4: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DockerContainer {
    pub names: String,
    pub state: String,
}

#[derive(Debug, Clone)]
pub struct VirtualMachine {
    pub id: String,
    pub name: String,
    pub state: String,
}

#[derive(Debug, Clone)]
pub struct ServiceStatus {
    pub name: String,
    pub active: bool,
}

#[derive(Debug, Clone, Default)]
pub struct UptimeData {
    pub days: u64,
    pub hours: u64,
    pub minutes: u64,
}

// ── Collection ─────────────────────────────────────────────

const GB: f64 = 1024.0 * 1024.0 * 1024.0;

pub fn collect_fast() -> FastData {
    FastData {
        cpu: read_cpu(),
        memory: read_memory(),
        gpu: read_gpu(),
        network: read_network(),
        uptime: read_uptime(),
    }
}

pub fn collect_slow() -> SlowData {
    SlowData {
        disks: read_disk_health(),
        docker: read_docker(),
        vms: read_vms(),
        services: read_services(),
        hostname: read_hostname(),
    }
}

pub fn collect() -> SystemData {
    let mut data = SystemData::default();
    data.apply_fast(collect_fast());
    data.apply_slow(collect_slow());
    data
}

// ── CPU ────────────────────────────────────────────────────

fn read_cpu() -> CpuData {
    let mut cpu = CpuData::default();

    // CPU temperature from hwmon
    if let Ok(entries) = fs::read_dir("/sys/class/hwmon") {
        for entry in entries.flatten() {
            let base = entry.path();
            for i in 1..=5 {
                let input = base.join(format!("temp{}_input", i));
                if let Ok(data) = fs::read_to_string(&input) {
                    if let Ok(t) = data.trim().parse::<f64>() {
                        let tv = t / 1000.0;
                        if tv > 0.0 && tv < 150.0 {
                            cpu.temperature_c = Some(round1(tv));
                        }
                    }
                }
            }
        }
    }

    // Frequency from sysfs
    if let Ok(data) = fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq") {
        if let Ok(f) = data.trim().parse::<f64>() {
            cpu.freq_mhz = Some(round1(f / 1000.0));
        }
    }

    // CPU usage via /proc/stat
    cpu.percent = read_cpu_percent();

    cpu
}

fn read_cpu_percent() -> f64 {
    fn read_times() -> Option<Vec<f64>> {
        let data = fs::read_to_string("/proc/stat").ok()?;
        for line in data.lines() {
            if line.starts_with("cpu ") {
                return Some(
                    line.split_whitespace()
                        .skip(1)
                        .filter_map(|s| s.parse().ok())
                        .collect(),
                );
            }
        }
        None
    }

    static PREVIOUS: OnceLock<Mutex<Option<Vec<f64>>>> = OnceLock::new();
    let current = match read_times() {
        Some(v) => v,
        None => return 0.0,
    };

    let previous = PREVIOUS.get_or_init(|| Mutex::new(None));
    let mut previous = previous
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(old) = previous.replace(current.clone()) else {
        return 0.0;
    };
    let a = old;
    let b = current;

    if a.len() < 4 || b.len() < 4 {
        return 0.0;
    }

    let a_idle = a[3] + a.get(4).unwrap_or(&0.0);
    let b_idle = b[3] + b.get(4).unwrap_or(&0.0);
    let a_total: f64 = a.iter().sum();
    let b_total: f64 = b.iter().sum();

    let total_delta = b_total - a_total;
    let idle_delta = b_idle - a_idle;

    if total_delta <= 0.0 {
        return 0.0;
    }
    round1((1.0 - idle_delta / total_delta) * 100.0)
}

// ── Memory ─────────────────────────────────────────────────

fn read_memory() -> MemoryData {
    let meminfo = read_meminfo();
    let total = *meminfo.get("MemTotal").unwrap_or(&0);
    let avail = *meminfo.get("MemAvailable").unwrap_or(&0);
    let free = *meminfo.get("MemFree").unwrap_or(&0);
    let buffers = *meminfo.get("Buffers").unwrap_or(&0);
    let cached = *meminfo.get("Cached").unwrap_or(&0);

    let avail = if avail > 0 {
        avail
    } else {
        free + buffers + cached
    };
    let used = total.saturating_sub(avail);
    let percent = if total > 0 {
        round1(used as f64 / total as f64 * 100.0)
    } else {
        0.0
    };

    MemoryData {
        total_gb: round1(total as f64 / (1024.0 * 1024.0)),
        used_gb: round1(used as f64 / (1024.0 * 1024.0)),
        percent,
    }
}

fn read_meminfo() -> HashMap<String, i64> {
    let mut map = HashMap::new();
    if let Ok(data) = fs::read_to_string("/proc/meminfo") {
        for line in data.lines() {
            let parts: Vec<&str> = line.splitn(2, ':').collect();
            if parts.len() != 2 {
                continue;
            }
            let key = parts[0].trim().to_string();
            let val = parts[1].trim().replace(" kB", "").trim().to_string();
            if let Ok(n) = val.parse::<i64>() {
                map.insert(key, n);
            }
        }
    }
    map
}

// ── GPU ────────────────────────────────────────────────────

fn read_gpu() -> GpuData {
    let mut gpu = GpuData::default();
    if let Ok(data) = fs::read_to_string("/sys/class/drm/card1/gt_cur_freq_mhz") {
        if let Ok(f) = data.trim().parse() {
            gpu.freq_mhz = Some(f);
        }
    }
    gpu
}

// ── Disk Health ────────────────────────────────────────────

fn read_disk_health() -> Vec<DiskHealth> {
    let mut disks = Vec::new();
    let output = run_cmd("lsblk", &["-dn", "-o", "NAME,SIZE,TYPE,MODEL"]);
    if output.is_empty() {
        return disks;
    }

    // Get system disk
    let system_source = run_cmd("findmnt", &["-n", "-o", "SOURCE", "/"]);
    let system_dev = base_disk_name(system_source.trim());
    let mount_disk_map = build_mount_disk_map();

    for line in output.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 || parts[2] != "disk" {
            continue;
        }
        let name = parts[0];
        let size = parts[1];
        if (name.starts_with("mmcblk") && name.contains("boot")) || name.ends_with("rpmb") {
            continue;
        }
        let model = if parts.len() > 3 {
            parts[3..].join(" ")
        } else {
            name.to_string()
        };

        let (role, disk_type) = classify_disk(name, &system_dev);
        let mounts = read_disk_mounts(name, &mount_disk_map);

        let mut disk = DiskHealth {
            name: name.to_string(),
            size: size.to_string(),
            model,
            health: None,
            temperature: None,
            power_on_hours: None,
            percent_used: None,
            life_range: None,
            reallocated_sectors: None,
            disk_type: disk_type.clone(),
            role: role.to_string(),
            mounts,
        };

        // eMMC wear
        if name.starts_with("mmc") {
            read_emmc_wear(name, &mut disk);
        }

        // SMART
        let smart = run_cmd("smartctl", &["-H", "-A", &format!("/dev/{}", name)]);
        parse_smart(&smart, &mut disk);

        disks.push(disk);
    }

    // Sort by role
    let order = |r: &str| match r {
        "system" => 0,
        "ssd" => 1,
        "hdd" => 2,
        _ => 3,
    };
    disks.sort_by_key(|d| order(&d.role));

    disks
}

fn base_disk_name(source: &str) -> String {
    let name = source.trim_start_matches("/dev/");
    if name.starts_with("nvme") || name.starts_with("mmcblk") {
        if let Some(partition_marker) = name.rfind('p') {
            if name[partition_marker + 1..]
                .chars()
                .all(|character| character.is_ascii_digit())
            {
                return name[..partition_marker].to_string();
            }
        }
        return name.to_string();
    }
    name.trim_end_matches(|character: char| character.is_ascii_digit())
        .to_string()
}

fn classify_disk(name: &str, system_dev: &str) -> (&'static str, String) {
    if !system_dev.is_empty() && name.starts_with(system_dev) {
        let dt = if name.starts_with("mmc") {
            "emmc"
        } else {
            "disk"
        };
        ("system", dt.to_string())
    } else if name.starts_with("nvme") {
        ("ssd", "nvme".to_string())
    } else if name.starts_with("sd") {
        ("hdd", "disk".to_string())
    } else if name.starts_with("mmc") {
        ("other", "emmc".to_string())
    } else {
        ("other", "disk".to_string())
    }
}

#[derive(Deserialize)]
struct LsblkTree {
    #[serde(default)]
    blockdevices: Vec<LsblkDevice>,
}

#[derive(Deserialize)]
struct LsblkDevice {
    name: String,
    #[serde(rename = "type")]
    device_type: String,
    mountpoint: Option<String>,
    #[serde(default)]
    children: Vec<LsblkDevice>,
}

fn build_mount_disk_map() -> HashMap<String, Vec<String>> {
    let output = run_cmd("lsblk", &["-J", "-o", "NAME,TYPE,MOUNTPOINT"]);
    let Ok(tree) = serde_json::from_str::<LsblkTree>(&output) else {
        return HashMap::new();
    };
    fn walk(
        device: &LsblkDevice,
        disk_name: Option<&str>,
        result: &mut HashMap<String, Vec<String>>,
    ) {
        let disk_name = if device.device_type == "disk" {
            Some(device.name.as_str())
        } else {
            disk_name
        };
        if let (Some(mountpoint), Some(disk_name)) = (device.mountpoint.as_deref(), disk_name) {
            let disks = result.entry(mountpoint.to_string()).or_default();
            if !disks.iter().any(|item| item == disk_name) {
                disks.push(disk_name.to_string());
            }
        }
        for child in &device.children {
            walk(child, disk_name, result);
        }
    }
    let mut result = HashMap::new();
    for device in &tree.blockdevices {
        walk(device, None, &mut result);
    }
    result
}

fn read_disk_mounts(
    disk_name: &str,
    mount_disk_map: &HashMap<String, Vec<String>>,
) -> Vec<DiskMount> {
    let mut mounts = Vec::new();
    for (mountpoint, disks) in mount_disk_map {
        if !disks.iter().any(|item| item == disk_name) {
            continue;
        }
        unsafe {
            let mut stat: libc::statfs = std::mem::zeroed();
            let Ok(c_path) = std::ffi::CString::new(mountpoint.as_str()) else {
                continue;
            };
            if libc::statfs(c_path.as_ptr(), &mut stat) != 0 {
                continue;
            }
            let total = stat.f_blocks * stat.f_bsize as u64;
            let free = stat.f_bfree * stat.f_bsize as u64;
            let used = total.saturating_sub(free);
            mounts.push(DiskMount {
                mount: mountpoint.to_string(),
                total_gb: round1(total as f64 / GB),
                used_gb: round1(used as f64 / GB),
            });
        }
    }
    mounts.sort_by(|left, right| left.mount.cmp(&right.mount));
    mounts
}

fn read_emmc_wear(name: &str, disk: &mut DiskHealth) {
    let base = format!("/sys/block/{}/device/", name);
    // eMMC has no power-on-hours counter (EXT_CSD exposes none and smartctl
    // does not support MMC), so the closest life indicators are the EXT_CSD
    // life_time estimate, the pre_eol_info flag and the manufacturing date.
    // lsblk reports no model for MMC; its sysfs name is the device part
    // number (e.g. Y0S256), which is a better title than the block name.
    if disk.model == disk.name {
        if let Ok(data) = fs::read_to_string(format!("{}name", base)) {
            let device_name = data.trim();
            if !device_name.is_empty() {
                disk.model = device_name.to_string();
            }
        }
    }
    // EXT_CSD life_time (field 268/269): 0x0 = 0-10% consumed, 0x1 = 10-20%,
    // ... 0x9 = 90-100%, 0xA/0xB = beyond rated life. Two values are
    // reported (A/B for different memory types); the larger one is shown.
    if let Ok(data) = fs::read_to_string(format!("{}life_time", base)) {
        let mut worst = 0u32;
        for val in data.split_whitespace() {
            if let Ok(v) = u32::from_str_radix(val.trim_start_matches("0x"), 16) {
                worst = worst.max(v);
            }
        }
        if worst <= 0x0B {
            disk.life_range = Some((worst * 10, (worst + 1) * 10));
        }
    }
    if let Ok(data) = fs::read_to_string(format!("{}pre_eol_info", base)) {
        disk.health = match data.trim() {
            "0x01" | "0x1" => Some("PASSED".to_string()),
            "0x02" | "0x2" => Some("WARNING".to_string()),
            "0x03" | "0x3" => Some("FAILED".to_string()),
            _ => disk.health.take(),
        };
    }
}

fn parse_smart(out: &str, disk: &mut DiskHealth) {
    // ATA attribute lines end with the raw value, optionally followed by a
    // parenthesized breakdown, e.g. "194 Temperature_Celsius ... - 44 (0 11 0)".
    fn raw_value(line: &str) -> Option<f64> {
        line.split('(')
            .next()
            .unwrap_or(line)
            .split_whitespace()
            .last()
            .and_then(|value| value.replace(',', "").parse().ok())
    }
    for line in out.lines() {
        let low = line.to_lowercase();
        if low.contains("overall-health") {
            if low.contains("passed") {
                disk.health = Some("PASSED".to_string());
            } else if low.contains("failed") {
                disk.health = Some("FAILED".to_string());
            }
        } else if low.starts_with("temperature:") {
            // NVMe-style header line: "Temperature: 46 Celsius"
            let parts: Vec<&str> = line.split_whitespace().collect();
            if let Some(t) = parts.get(1).and_then(|s| s.parse().ok()) {
                disk.temperature = Some(t);
            }
        } else if low.contains("temperature_celsius") {
            // ATA attribute 194: "194 Temperature_Celsius ... - 44"
            if let Some(t) = raw_value(line) {
                disk.temperature = Some(t);
            }
        } else if low.contains("percentage used:") {
            if let Some(f) = low
                .split(':')
                .nth(1)
                .and_then(|v| v.trim().trim_end_matches('%').parse().ok())
            {
                disk.percent_used = Some(f);
            }
        } else if low.contains("power_on_hours") {
            // ATA attribute 9: "9 Power_On_Hours ... - 16402"
            if let Some(h) = raw_value(line) {
                disk.power_on_hours = Some(h);
            }
        } else if low.starts_with("power on hours:") {
            // NVMe-style header line: "Power On Hours: 23,230"
            if let Some(h) = low
                .split(':')
                .nth(1)
                .and_then(|v| v.trim().replace(',', "").parse().ok())
            {
                disk.power_on_hours = Some(h);
            }
        } else if low.contains("reallocated_sector") {
            // ATA attribute 5: "5 Reallocated_Sector_Ct ... - 0"
            if let Some(value) = raw_value(line) {
                disk.reallocated_sectors = Some(value as u64);
            }
        }
    }
}

// ── Network ────────────────────────────────────────────────

fn read_network() -> Vec<NetworkIface> {
    static PREVIOUS: OnceLock<Mutex<(std::time::Instant, HashMap<String, (i64, i64)>)>> =
        OnceLock::new();
    let curr = read_net_dev();
    let mut ifaces = Vec::new();
    let now = std::time::Instant::now();
    let previous = PREVIOUS.get_or_init(|| Mutex::new((now, HashMap::new())));
    let mut previous = previous
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let elapsed = now.duration_since(previous.0).as_secs_f64();

    for (name, (rx, tx)) in &curr {
        let ips_v4 = read_iface_ips(name);
        let is_up = fs::read_to_string(format!("/sys/class/net/{}/operstate", name))
            .map(|s| s.trim() == "up")
            .unwrap_or(false);

        let (rx_speed, tx_speed) = previous
            .1
            .get(name)
            .filter(|_| elapsed > 0.0)
            .map(|(old_rx, old_tx)| {
                (
                    rx.saturating_sub(*old_rx) as f64 / elapsed,
                    tx.saturating_sub(*old_tx) as f64 / elapsed,
                )
            })
            .unwrap_or((0.0, 0.0));

        ifaces.push(NetworkIface {
            name: name.clone(),
            is_up,
            rx_bytes: *rx,
            tx_bytes: *tx,
            rx_speed,
            tx_speed,
            ipv4: ips_v4,
        });
    }

    *previous = (now, curr);
    ifaces
}

fn read_iface_ips(name: &str) -> Vec<String> {
    // Read IPs from `ip addr show <name>` or /sys
    let output = run_cmd("ip", &["-4", "-o", "addr", "show", name]);
    let mut ips = Vec::new();
    for line in output.lines() {
        // Format: <num>: <name> inet <ip>/<prefix> ...
        if let Some(inet_pos) = line.find("inet ") {
            let rest = &line[inet_pos + 5..];
            if let Some(space_pos) = rest.find(|c: char| c.is_whitespace() || c == '/') {
                ips.push(rest[..space_pos].to_string());
            }
        }
    }
    ips
}

fn read_net_dev() -> HashMap<String, (i64, i64)> {
    let mut map = HashMap::new();
    let data = match fs::read_to_string("/proc/net/dev") {
        Ok(d) => d,
        Err(_) => return map,
    };
    for line in data.lines() {
        let parts: Vec<&str> = line.splitn(2, ':').collect();
        if parts.len() != 2 {
            continue;
        }
        let name = parts[0].trim().to_string();
        let fields: Vec<&str> = parts[1].split_whitespace().collect();
        if fields.len() < 9 {
            continue;
        }
        let rx: i64 = fields[0].parse().unwrap_or(0);
        let tx: i64 = fields[8].parse().unwrap_or(0);
        map.insert(name, (rx, tx));
    }
    map
}

// ── Docker ─────────────────────────────────────────────────

fn read_docker() -> Vec<DockerContainer> {
    let out = run_cmd(
        "docker",
        &["ps", "-a", "--format", "{{.Names}}\t{{.State}}"],
    );
    let mut containers = Vec::new();
    for line in out.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 2 {
            containers.push(DockerContainer {
                names: parts[0].to_string(),
                state: parts[1].to_string(),
            });
        }
    }
    containers
}

// ── VMs ────────────────────────────────────────────────────

fn read_vms() -> Vec<VirtualMachine> {
    let out = run_cmd("virsh", &["list", "--all"]);
    let mut vms = Vec::new();
    for (i, line) in out.lines().enumerate() {
        if i < 2 || line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            vms.push(VirtualMachine {
                id: parts[0].to_string(),
                name: parts[1].to_string(),
                state: parts[2..].join(" "),
            });
        }
    }
    vms
}

// ── Services ───────────────────────────────────────────────

fn read_services() -> Vec<ServiceStatus> {
    let names = [
        "docker.service",
        "libvirtd.service",
        "containerd.service",
        "NetworkManager.service",
        "cron.service",
    ];
    names
        .iter()
        .map(|n| {
            let out = run_cmd("systemctl", &["is-active", n]);
            ServiceStatus {
                name: n.trim_end_matches(".service").to_string(),
                active: out.trim() == "active",
            }
        })
        .collect()
}

// ── Uptime ─────────────────────────────────────────────────

fn read_uptime() -> UptimeData {
    let secs = fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|d| d.split_whitespace().next()?.parse::<f64>().ok())
        .unwrap_or(0.0) as u64;

    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let minutes = (secs % 3600) / 60;

    UptimeData {
        days,
        hours,
        minutes,
    }
}

fn read_hostname() -> String {
    fs::read_to_string("/etc/hostname")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "nas".to_string())
}

// ── Helpers ────────────────────────────────────────────────

fn run_cmd(cmd: &str, args: &[&str]) -> String {
    Command::new(cmd)
        .args(args)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

fn round1(v: f64) -> f64 {
    (v * 10.0 + 0.5).floor() / 10.0
}

#[cfg(test)]
mod tests {
    use super::base_disk_name;

    #[test]
    fn extracts_base_disk_from_linux_device_names() {
        assert_eq!(base_disk_name("/dev/mmcblk0p2"), "mmcblk0");
        assert_eq!(base_disk_name("/dev/nvme0n1p3"), "nvme0n1");
        assert_eq!(base_disk_name("/dev/sda1"), "sda");
        assert_eq!(base_disk_name("/dev/mmcblk0"), "mmcblk0");
        assert_eq!(base_disk_name("/dev/nvme1n1"), "nvme1n1");
    }
}
