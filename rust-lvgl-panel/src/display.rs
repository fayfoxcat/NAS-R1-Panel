// DRM/KMS display backend — direct framebuffer rendering.
// Opens the DSI display via DRM, sets up KMS mode, and provides
// a memory-mapped framebuffer for LVGL to render into.

use std::fs::{self, File, OpenOptions};
use std::os::fd::AsRawFd;
use std::os::unix::io::RawFd;
use std::ptr;

pub struct DrmDisplay {
    fd: File,
    width: u32,
    height: u32,
    stride: u32,
    fb_ptr: *mut u8,
    fb_size: usize,
    connector_name: String,
    // DRM resources
    crtc_id: u32,
    connector_id: u32,
    mode: drm::control::Mode,
    fb_id: u32,
    // Dumb buffer handle
    handle: u32,
    // mmap handle
    map: *mut libc::c_void,
}

impl DrmDisplay {
    /// Open a DRM device and set up KMS with a dumb framebuffer.
    /// Tries card1 (DSI) first, then card0.
    pub fn open() -> Result<Self, String> {
        let card = find_dsi_card().unwrap_or_else(|| "/dev/dri/card0".to_string());
        let fd = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&card)
            .map_err(|e| format!("Failed to open {}: {}", card, e))?;
        log::info!("Opened {}", card);

        let raw_fd = fd.as_raw_fd();

        // Get DRM resources
        let res = drm_resources(raw_fd)?;
        log::info!("DRM: {} connectors, {} crtcs, {} encoders", res.count_connectors(), res.count_crtcs(), res.count_encoders());

        // Find a connected connector
        let (conn_id, mode, conn_name) = find_connected_connector(raw_fd, &res)?;
        log::info!("Connector: {} ({})", conn_name, mode.name());

        // Find a suitable CRTC
        let crtc_id = find_crtc(raw_fd, &res, conn_id)?;
        log::info!("CRTC: {}", crtc_id);

        // Create dumb buffer
        let (fb_id, handle, stride, size) =
            create_dumb_fb(raw_fd, mode.hdisplay as u32, mode.vdisplay as u32)?;
        log::info!(
            "Framebuffer: {}x{}, stride={}, size={}",
            mode.hdisplay,
            mode.vdisplay,
            stride,
            size
        );

        // mmap the dumb buffer
        let map = mmap_dumb(raw_fd, handle, size as u64)?;
        log::info!("Mapped framebuffer at {:p}", map);

        // Set CRTC mode
        set_crtc(raw_fd, crtc_id, conn_id, fb_id, &mode)?;
        log::info!("CRTC mode set: {}x{}", mode.hdisplay, mode.vdisplay);

        Ok(DrmDisplay {
            fd,
            width: mode.hdisplay as u32,
            height: mode.vdisplay as u32,
            stride,
            fb_ptr: map as *mut u8,
            fb_size: size as usize,
            connector_name: conn_name,
            crtc_id,
            connector_id: conn_id,
            mode,
            fb_id,
            handle,
            map,
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
    pub fn fb_ptr(&self) -> *mut u8 {
        self.fb_ptr
    }
    pub fn connector_name(&self) -> &str {
        &self.connector_name
    }

    /// Perform a page flip to show the rendered frame.
    /// For dumb buffers on simple drivers, this may be a no-op
    /// (the framebuffer is always scanned out).
    pub fn page_flip(&mut self) {
        // For simple dumb-buffer setup without page-flipping support,
        // the framebuffer is continuously scanned out. No-op.
        // For proper vsync, we'd create two buffers and use drmModePageFlip.
    }
}

impl Drop for DrmDisplay {
    fn drop(&mut self) {
        unsafe {
            // Unmap
            libc::munmap(self.map, self.fb_size);
        }
        // Destroy dumb buffer
        let mut req = drm::control::drm_mode_destroy_dumb {
            handle: self.handle,
        };
        unsafe {
            drm_ioctl(self.fd.as_raw_fd(), drm::control::DRM_IOCTL_MODE_DESTROY_DUMB, &mut req)
                .ok();
        }
        // Restore original CRTC (disable)
        unsafe {
            drm_ioctl(
                self.fd.as_raw_fd(),
                drm::control::DRM_IOCTL_MODE_SETCRTC,
                &mut drm::control::drm_mode_crtc {
                    crtc_id: self.crtc_id,
                    fb_id: 0,
                    x: 0,
                    y: 0,
                    mode_valid: 0,
                    mode: unsafe { std::mem::zeroed() },
                    ..Default::default()
                },
            )
            .ok();
        }
    }
}

// ── Internal helpers ───────────────────────────────────────

fn find_dsi_card() -> Option<String> {
    // On the NAS, the DSI panel is on card1-DSI-1
    for card in ["/dev/dri/card1", "/dev/dri/card0"] {
        if std::path::Path::new(card).exists() {
            // Check if this card has a DSI connector
            if let Ok(fd) = File::open(card) {
                if let Ok(res) = drm_resources(fd.as_raw_fd()) {
                    let connectors = res.connectors();
                    for &conn_id in connectors {
                        if let Ok(info) = drm_connector(fd.as_raw_fd(), conn_id) {
                            let name =
                                info.interface().map(|i| format!("{:?}", i)).unwrap_or_default();
                            let status = info.state();
                            if name.contains("DSI") || status == drm::control::connector::State::Connected {
                                return Some(card.to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

fn find_connected_connector(
    fd: RawFd,
    res: &drm::control::ResourceHandles,
) -> Result<(u32, drm::control::Mode, String), String> {
    for &conn_id in res.connectors() {
        let info = drm_connector(fd, conn_id)?;
        if info.state() == drm::control::connector::State::Connected {
            let name = info
                .interface()
                .map(|i| format!("{:?}", i))
                .unwrap_or_else(|| "Unknown".to_string());
            if let Some(mode) = info.modes().first() {
                return Ok((conn_id, *mode, name));
            }
        }
    }
    Err("No connected connector found".to_string())
}

fn find_crtc(
    fd: RawFd,
    res: &drm::control::ResourceHandles,
    conn_id: u32,
) -> Result<u32, String> {
    // Try to get the current CRTC for this connector
    let info = drm_connector(fd, conn_id)?;
    if let Some(encoder_id) = info.current_encoder() {
        if let Ok(enc) = drm_encoder(fd, encoder_id) {
            if let Some(crtc) = enc.current_crtc() {
                return Ok(crtc);
            }
        }
    }
    // Fallback: first available CRTC
    res.crtcs()
        .first()
        .copied()
        .ok_or_else(|| "No CRTC available".to_string())
}

fn create_dumb_fb(
    fd: RawFd,
    width: u32,
    height: u32,
) -> Result<(u32, u32, u32, u64), String> {
    // Create dumb buffer
    let mut create = drm::control::drm_mode_create_dumb {
        width,
        height,
        bpp: 32,
        flags: 0,
        handle: 0,
        pitch: 0,
        size: 0,
    };
    unsafe {
        drm_ioctl(fd, drm::control::DRM_IOCTL_MODE_CREATE_DUMB, &mut create)
            .map_err(|e| format!("DUMB_CREATE failed: {:?}", e))?;
    }

    let handle = create.handle;
    let stride = create.pitch;
    let size = create.size;

    // Add framebuffer
    let mut fb = drm::control::drm_mode_fb_cmd {
        width,
        height,
        pitch: stride,
        bpp: 32,
        depth: 24,
        handle,
        ..Default::default() // zero-initialize padding
    };
    unsafe {
        drm_ioctl(fd, drm::control::DRM_IOCTL_MODE_ADDFB, &mut fb)
            .map_err(|e| format!("ADDFB failed: {:?}", e))?;
    }

    Ok((fb.fb_id, handle, stride, size))
}

fn mmap_dumb(fd: RawFd, handle: u32, size: u64) -> Result<*mut libc::c_void, String> {
    let mut map_req = drm::control::drm_mode_map_dumb {
        handle,
        pad: 0,
        offset: 0,
    };
    unsafe {
        drm_ioctl(fd, drm::control::DRM_IOCTL_MODE_MAP_DUMB, &mut map_req)
            .map_err(|e| format!("MAP_DUMB failed: {:?}", e))?;
    }

    let ptr = unsafe {
        libc::mmap(
            ptr::null_mut(),
            size as libc::size_t,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED,
            fd,
            map_req.offset as libc::off_t,
        )
    };

    if ptr == libc::MAP_FAILED {
        return Err("mmap failed".to_string());
    }

    Ok(ptr)
}

fn set_crtc(
    fd: RawFd,
    crtc_id: u32,
    conn_id: u32,
    fb_id: u32,
    mode: &drm::control::Mode,
) -> Result<(), String> {
    let mut crtc_req = drm::control::drm_mode_crtc {
        crtc_id,
        fb_id,
        x: 0,
        y: 0,
        set_connectors_ptr: &conn_id as *const u32 as u64,
        count_connectors: 1,
        mode_valid: 1,
        mode: drm_mode_from_control(mode),
        ..Default::default()
    };

    unsafe {
        drm_ioctl(fd, drm::control::DRM_IOCTL_MODE_SETCRTC, &mut crtc_req)
            .map_err(|e| format!("SETCRTC failed: {:?}", e))?;
    }
    Ok(())
}

// ── DRM wrapper helpers ────────────────────────────────────

fn drm_resources(
    fd: RawFd,
) -> Result<drm::control::ResourceHandles, String> {
    let f = std::fs::File::from(unsafe { std::os::fd::FromRawFd::from_raw_fd(fd) });
    let res = drm::control::Device::new(f)
        .map_err(|e| format!("Device::new: {:?}", e))?
        .resource_handles()
        .map_err(|e| format!("resources: {:?}", e))?;
    Ok(res)
}

fn drm_connector(
    fd: RawFd,
    conn_id: u32,
) -> Result<drm::control::connector::Info, String> {
    let f = std::fs::File::from(unsafe { std::os::fd::FromRawFd::from_raw_fd(fd) });
    let info = drm::control::Device::new(f)
        .map_err(|e| format!("Device::new: {:?}", e))?
        .get_connector(conn_id, None)
        .map_err(|e| format!("connector: {:?}", e))?;
    Ok(info)
}

fn drm_encoder(
    fd: RawFd,
    encoder_id: u32,
) -> Result<drm::control::encoder::Info, String> {
    let f = std::fs::File::from(unsafe { std::os::fd::FromRawFd::from_raw_fd(fd) });
    let info = drm::control::Device::new(f)
        .map_err(|e| format!("Device::new: {:?}", e))?
        .get_encoder(encoder_id)
        .map_err(|e| format!("encoder: {:?}", e))?;
    Ok(info)
}

fn drm_mode_from_control(mode: &drm::control::Mode) -> drm::control::drm_mode_modeinfo {
    drm::control::drm_mode_modeinfo {
        clock: mode.clock() as u32,
        hdisplay: mode.hdisplay(),
        hsync_start: mode.hsync_start(),
        hsync_end: mode.hsync_end(),
        htotal: mode.htotal(),
        vdisplay: mode.vdisplay(),
        vsync_start: mode.vsync_start(),
        vsync_end: mode.vsync_end(),
        vtotal: mode.vtotal(),
        vscan: 0,
        vrefresh: mode.vrefresh() as u32,
        flags: mode.flags().bits(),
        ..Default::default()
    }
}

unsafe fn drm_ioctl<T>(fd: RawFd, cmd: u64, data: &mut T) -> Result<(), std::io::Error> {
    let ret = libc::ioctl(fd, cmd as libc::c_ulong, data as *mut T);
    if ret < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}
