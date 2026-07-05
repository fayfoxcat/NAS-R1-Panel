// DRM/KMS display backend — raw DRM IOCTL approach.
// Opens the DSI display via DRM, sets up KMS mode with dumb buffers.

use std::fs::{File, OpenOptions};
use std::os::fd::AsRawFd;
use std::os::unix::io::RawFd;
use std::ptr;

// DRM IOCTL constants
const DRM_IOCTL_BASE: u8 = b'd';
const DRM_IOC_READWRITE: u32 = 0xC0000000;
const DRM_IOC_SIZEOF: fn(u32) -> u32 = |n| n;

macro_rules! DRM_IOWR {
    ($nr:expr, $ty:ty) => {
        (DRM_IOC_READWRITE) | ((std::mem::size_of::<$ty>() as u32) << 16) | ((DRM_IOCTL_BASE as u32) << 8) | ($nr)
    };
}

const DRM_IOCTL_SET_MASTER: u64 = DRM_IOWR!(0x1e, drm_set_master);
const DRM_IOCTL_MODE_GETRESOURCES: u64 = DRM_IOWR!(0xa0, drm_mode_card_res);
const DRM_IOCTL_MODE_GETCONNECTOR: u64 = DRM_IOWR!(0xa7, drm_mode_get_connector);
const DRM_IOCTL_MODE_GETENCODER: u64 = DRM_IOWR!(0xa6, drm_mode_get_encoder);
const DRM_IOCTL_MODE_CREATE_DUMB: u64 = DRM_IOWR!(0xb2, drm_mode_create_dumb);
const DRM_IOCTL_MODE_MAP_DUMB: u64 = DRM_IOWR!(0xb3, drm_mode_map_dumb);
const DRM_IOCTL_MODE_DESTROY_DUMB: u64 = DRM_IOWR!(0xb4, drm_mode_destroy_dumb);
const DRM_IOCTL_MODE_ADDFB: u64 = DRM_IOWR!(0xae, drm_mode_fb_cmd);
const DRM_IOCTL_MODE_SETCRTC: u64 = DRM_IOWR!(0xa2, drm_mode_crtc);

// DRM structs (x86_64 Linux ABI)
#[repr(C)]
struct drm_set_master { __pad: u64 }

#[repr(C)]
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
    pad: u32,
}

#[repr(C)]
struct drm_mode_get_encoder {
    encoder_id: u32,
    encoder_type: u32,
    crtc_id: u32,
    possible_crtcs: u32,
    possible_clones: u32,
}

#[repr(C)]
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
struct drm_mode_map_dumb {
    handle: u32,
    pad: u32,
    offset: u64,
}

#[repr(C)]
struct drm_mode_destroy_dumb {
    handle: u32,
}

#[repr(C)]
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
    type: u32,
    name: [i8; 32],
}

#[repr(C)]
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

// Connector states
const DRM_MODE_CONNECTED: u32 = 1;

unsafe fn drm_ioctl<T>(fd: RawFd, cmd: u64, data: &mut T) -> Result<(), std::io::Error> {
    let ret = libc::ioctl(fd, cmd as libc::c_ulong, data as *mut T);
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
    fb_ptr: *mut u8,
    fb_size: usize,
    connector_name: String,
    crtc_id: u32,
    _connector_id: u32,
    _fb_id: u32,
    _dumb_handle: u32,
    _dumb_offset: u64,
    map_ptr: *mut libc::c_void,
}

impl DrmDisplay {
    pub fn open() -> Result<Self, String> {
        let card = find_drm_card().ok_or("No DRM card")?;
        let fd = OpenOptions::new()
            .read(true).write(true)
            .open(&card)
            .map_err(|e| format!("open {}: {}", card, e))?;
        let raw_fd = fd.as_raw_fd();
        log::info!("Opened {}", card);

        // Set master
        unsafe {
            let mut m: drm_set_master = unsafe { std::mem::zeroed() };
            drm_ioctl(raw_fd, DRM_IOCTL_SET_MASTER, &mut m)
                .map_err(|e| format!("set_master: {:?}", e))?;
        }

        // Get resources
        let (crtcs, connectors) = unsafe { get_resources(raw_fd)? };
        log::info!("DRM: {} crtcs, {} connectors", crtcs.len(), connectors.len());

        // Find connected connector
        let (conn_id, conn_name, mode) = unsafe { find_connected(raw_fd, &connectors)? };

        let w = mode.hdisplay as u32;
        let h = mode.vdisplay as u32;
        log::info!("Mode: {}x{} @ {}Hz ({})", w, h, mode.vrefresh, conn_name);

        // Find CRTC
        let crtc_id = unsafe { find_crtc_for_conn(raw_fd, conn_id, &crtcs)? };
        log::info!("CRTC: {}", crtc_id);

        // Create dumb buffer
        let (dumb_handle, stride, size) = unsafe { create_dumb(raw_fd, w, h)? };
        log::info!("Dumb: {}x{}, stride={}, size={}", w, h, stride, size);

        // Add framebuffer
        let fb_id = unsafe { add_fb(raw_fd, w, h, stride, dumb_handle)? };
        log::info!("FB id: {}", fb_id);

        // Map dumb buffer
        let (offset, map_ptr) = unsafe { map_dumb(raw_fd, dumb_handle, size)? };
        let fb_ptr = map_ptr as *mut u8;
        log::info!("Mapped at {:p} (offset {})", fb_ptr, offset);

        // Set CRTC
        unsafe { set_crtc(raw_fd, crtc_id, conn_id, fb_id, &mode)? };
        log::info!("CRTC set");

        Ok(DrmDisplay {
            fd,
            width: w,
            height: h,
            stride,
            fb_ptr,
            fb_size: size as usize,
            connector_name: conn_name,
            crtc_id,
            _connector_id: conn_id,
            _fb_id: fb_id,
            _dumb_handle: dumb_handle,
            _dumb_offset: offset,
            map_ptr,
        })
    }

    pub fn width(&self) -> u32 { self.width }
    pub fn height(&self) -> u32 { self.height }
    pub fn stride(&self) -> u32 { self.stride }
    pub fn fb_ptr(&self) -> *mut u8 { self.fb_ptr }
    pub fn connector_name(&self) -> &str { &self.connector_name }
    pub fn page_flip(&mut self) { /* no-op for dumb buffer */ }
}

impl Drop for DrmDisplay {
    fn drop(&mut self) {
        let raw_fd = self.fd.as_raw_fd();
        unsafe {
            // Disable CRTC
            let mut crtc: drm_mode_crtc = std::mem::zeroed();
            crtc.crtc_id = self.crtc_id;
            drm_ioctl(raw_fd, DRM_IOCTL_MODE_SETCRTC, &mut crtc).ok();

            // Unmap
            libc::munmap(self.map_ptr, self.fb_size);

            // Destroy dumb buffer
            let mut d: drm_mode_destroy_dumb = std::mem::zeroed();
            d.handle = self._dumb_handle;
            drm_ioctl(raw_fd, DRM_IOCTL_MODE_DESTROY_DUMB, &mut d).ok();
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
    let mut res: drm_mode_card_res = std::mem::zeroed();
    drm_ioctl(fd, DRM_IOCTL_MODE_GETRESOURCES, &mut res)
        .map_err(|e| format!("get_res: {:?}", e))?;

    let crtcs = read_u32s(fd, res.crtc_id_ptr, res.count_crtcs as usize);
    let connectors = read_u32s(fd, res.connector_id_ptr, res.count_connectors as usize);
    Ok((crtcs, connectors))
}

unsafe fn read_u32s(_fd: RawFd, ptr: u64, count: usize) -> Vec<u32> {
    if ptr == 0 || count == 0 { return Vec::new(); }
    let slice = std::slice::from_raw_parts(ptr as *const u32, count);
    slice.to_vec()
}

unsafe fn find_connected(fd: RawFd, connectors: &[u32]) -> Result<(u32, String, drm_mode_modeinfo), String> {
    for &conn_id in connectors {
        let mut modes_buf = [drm_mode_modeinfo { clock: 0, hdisplay: 0, hsync_start: 0, hsync_end: 0, htotal: 0, hskew: 0, vdisplay: 0, vsync_start: 0, vsync_end: 0, vtotal: 0, vscan: 0, vrefresh: 0, flags: 0, type_: 0, name: [0; 32] }; 32];

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
    match t {
        1 => "VGA", 2 => "DVI", 4 => "HDMI", 5 => "LVDS",
        14 => "DSI", 15 => "eDP", 16 => "Virtual", 17 => "DSI",
        _ => "Unknown",
    }.to_string()
}

unsafe fn find_crtc_for_conn(fd: RawFd, conn_id: u32, crtcs: &[u32]) -> Result<u32, String> {
    // Try encoder -> crtc
    let mut gc: drm_mode_get_connector = std::mem::zeroed();
    gc.connector_id = conn_id;
    gc.modes_ptr = ptr::null_mut() as u64;
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
    drm_ioctl(fd, DRM_IOCTL_MODE_ADDFB, &mut fb)
        .map_err(|e| format!("add_fb: {:?}", e))?;
    Ok(fb.fb_id)
}

unsafe fn map_dumb(fd: RawFd, handle: u32, size: u64) -> Result<(u64, *mut libc::c_void), String> {
    let mut md: drm_mode_map_dumb = std::mem::zeroed();
    md.handle = handle;
    drm_ioctl(fd, DRM_IOCTL_MODE_MAP_DUMB, &mut md)
        .map_err(|e| format!("map_dumb: {:?}", e))?;

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

unsafe fn set_crtc(fd: RawFd, crtc: u32, conn: u32, fb: u32, mode: &drm_mode_modeinfo) -> Result<(), String> {
    let mut sc: drm_mode_crtc = std::mem::zeroed();
    sc.crtc_id = crtc;
    sc.fb_id = fb;
    sc.x = 0;
    sc.y = 0;
    sc.count_connectors = 1;
    sc.set_connectors_ptr = (&conn as *const u32) as u64;
    sc.mode_valid = 1;
    sc.mode = *mode;
    drm_ioctl(fd, DRM_IOCTL_MODE_SETCRTC, &mut sc)
        .map_err(|e| format!("set_crtc: {:?}", e))?;
    Ok(())
}
