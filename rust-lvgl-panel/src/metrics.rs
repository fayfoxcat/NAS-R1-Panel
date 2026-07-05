// System metric collection — reads /proc, /sys, and runs commands.
// Equivalent to the Go internal/*.go files, but in Rust.

use std::collections::HashMap;
use std::fs;
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// ── Public data structures ─────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct SystemData {
    pub cpu: CpuData,
    pub memory: MemoryData,
    pub gpu: GpuData,
    pub storage: Vec<StorageMount>,
    pub disks: Vec<DiskHealth>,
    pub network: Vec<NetworkIface>,
    pub docker: Vec<DockerContainer>,
    pub vms: Vec<VirtualMachine>,
    pub services: Vec<ServiceStatus>,
    pub uptime: UptimeData,
    pub hostname: String,
}

#[derive(Debug, Clone, Default)]
pub struct CpuData {
    pub percent: f64,
    pub count: u32,
    pub freq_mhz: Option<f64>,
    pub temperature_c: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct MemoryData {
    pub total_gb: f64,
    pub used_gb: f64,
    pub available_gb: f64,
    pub percent: f64,
    pub swap_total_gb: f64,
    pub swap_used_gb: f64,
    pub swap_percent: f64,
}

#[derive(Debug, Clone, Default)]
pub struct GpuData {
    pub name: String,
    pub freq_mhz: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct StorageMount {
    pub device: String,
    pub mount: String,
    pub fstype: String,
    pub total_gb: f64,
    pub used_gb: f64,
    pub free_gb: f64,
    pub percent: f64,
}

#[derive(Debug, Clone)]
pub struct DiskHealth {
    pub name: String,
    pub device: String,
    pub size: String,
    pub model: String,
    pub health: Option<String>,
    pub temperature: Option<f64>,
    pub power_on_hours: Option<f64>,
    pub percent_used: Option<f64>,
    pub disk_type: String,
    pub role: String,
    pub mounts: Vec<DiskMount>,
}

#[derive(Debug, Clone)]
pub struct DiskMount {
    pub mount: String,
    pub total_gb: f64,
    pub used_gb: f64,
    pub free_gb: f64,
    pub percent: f64,
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
    pub ipv6: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DockerContainer {
    pub names: String,
    pub state: String,
    pub status: String,
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
    pub str: String,
}

// ── Collection ─────────────────────────────────────────────

const GB: f64 = 1024.0 * 1024.0 * 1024.0;

pub fn collect() -> SystemData {
    SystemData {
        cpu: read_cpu(),
        memory: read_memory(),
        gpu: read_gpu(),
        storage: read_storage(),
        disks: read_disk_health(),
        network: read_network(),
        docker: read_docker(),
        vms: read_vms(),
        services: read_services(),
        uptime: read_uptime(),
        hostname: read_hostname(),
    }
}

// ── CPU ────────────────────────────────────────────────────

fn read_cpu() -> CpuData {
    let mut cpu = CpuData {
        count: num_cpus::get() as u32,
        ..Default::default()
    };

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
    if let Ok(data) =
        fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq")
    {
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

    let a = match read_times() {
        Some(v) => v,
        None => return 0.0,
    };
    std::thread::sleep(Duration::from_millis(200));
    let b = match read_times() {
        Some(v) => v,
        None => return 0.0,
    };

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

    let avail = if avail > 0 { avail } else { free + buffers + cached };
    let used = total - avail;
    let percent = if total > 0 {
        round1(used as f64 / total as f64 * 100.0)
    } else {
        0.0
    };

    let swap_total = *meminfo.get("SwapTotal").unwrap_or(&0);
    let swap_free = *meminfo.get("SwapFree").unwrap_or(&0);
    let swap_used = swap_total - swap_free;
    let swap_percent = if swap_total > 0 {
        round1(swap_used as f64 / swap_total as f64 * 100.0)
    } else {
        0.0
    };

    MemoryData {
        total_gb: round1(total as f64 / (1024.0 * 1024.0)),
        used_gb: round1(used as f64 / (1024.0 * 1024.0)),
        available_gb: round1(avail as f64 / (1024.0 * 1024.0)),
        percent,
        swap_total_gb: round1(swap_total as f64 / (1024.0 * 1024.0)),
        swap_used_gb: round1(swap_used as f64 / (1024.0 * 1024.0)),
        swap_percent,
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
    let mut gpu = GpuData {
        name: "Intel UHD Graphics (N100)".into(),
        ..Default::default()
    };
    if let Ok(data) = fs::read_to_string("/sys/class/drm/card1/gt_cur_freq_mhz") {
        if let Ok(f) = data.trim().parse() {
            gpu.freq_mhz = Some(f);
        }
    }
    gpu
}

// ── Storage ────────────────────────────────────────────────

fn read_storage() -> Vec<StorageMount> {
    let mut mounts = Vec::new();
    let data = match fs::read_to_string("/proc/mounts") {
        Ok(d) => d,
        Err(_) => return mounts,
    };

    let skip_fs = [
        "proc", "sysfs", "devtmpfs", "devpts", "tmpfs", "cgroup", "cgroup2",
        "pstore", "bpf", "securityfs", "debugfs", "tracefs", "hugetlbfs",
        "mqueue", "configfs", "fusectl", "ramfs",
    ];

    for line in data.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            continue;
        }
        let dev = parts[0];
        let mp = parts[1];
        let fstype = parts[2];

        if skip_fs.contains(&fstype) {
            continue;
        }

        // Use libc::statfs
        let total: u64;
        let avail: u64;
        let free: u64;
        unsafe {
            let mut stat: libc::statfs = std::mem::zeroed();
            let c_path = std::ffi::CString::new(mp).unwrap();
            if libc::statfs(c_path.as_ptr(), &mut stat) != 0 {
                continue;
            }
            total = stat.f_blocks * stat.f_bsize as u64;
            avail = stat.f_bavail * stat.f_bsize as u64;
            free = stat.f_bfree * stat.f_bsize as u64;
        }

        let used = total.saturating_sub(free);
        let percent = if total > 0 {
            round1(used as f64 / total as f64 * 100.0)
        } else {
            0.0
        };

        mounts.push(StorageMount {
            device: dev.to_string(),
            mount: mp.to_string(),
            fstype: fstype.to_string(),
            total_gb: round1(total as f64 / GB),
            used_gb: round1(used as f64 / GB),
            free_gb: round1(avail as f64 / GB),
            percent,
        });
    }
    mounts
}

// ── Disk Health ────────────────────────────────────────────

fn read_disk_health() -> Vec<DiskHealth> {
    let mut disks = Vec::new();
    let output = run_cmd("lsblk", &["-dn", "-o", "NAME,SIZE,TYPE,MODEL"]);
    if output.is_empty() {
        return disks;
    }

    // Get system disk
    let system_dev = run_cmd("findmnt", &["-n", "-o", "SOURCE", "/"])
        .trim_start_matches("/dev/")
        .trim_end_matches(|c: char| c.is_ascii_digit() || c == 'p')
        .to_string();

    for line in output.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 || parts[2] != "disk" {
            continue;
        }
        let name = parts[0];
        let size = parts[1];
        let model = if parts.len() > 3 { parts[3..].join(" ") } else { name.to_string() };

        let (role, disk_type) = classify_disk(name, &system_dev);
        let mounts = read_disk_mounts(name);

        let mut disk = DiskHealth {
            name: name.to_string(),
            device: format!("/dev/{}", name),
            size: size.to_string(),
            model,
            health: None,
            temperature: None,
            power_on_hours: None,
            percent_used: None,
            disk_type: disk_type.clone(),
            role,
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

fn classify_disk(name: &str, system_dev: &str) -> (&'static str, String) {
    if !system_dev.is_empty() && name.starts_with(system_dev) {
        let dt = if name.starts_with("mmc") { "emmc" } else { "disk" };
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

fn read_disk_mounts(_disk_name: &str) -> Vec<DiskMount> {
    // Simplified: iterate mounts from /proc/mounts and match by device prefix
    let mut mounts = Vec::new();
    if let Ok(data) = fs::read_to_string("/proc/mounts") {
        for line in data.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 2 {
                continue;
            }
            let dev = parts[0];
            let mp = parts[1];
            if !dev.starts_with("/dev/") {
                continue;
            }
            unsafe {
                let mut stat: libc::statfs = std::mem::zeroed();
                let c_path = std::ffi::CString::new(mp).unwrap();
                if libc::statfs(c_path.as_ptr(), &mut stat) != 0 {
                    continue;
                }
                let total = stat.f_blocks * stat.f_bsize as u64;
                let avail = stat.f_bavail * stat.f_bsize as u64;
                let free = stat.f_bfree * stat.f_bsize as u64;
                let used = total.saturating_sub(free);
                let percent = if total > 0 { round1(used as f64 / total as f64 * 100.0) } else { 0.0 };
                mounts.push(DiskMount {
                    mount: mp.to_string(),
                    total_gb: round1(total as f64 / GB),
                    used_gb: round1(used as f64 / GB),
                    free_gb: round1(avail as f64 / GB),
                    percent,
                });
            }
        }
    }
    mounts
}

fn read_emmc_wear(name: &str, disk: &mut DiskHealth) {
    let base = format!("/sys/block/{}/device/", name);
    if let Ok(data) = fs::read_to_string(format!("{}life_time", base)) {
        for val in data.split_whitespace() {
            if let Ok(v) = u32::from_str_radix(val.trim_start_matches("0x"), 16) {
                if (1..=11).contains(&v) {
                    let w = (v - 1) as f64 * 10.0;
                    if disk.percent_used.map_or(true, |p| w > p) {
                        disk.percent_used = Some(w);
                    }
                }
            }
        }
    }
    if let Ok(data) = fs::read_to_string(format!("{}pre_eol_info", base)) {
        if data.trim() == "0x02" {
            disk.health = Some("FAILED".to_string());
        }
    }
}

fn parse_smart(out: &str, disk: &mut DiskHealth) {
    for line in out.lines() {
        let low = line.to_lowercase();
        if low.contains("overall-health") {
            if low.contains("passed") {
                disk.health = Some("PASSED".to_string());
            } else if low.contains("failed") {
                disk.health = Some("FAILED".to_string());
            }
        } else if low.starts_with("temperature:") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if let Some(t) = parts.get(1).and_then(|s| s.parse().ok()) {
                disk.temperature = Some(t);
            }
        } else if low.contains("percentage used:") {
            if let Some(val) = line.split(':').nth(1) {
                let v = val.trim().trim_end_matches('%');
                if let Ok(f) = v.parse::<f64>() {
                    disk.percent_used = Some(f);
                }
            }
        } else if low.starts_with("power on hours:") {
            if let Some(val) = line.split(':').nth(1) {
                if let Ok(h) = val.trim().replace(',', "").parse::<f64>() {
                    disk.power_on_hours = Some(h);
                }
            }
        }
    }
}

// ── Network ────────────────────────────────────────────────

fn read_network() -> Vec<NetworkIface> {
    let curr = read_net_dev();
    let mut ifaces = Vec::new();

    for (name, (rx, tx)) in &curr {
        // Read IPs from /sys/class/net/<name>/ (simplified)
        let ips_v4 = read_iface_ips(name);

        let is_up = std::path::Path::new(&format!("/sys/class/net/{}/operstate", name))
            .exists()
            .then(|| {
                fs::read_to_string(format!("/sys/class/net/{}/operstate", name))
                    .map(|s| s.trim() == "up")
                    .unwrap_or(false)
            })
            .unwrap_or(false);

        ifaces.push(NetworkIface {
            name: name.clone(),
            is_up,
            rx_bytes: *rx,
            tx_bytes: *tx,
            rx_speed: 0.0,
            tx_speed: 0.0,
            ipv4: ips_v4,
            ipv6: Vec::new(),
        });
    }

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
    let out = run_cmd("docker", &["ps", "-a", "--format", "{{.Names}}\t{{.State}}\t{{.Status}}"]);
    let mut containers = Vec::new();
    for line in out.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 3 {
            containers.push(DockerContainer {
                names: parts[0].to_string(),
                state: parts[1].to_string(),
                status: parts[2].to_string(),
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
        str: format!("{}天 {}时 {}分", days, hours, minutes),
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
