// DRM/KMS display backend — raw DRM IOCTL approach.
// Opens the DSI display via DRM, sets up KMS mode with dumb buffers.

use std::fs::{File, OpenOptions};
use std::os::fd::AsRawFd;
use std::os::unix::io::{FromRawFd, RawFd};
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

// DRM IOCTL constants
const DRM_IOCTL_BASE: u8 = b'd';
const DRM_IOC_READWRITE: u32 = 0xC0000000;
macro_rules! DRM_IOWR {
    ($nr:expr, $ty:ty) => {{
        let sz = std::mem::size_of::<$ty>() as u64;
        (DRM_IOC_READWRITE as u64) | (sz << 16) | ((DRM_IOCTL_BASE as u64) << 8) | ($nr as u64)
    }};
}

macro_rules! DRM_IO {
    ($nr:expr) => {
        ((DRM_IOCTL_BASE as u64) << 8) | ($nr as u64)
    };
}

const DRM_IOCTL_SET_MASTER: u64 = DRM_IO!(0x1e);
const DRM_IOCTL_MODE_GETRESOURCES: u64 = DRM_IOWR!(0xa0, drm_mode_card_res);
const DRM_IOCTL_MODE_GETCONNECTOR: u64 = DRM_IOWR!(0xa7, drm_mode_get_connector);
const DRM_IOCTL_MODE_GETENCODER: u64 = DRM_IOWR!(0xa6, drm_mode_get_encoder);
const DRM_IOCTL_MODE_CREATE_DUMB: u64 = DRM_IOWR!(0xb2, drm_mode_create_dumb);
const DRM_IOCTL_MODE_MAP_DUMB: u64 = DRM_IOWR!(0xb3, drm_mode_map_dumb);
const DRM_IOCTL_MODE_DESTROY_DUMB: u64 = DRM_IOWR!(0xb4, drm_mode_destroy_dumb);
const DRM_IOCTL_MODE_ADDFB: u64 = DRM_IOWR!(0xae, drm_mode_fb_cmd);
const DRM_IOCTL_MODE_SETCRTC: u64 = DRM_IOWR!(0xa2, drm_mode_crtc);
const DRM_IOCTL_MODE_PAGE_FLIP: u64 = DRM_IOWR!(0xb0, drm_mode_crtc_page_flip);

// DRM page-flip flags / events
const DRM_MODE_PAGE_FLIP_EVENT: u32 = 0x01;
const DRM_EVENT_FLIP_COMPLETE: u32 = 0x01;

// DRM structs (x86_64 Linux ABI)
#[repr(C)]
#[derive(Clone, Copy)]
struct drm_mode_card_res {
    fb_id_ptr: u64,
    crtc_id_ptr: u64,
    connector_id_ptr: u64,
    encoder_id_ptr: u64,
    count_fbs: u32,
    count_crtcs: u32,
    count_connectors: u32,
    count_encoders: u32,
    min_width: u32,
    max_width: u32,
    min_height: u32,
    max_height: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct drm_mode_get_connector {
    encoders_ptr: u64,
    modes_ptr: u64,
    props_ptr: u64,
    prop_values_ptr: u64,
    count_modes: u32,
    count_props: u32,
    count_encoders: u32,
    encoder_id: u32,
    connector_id: u32,
    connector_type: u32,
    connector_type_id: u32,
    connection: u32,
    mm_width: u32,
    mm_height: u32,
    subpixel: u32,
    _pad: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct drm_mode_get_encoder {
    encoder_id: u32,
    encoder_type: u32,
    crtc_id: u32,
    possible_crtcs: u32,
    possible_clones: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct drm_mode_create_dumb {
    height: u32,
    width: u32,
    bpp: u32,
    flags: u32,
    handle: u32,
    pitch: u32,
    size: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct drm_mode_map_dumb {
    handle: u32,
    pad: u32,
    offset: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct drm_mode_destroy_dumb {
    handle: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct drm_mode_fb_cmd {
    fb_id: u32,
    width: u32,
    height: u32,
    pitch: u32,
    bpp: u32,
    depth: u32,
    handle: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct drm_mode_modeinfo {
    clock: u32,
    hdisplay: u16,
    hsync_start: u16,
    hsync_end: u16,
    htotal: u16,
    hskew: u16,
    vdisplay: u16,
    vsync_start: u16,
    vsync_end: u16,
    vtotal: u16,
    vscan: u16,
    vrefresh: u32,
    flags: u32,
    r#type: u32,
    name: [i8; 32],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct drm_mode_crtc {
    set_connectors_ptr: u64,
    count_connectors: u32,
    crtc_id: u32,
    fb_id: u32,
    x: u32,
    y: u32,
    gamma_size: u32,
    mode_valid: u32,
    mode: drm_mode_modeinfo,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct drm_mode_crtc_page_flip {
    crtc_id: u32,
    fb_id: u32,
    flags: u32,
    reserved: u32,
    user_data: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct drm_event {
    type_: u32,
    length: u32,
}

// Connector states
const DRM_MODE_CONNECTED: u32 = 1;

unsafe fn drm_ioctl<T>(fd: RawFd, cmd: u64, data: &mut T) -> Result<(), std::io::Error> {
    let ret = libc::ioctl(fd, cmd as libc::Ioctl, data as *mut T);
    if ret < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

// ── Display struct ─────────────────────────────────────────

pub struct DrmDisplay {
    fd: File,
    width: u32,
    height: u32,
    stride: u32,
    /// fb id currently presented on the CRTC.
    front_fb: u32,
    /// fb id the next page flip will present.
    back_fb: u32,
    /// front fb id shared with the refresher thread so periodic SETCRTC
    /// always targets the buffer actually on screen.
    front_fb_shared: Arc<AtomicU32>,
    /// pointer to the back (off-screen) buffer the renderer draws into.
    back_ptr: *mut u8,
    fb_size: usize,
    connector_name: String,
    crtc_id: u32,
    connector_id: u32,
    mode: drm_mode_modeinfo,
    flip_pending: bool,
    _dumb_handles: (u32, u32),
    _offsets: (u64, u64),
    _map_ptrs: (*mut libc::c_void, *mut libc::c_void),
}

impl DrmDisplay {
    pub fn open() -> Result<Self, String> {
        let card = find_drm_card().ok_or("No DRM card")?;
        let fd = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&card)
            .map_err(|e| format!("open {}: {}", card, e))?;
        let raw_fd = fd.as_raw_fd();
        log::info!("Opened {}", card);

        // Set master
        let set_master = unsafe { libc::ioctl(raw_fd, DRM_IOCTL_SET_MASTER as libc::Ioctl, 0) };
        if set_master < 0 {
            return Err(format!("set_master: {:?}", std::io::Error::last_os_error()));
        }

        // Get resources
        let (crtcs, connectors) = unsafe { get_resources(raw_fd)? };
        log::info!(
            "DRM: {} crtcs, {} connectors",
            crtcs.len(),
            connectors.len()
        );

        // Find connected connector
        let (conn_id, conn_name, mode) = unsafe { find_connected(raw_fd, &connectors)? };

        let w = mode.hdisplay as u32;
        let h = mode.vdisplay as u32;
        log::info!("Mode: {}x{} @ {}Hz ({})", w, h, mode.vrefresh, conn_name);

        // Find CRTC
        let crtc_id = unsafe { find_crtc_for_conn(raw_fd, conn_id, &crtcs)? };
        log::info!("CRTC: {}", crtc_id);

        // Create two dumb buffers (front/back) for tear-free page flips.
        let (h0, stride, size) = unsafe { create_dumb(raw_fd, w, h)? };
        let (h1, _, _) = unsafe { create_dumb(raw_fd, w, h)? };
        log::info!("Dumb: {}x{}, stride={}, size={}", w, h, stride, size);

        let fb0 = unsafe { add_fb(raw_fd, w, h, stride, h0)? };
        let fb1 = unsafe { add_fb(raw_fd, w, h, stride, h1)? };
        log::info!("FB ids: {} {}", fb0, fb1);

        let (off0, map0) = unsafe { map_dumb(raw_fd, h0, size)? };
        let (off1, map1) = unsafe { map_dumb(raw_fd, h1, size)? };
        log::info!(
            "Mapped front at {:p}, back at {:p} (offset {})",
            map0,
            map1,
            off0
        );

        // Set CRTC to the front buffer.
        unsafe { set_crtc(raw_fd, crtc_id, conn_id, fb0, &mode)? };
        log::info!("CRTC set");

        Ok(DrmDisplay {
            fd,
            width: w,
            height: h,
            stride,
            front_fb: fb0,
            back_fb: fb1,
            front_fb_shared: Arc::new(AtomicU32::new(fb0)),
            back_ptr: map1 as *mut u8,
            fb_size: size as usize,
            connector_name: conn_name,
            crtc_id,
            connector_id: conn_id,
            mode,
            flip_pending: false,
            _dumb_handles: (h0, h1),
            _offsets: (off0, off1),
            _map_ptrs: (map0, map1),
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn stride(&self) -> u32 {
        self.stride
    }
    /// Pointer to the back buffer; the renderer draws into this one.
    pub fn fb_ptr(&self) -> *mut u8 {
        self.back_ptr
    }
    /// Pointer to the current back (off-screen) buffer for the renderer.
    pub fn render_ptr(&self) -> *mut u8 {
        self.back_ptr
    }
    pub fn connector_name(&self) -> &str {
        &self.connector_name
    }

    /// Present the back buffer with a page flip. Returns true when a flip was
    /// actually queued. If a previous flip is still pending the call is
    /// skipped (the frame will be presented on the next attempt).
    pub fn present(&mut self) -> bool {
        if self.flip_pending {
            return false;
        }
        let mut flip: drm_mode_crtc_page_flip = unsafe { std::mem::zeroed() };
        flip.crtc_id = self.crtc_id;
        flip.fb_id = self.back_fb;
        flip.flags = DRM_MODE_PAGE_FLIP_EVENT;
        let result = unsafe { drm_ioctl(self.fd.as_raw_fd(), DRM_IOCTL_MODE_PAGE_FLIP, &mut flip) };
        match result {
            Ok(()) => {
                self.flip_pending = true;
                std::mem::swap(&mut self.front_fb, &mut self.back_fb);
                std::mem::swap(&mut self._map_ptrs.0, &mut self._map_ptrs.1);
                self.back_ptr = self._map_ptrs.1 as *mut u8;
                self.front_fb_shared.store(self.front_fb, Ordering::SeqCst);
                true
            }
            Err(error) => {
                log::warn!("page_flip fb={}: {:?}", self.back_fb, error);
                false
            }
        }
    }

    /// Drain DRM events (flip completion) and clear the pending flag.
    pub fn poll_flip_events(&mut self) {
        if !self.flip_pending {
            return;
        }
        let mut fds = [libc::pollfd {
            fd: self.fd.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        }];
        let ready = unsafe { libc::poll(fds.as_mut_ptr(), 1, 0) };
        if ready <= 0 {
            return;
        }
        let mut buffer = [0u8; 512];
        loop {
            let size = match std::io::Read::read(&mut self.fd, &mut buffer) {
                Ok(size) => size,
                Err(_) => break,
            };
            if size == 0 {
                break;
            }
            let mut done = false;
            for chunk in buffer[..size].chunks(std::mem::size_of::<drm_event>()) {
                if chunk.len() < std::mem::size_of::<drm_event>() {
                    continue;
                }
                let event: drm_event = unsafe { std::ptr::read_unaligned(chunk.as_ptr().cast()) };
                if event.type_ == DRM_EVENT_FLIP_COMPLETE {
                    done = true;
                }
            }
            if done || size < buffer.len() {
                break;
            }
        }
        self.flip_pending = false;
    }

    /// Keep the DSI link alive from a background thread so a slow or stuck
    /// SETCRTC can never starve the touch/rendering loop. Refreshes every
    /// `interval`, and re-syncs shortly after a touch burst starts (the panel
    /// is most likely to stall during/after heavy interaction).
    pub fn spawn_link_refresher(&self, running: Arc<AtomicBool>, touch_kick: Arc<AtomicBool>) {
        let raw = unsafe { libc::dup(self.fd.as_raw_fd()) };
        if raw < 0 {
            log::warn!("dup drm fd failed: {}", std::io::Error::last_os_error());
            return;
        }
        let fd = unsafe { File::from_raw_fd(raw) };
        let crtc_id = self.crtc_id;
        let connector_id = self.connector_id;
        let front_fb = self.front_fb_shared.clone();
        let mode = self.mode;
        let light_interval = std::time::Duration::from_secs(30);
        let deep_interval = std::time::Duration::from_secs(300);
        std::thread::spawn(move || {
            let mut last_refresh = std::time::Instant::now();
            // The first full pipe cycle must wait until the system has fully
            // settled after boot. A pipe off/on cycle in the first minutes
            // after boot hits the i915 "Missing case (video_mode == 3)" DSI
            // bug and can leave the link dead (gray screen); the same cycle
            // after the link has stabilized is harmless.
            let mut last_deep =
                std::time::Instant::now() - deep_interval + std::time::Duration::from_secs(600);
            while running.load(Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_millis(500));
                if !running.load(Ordering::SeqCst) {
                    break;
                }
                let now = std::time::Instant::now();
                let kick = touch_kick.swap(false, Ordering::SeqCst);
                let kick_due =
                    kick && now.duration_since(last_refresh) >= std::time::Duration::from_secs(10);
                if now.duration_since(last_refresh) >= light_interval || kick_due {
                    let fb = front_fb.load(Ordering::SeqCst);
                    let started = std::time::Instant::now();
                    match unsafe { set_crtc(fd.as_raw_fd(), crtc_id, connector_id, fb, &mode) } {
                        Ok(()) => log::debug!(
                            "Periodic display refresh ok ({}ms, fb {})",
                            started.elapsed().as_millis(),
                            fb
                        ),
                        Err(error) => log::warn!("Periodic display refresh failed: {}", error),
                    }
                    last_refresh = std::time::Instant::now();
                }
                if now.duration_since(last_deep) >= deep_interval {
                    // A plain SETCRTC with identical parameters can be skipped
                    // by the driver. Cycle the pipe off and on to force a real
                    // modeset and DSI link retrain.
                    let fb = front_fb.load(Ordering::SeqCst);
                    let started = std::time::Instant::now();
                    let result = unsafe {
                        match set_crtc_disabled(fd.as_raw_fd(), crtc_id) {
                            Ok(()) => {
                                std::thread::sleep(std::time::Duration::from_millis(120));
                                set_crtc(fd.as_raw_fd(), crtc_id, connector_id, fb, &mode)
                            }
                            Err(error) => Err(error),
                        }
                    };
                    match result {
                        Ok(()) => log::info!(
                            "Deep display refresh ok ({}ms, fb {})",
                            started.elapsed().as_millis(),
                            fb
                        ),
                        Err(error) => log::warn!("Deep display refresh failed: {}", error),
                    }
                    last_deep = std::time::Instant::now();
                    last_refresh = std::time::Instant::now();
                }
            }
        });
    }
}

impl Drop for DrmDisplay {
    fn drop(&mut self) {
        let raw_fd = self.fd.as_raw_fd();
        unsafe {
            // Disable CRTC
            let mut crtc: drm_mode_crtc = std::mem::zeroed();
            crtc.crtc_id = self.crtc_id;
            drm_ioctl(raw_fd, DRM_IOCTL_MODE_SETCRTC, &mut crtc).ok();

            // Unmap both buffers
            libc::munmap(self._map_ptrs.0, self.fb_size);
            libc::munmap(self._map_ptrs.1, self.fb_size);

            // Destroy both dumb buffers
            for handle in [self._dumb_handles.0, self._dumb_handles.1] {
                let mut d: drm_mode_destroy_dumb = std::mem::zeroed();
                d.handle = handle;
                drm_ioctl(raw_fd, DRM_IOCTL_MODE_DESTROY_DUMB, &mut d).ok();
            }
        }
    }
}

// ── Internal helpers ───────────────────────────────────────

fn find_drm_card() -> Option<String> {
    for card in ["/dev/dri/card1", "/dev/dri/card0"] {
        if std::path::Path::new(card).exists() {
            return Some(card.to_string());
        }
    }
    None
}

unsafe fn get_resources(fd: RawFd) -> Result<(Vec<u32>, Vec<u32>), String> {
    // First call: get counts
    let mut res: drm_mode_card_res = std::mem::zeroed();
    drm_ioctl(fd, DRM_IOCTL_MODE_GETRESOURCES, &mut res)
        .map_err(|e| format!("get_res: {:?}", e))?;

    log::info!(
        "count_fbs={} count_crtcs={} count_connectors={} count_encoders={}",
        res.count_fbs,
        res.count_crtcs,
        res.count_connectors,
        res.count_encoders
    );

    // Second call: allocate buffers based on actual counts, re-fetch
    let nc = res.count_crtcs.max(1) as usize;
    let nconn = res.count_connectors.max(1) as usize;
    let nenc = res.count_encoders.max(1) as usize;
    let nfb = res.count_fbs.max(1) as usize;

    let mut crtc_buf = vec![0u32; nc];
    let mut conn_buf = vec![0u32; nconn];
    let mut enc_buf = vec![0u32; nenc];
    let mut fb_buf = vec![0u32; nfb];

    res.fb_id_ptr = fb_buf.as_mut_ptr() as u64;
    res.crtc_id_ptr = crtc_buf.as_mut_ptr() as u64;
    res.connector_id_ptr = conn_buf.as_mut_ptr() as u64;
    res.encoder_id_ptr = enc_buf.as_mut_ptr() as u64;
    res.count_fbs = nfb as u32;
    res.count_crtcs = nc as u32;
    res.count_connectors = nconn as u32;
    res.count_encoders = nenc as u32;

    drm_ioctl(fd, DRM_IOCTL_MODE_GETRESOURCES, &mut res)
        .map_err(|e| format!("get_res(2): {:?}", e))?;

    log::info!(
        "Got {} crtcs, {} connectors",
        res.count_crtcs,
        res.count_connectors
    );

    let crtcs = crtc_buf[..res.count_crtcs as usize].to_vec();
    let connectors = conn_buf[..res.count_connectors as usize].to_vec();
    Ok((crtcs, connectors))
}

unsafe fn find_connected(
    fd: RawFd,
    connectors: &[u32],
) -> Result<(u32, String, drm_mode_modeinfo), String> {
    for &conn_id in connectors {
        let mut modes_buf = [drm_mode_modeinfo {
            clock: 0,
            hdisplay: 0,
            hsync_start: 0,
            hsync_end: 0,
            htotal: 0,
            hskew: 0,
            vdisplay: 0,
            vsync_start: 0,
            vsync_end: 0,
            vtotal: 0,
            vscan: 0,
            vrefresh: 0,
            flags: 0,
            r#type: 0,
            name: [0; 32],
        }; 32];

        let mut gc: drm_mode_get_connector = std::mem::zeroed();
        gc.connector_id = conn_id;
        gc.modes_ptr = modes_buf.as_mut_ptr() as u64;
        gc.count_modes = modes_buf.len() as u32;

        drm_ioctl(fd, DRM_IOCTL_MODE_GETCONNECTOR, &mut gc)
            .map_err(|e| format!("get_conn: {:?}", e))?;

        if gc.connection == DRM_MODE_CONNECTED && gc.count_modes > 0 {
            let name = connector_type_name(gc.connector_type);
            return Ok((conn_id, name, modes_buf[0]));
        }
    }
    Err("No connected connector".to_string())
}

fn connector_type_name(t: u32) -> String {
    (match t {
        1 => "VGA",
        2 => "DVI-I",
        3 => "DVI-D",
        4 => "DVI-A",
        5 => "Composite",
        6 => "SVIDEO",
        7 => "LVDS",
        10 => "DisplayPort",
        11 => "HDMI-A",
        12 => "HDMI-B",
        13 => "TV",
        14 => "eDP",
        15 => "Virtual",
        16 => "DSI",
        17 => "DPI",
        18 => "Writeback",
        _ => "Unknown",
    })
    .to_string()
}

unsafe fn find_crtc_for_conn(fd: RawFd, conn_id: u32, crtcs: &[u32]) -> Result<u32, String> {
    // Try encoder -> crtc
    let mut gc: drm_mode_get_connector = std::mem::zeroed();
    gc.connector_id = conn_id;
    gc.modes_ptr = 0u64;
    drm_ioctl(fd, DRM_IOCTL_MODE_GETCONNECTOR, &mut gc)
        .map_err(|e| format!("get_conn2: {:?}", e))?;

    if gc.encoder_id != 0 {
        let mut ge: drm_mode_get_encoder = std::mem::zeroed();
        ge.encoder_id = gc.encoder_id;
        drm_ioctl(fd, DRM_IOCTL_MODE_GETENCODER, &mut ge)
            .map_err(|e| format!("get_enc: {:?}", e))?;
        if ge.crtc_id != 0 {
            return Ok(ge.crtc_id);
        }
    }
    crtcs.first().copied().ok_or("No CRTC".to_string())
}

unsafe fn create_dumb(fd: RawFd, w: u32, h: u32) -> Result<(u32, u32, u64), String> {
    let mut cd: drm_mode_create_dumb = std::mem::zeroed();
    cd.width = w;
    cd.height = h;
    cd.bpp = 32;
    drm_ioctl(fd, DRM_IOCTL_MODE_CREATE_DUMB, &mut cd)
        .map_err(|e| format!("create_dumb: {:?}", e))?;
    Ok((cd.handle, cd.pitch, cd.size))
}

unsafe fn add_fb(fd: RawFd, w: u32, h: u32, pitch: u32, handle: u32) -> Result<u32, String> {
    let mut fb: drm_mode_fb_cmd = std::mem::zeroed();
    fb.width = w;
    fb.height = h;
    fb.pitch = pitch;
    fb.bpp = 32;
    fb.depth = 24;
    fb.handle = handle;
    drm_ioctl(fd, DRM_IOCTL_MODE_ADDFB, &mut fb).map_err(|e| format!("add_fb: {:?}", e))?;
    Ok(fb.fb_id)
}

unsafe fn map_dumb(fd: RawFd, handle: u32, size: u64) -> Result<(u64, *mut libc::c_void), String> {
    let mut md: drm_mode_map_dumb = std::mem::zeroed();
    md.handle = handle;
    drm_ioctl(fd, DRM_IOCTL_MODE_MAP_DUMB, &mut md).map_err(|e| format!("map_dumb: {:?}", e))?;

    let ptr = libc::mmap(
        ptr::null_mut(),
        size as libc::size_t,
        libc::PROT_READ | libc::PROT_WRITE,
        libc::MAP_SHARED,
        fd,
        md.offset as libc::off_t,
    );
    if ptr == libc::MAP_FAILED {
        return Err("mmap failed".to_string());
    }
    Ok((md.offset, ptr))
}

unsafe fn set_crtc_disabled(fd: RawFd, crtc: u32) -> Result<(), String> {
    let mut sc: drm_mode_crtc = std::mem::zeroed();
    sc.crtc_id = crtc;
    sc.mode_valid = 0;
    drm_ioctl(fd, DRM_IOCTL_MODE_SETCRTC, &mut sc).map_err(|e| format!("disable_crtc: {:?}", e))
}

unsafe fn set_crtc(
    fd: RawFd,
    crtc: u32,
    conn: u32,
    fb: u32,
    mode: &drm_mode_modeinfo,
) -> Result<(), String> {
    let mut sc: drm_mode_crtc = std::mem::zeroed();
    sc.crtc_id = crtc;
    sc.fb_id = fb;
    sc.x = 0;
    sc.y = 0;
    sc.count_connectors = 1;
    sc.set_connectors_ptr = (&conn as *const u32) as u64;
    sc.mode_valid = 1;
    sc.mode = *mode;
    drm_ioctl(fd, DRM_IOCTL_MODE_SETCRTC, &mut sc).map_err(|e| format!("set_crtc: {:?}", e))?;
    Ok(())
}
