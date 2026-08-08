// Pure Rust framebuffer rendering with no runtime font or image dependencies.

use fontdue::{Font, FontSettings, Metrics};
use std::cell::{Cell, UnsafeCell};
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;

pub struct Renderer {
    scanout: *mut u8,
    buffer: UnsafeCell<Box<[u8]>>,
    width: u32,
    height: u32,
    stride: u32,
    glyphs: HashMap<(char, u32), CachedGlyph>,
    ascents: [i32; 9],
    origin_x: Cell<i32>,
}

struct CachedGlyph {
    metrics: Metrics,
    bitmap: Vec<u8>,
}

impl Renderer {
    pub fn new(fb: *mut u8, width: u32, height: u32, stride: u32) -> Self {
        let (glyphs, ascents) = load_glyphs();
        Self {
            scanout: fb,
            buffer: UnsafeCell::new(vec![0; stride as usize * height as usize].into_boxed_slice()),
            width,
            height,
            stride,
            glyphs,
            ascents,
            origin_x: Cell::new(0),
        }
    }

    fn pixel_absolute(&self, x: i32, y: i32, color: u32) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        let offset = y as usize * self.stride as usize + x as usize * 4;
        unsafe {
            let buffer = &mut *self.buffer.get();
            *buffer.get_unchecked_mut(offset) = color as u8;
            *buffer.get_unchecked_mut(offset + 1) = (color >> 8) as u8;
            *buffer.get_unchecked_mut(offset + 2) = (color >> 16) as u8;
            *buffer.get_unchecked_mut(offset + 3) = 0;
        }
    }

    fn blend_pixel(&self, x: i32, y: i32, color: u32, alpha: u8) {
        let x = x + self.origin_x.get();
        if alpha == 0 || x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        if alpha == u8::MAX {
            self.pixel_absolute(x, y, color);
            return;
        }
        let offset = y as usize * self.stride as usize + x as usize * 4;
        let alpha = alpha as u32;
        let inverse = 255 - alpha;
        unsafe {
            let buffer = &mut *self.buffer.get();
            let channels = [color as u8, (color >> 8) as u8, (color >> 16) as u8];
            for (index, foreground) in channels.into_iter().enumerate() {
                let background = *buffer.get_unchecked(offset + index) as u32;
                *buffer.get_unchecked_mut(offset + index) =
                    ((foreground as u32 * alpha + background * inverse + 127) / 255) as u8;
            }
        }
    }

    pub(crate) fn rect(&self, x: i32, y: i32, width: u32, height: u32, color: u32) {
        if width == 0 || height == 0 {
            return;
        }
        let x = x + self.origin_x.get();
        let x0 = x.max(0);
        let y0 = y.max(0);
        let x1 = (x + width as i32).min(self.width as i32);
        let y1 = (y + height as i32).min(self.height as i32);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let color = color.to_le_bytes();
        let row_start = x0 as usize * 4;
        let row_end = x1 as usize * 4;
        let stride = self.stride as usize;
        let buffer = unsafe { &mut *self.buffer.get() };
        for py in y0..y1 {
            let start = py as usize * stride + row_start;
            let end = py as usize * stride + row_end;
            for pixel in buffer[start..end].chunks_exact_mut(4) {
                pixel.copy_from_slice(&color);
            }
        }
    }

    pub(crate) fn blend_rect(
        &self,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        color: u32,
        alpha: u8,
    ) {
        let x0 = x.max(0);
        let y0 = y.max(0);
        let x1 = (x + width as i32).min(self.width as i32);
        let y1 = (y + height as i32).min(self.height as i32);
        for py in y0..y1 {
            for px in x0..x1 {
                self.blend_pixel(px, py, color, alpha);
            }
        }
    }

    pub(crate) fn rgba_image(&self, x: i32, y: i32, width: u32, height: u32, pixels: &[u8]) {
        debug_assert_eq!(pixels.len(), (width * height * 4) as usize);
        for row in 0..height as usize {
            for col in 0..width as usize {
                let offset = (row * width as usize + col) * 4;
                let alpha = pixels[offset + 3];
                if alpha == 0 {
                    continue;
                }
                let color = ((pixels[offset] as u32) << 16)
                    | ((pixels[offset + 1] as u32) << 8)
                    | pixels[offset + 2] as u32;
                self.blend_pixel(x + col as i32, y + row as i32, color, alpha);
            }
        }
    }

    pub(crate) fn rounded_rect(
        &self,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        radius: i32,
        color: u32,
    ) {
        if radius <= 1 {
            self.rect(x, y, width, height, color);
            return;
        }
        if width == 0 || height == 0 {
            return;
        }

        // Fill the rectangular body using the fast scanline path. Only the four
        // radius-sized corners require distance/coverage calculations.
        let radius = radius.min(width as i32 / 2).min(height as i32 / 2).max(1);
        let diameter = radius * 2;
        self.rect(
            x + radius,
            y,
            width.saturating_sub(diameter as u32),
            height,
            color,
        );
        self.rect(
            x,
            y + radius,
            width,
            height.saturating_sub(diameter as u32),
            color,
        );

        let right = x + width as i32 - 1;
        let bottom = y + height as i32 - 1;
        for row in 0..radius {
            let dy = radius as f64 - row as f64 - 0.5;
            for col in 0..radius {
                let dx = radius as f64 - col as f64 - 0.5;
                let coverage = (radius as f64 + 0.5 - dx.hypot(dy)).clamp(0.0, 1.0);
                if coverage <= 0.0 {
                    continue;
                }
                let alpha = (coverage * 255.0).round() as u8;
                self.blend_pixel(x + col, y + row, color, alpha);
                self.blend_pixel(right - col, y + row, color, alpha);
                self.blend_pixel(x + col, bottom - row, color, alpha);
                self.blend_pixel(right - col, bottom - row, color, alpha);
            }
        }
    }

    pub(crate) fn ring(
        &self,
        cx: i32,
        cy: i32,
        radius: i32,
        thickness: i32,
        percent: f64,
        color: u32,
    ) {
        let sweep = percent.clamp(0.0, 100.0) / 100.0 * std::f64::consts::TAU;
        let inner = radius - thickness;
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let distance = (dx as f64).hypot(dy as f64);
                let coverage = (radius as f64 + 0.5 - distance)
                    .min(distance - inner as f64 + 0.5)
                    .clamp(0.0, 1.0);
                if coverage <= 0.0 {
                    continue;
                }
                // SVG circles start their dash at 3 o'clock and advance clockwise.
                let mut angle = (dy as f64).atan2(dx as f64);
                if angle < 0.0 {
                    angle += std::f64::consts::TAU;
                }
                if angle <= sweep {
                    self.blend_pixel(cx + dx, cy + dy, color, (coverage * 255.0).round() as u8);
                }
            }
        }
        if percent > 0.0 && percent < 100.0 {
            let cap_radius = (thickness / 2).max(1);
            let diameter = (cap_radius * 2) as u32;
            let path_radius = radius as f64 - thickness as f64 / 2.0;
            let start_x = cx + path_radius.round() as i32;
            let end_x = cx + (path_radius * sweep.cos()).round() as i32;
            let end_y = cy + (path_radius * sweep.sin()).round() as i32;
            self.rounded_rect(
                start_x - cap_radius,
                cy - cap_radius,
                diameter,
                diameter,
                cap_radius,
                color,
            );
            self.rounded_rect(
                end_x - cap_radius,
                end_y - cap_radius,
                diameter,
                diameter,
                cap_radius,
                color,
            );
        }
    }

    pub(crate) fn text(&self, x: i32, y: i32, text: &str, scale: u32, color: u32) {
        self.draw_text(x, y, text, scale, color, false);
    }

    pub(crate) fn text_bold(&self, x: i32, y: i32, text: &str, scale: u32, color: u32) {
        self.draw_text(x, y, text, scale, color, true);
    }

    fn draw_text(&self, x: i32, y: i32, text: &str, scale: u32, color: u32, bold: bool) {
        if self.glyphs.is_empty() {
            self.bitmap_text(x, y, text, scale, color);
            return;
        }
        let scale = scale.clamp(1, 8);
        let baseline = y + self.ascents[scale as usize];
        let mut pen_x = x as f32;

        for ch in text.chars() {
            let key = if self.glyphs.contains_key(&(ch, scale)) {
                (ch, scale)
            } else {
                ('?', scale)
            };
            let glyph = &self.glyphs[&key];
            let glyph_x = pen_x.round() as i32 + glyph.metrics.xmin;
            let glyph_y = baseline - glyph.metrics.height as i32 - glyph.metrics.ymin;
            for row in 0..glyph.metrics.height {
                for col in 0..glyph.metrics.width {
                    let glyph_alpha = glyph.bitmap[row * glyph.metrics.width + col];
                    self.blend_pixel(
                        glyph_x + col as i32,
                        glyph_y + row as i32,
                        color,
                        glyph_alpha,
                    );
                    if bold && glyph_alpha > 0 {
                        self.blend_pixel(
                            glyph_x + col as i32 + 1,
                            glyph_y + row as i32,
                            color,
                            glyph_alpha,
                        );
                    }
                }
            }
            pen_x += glyph.metrics.advance_width;
        }
    }

    fn bitmap_text(&self, x: i32, y: i32, text: &str, scale: u32, color: u32) {
        for (index, ch) in text.chars().enumerate() {
            let glyph = glyph(ch);
            let left = x + index as i32 * 6 * scale as i32;
            for (row, bits) in glyph.iter().enumerate() {
                for col in 0..5 {
                    if bits & (1 << (4 - col)) != 0 {
                        self.rect(
                            left + col * scale as i32,
                            y + row as i32 * scale as i32,
                            scale,
                            scale,
                            color,
                        );
                    }
                }
            }
        }
    }

    pub(crate) fn text_right(&self, right: i32, y: i32, text: &str, scale: u32, color: u32) {
        self.text(
            right - self.text_width(text, scale) as i32,
            y,
            text,
            scale,
            color,
        );
    }

    pub(crate) fn text_center(&self, center: i32, y: i32, text: &str, scale: u32, color: u32) {
        self.text(
            center - self.text_width(text, scale) as i32 / 2,
            y,
            text,
            scale,
            color,
        );
    }

    pub(crate) fn text_in_rect(
        &self,
        x: i32,
        rect_y: i32,
        rect_height: i32,
        text: &str,
        scale: u32,
        color: u32,
    ) {
        self.text(
            x,
            self.vertical_text_origin(rect_y, rect_height, text, scale),
            text,
            scale,
            color,
        );
    }

    pub(crate) fn text_bold_in_rect(
        &self,
        x: i32,
        rect_y: i32,
        rect_height: i32,
        text: &str,
        scale: u32,
        color: u32,
    ) {
        self.text_bold(
            x,
            self.vertical_text_origin(rect_y, rect_height, text, scale),
            text,
            scale,
            color,
        );
    }

    pub(crate) fn text_center_bold_in_rect(
        &self,
        center: i32,
        rect_y: i32,
        rect_height: i32,
        text: &str,
        scale: u32,
        color: u32,
    ) {
        self.text_bold(
            center - self.text_width(text, scale) as i32 / 2,
            self.vertical_text_origin(rect_y, rect_height, text, scale),
            text,
            scale,
            color,
        );
    }

    pub(crate) fn text_right_bold_in_rect(
        &self,
        right: i32,
        rect_y: i32,
        rect_height: i32,
        text: &str,
        scale: u32,
        color: u32,
    ) {
        self.text_bold(
            right - self.text_width(text, scale) as i32,
            self.vertical_text_origin(rect_y, rect_height, text, scale),
            text,
            scale,
            color,
        );
    }

    fn vertical_text_origin(&self, rect_y: i32, rect_height: i32, text: &str, scale: u32) -> i32 {
        let (top, bottom) = self.text_bounds(text, scale);
        rect_y + (rect_height - (bottom - top)) / 2 - top
    }

    fn text_bounds(&self, text: &str, scale: u32) -> (i32, i32) {
        if self.glyphs.is_empty() {
            return (0, 7 * scale.clamp(1, 8) as i32);
        }

        let scale = scale.clamp(1, 8);
        let baseline = self.ascents[scale as usize];
        let mut top = i32::MAX;
        let mut bottom = i32::MIN;
        for ch in text.chars() {
            let key = if self.glyphs.contains_key(&(ch, scale)) {
                (ch, scale)
            } else {
                ('?', scale)
            };
            let glyph = &self.glyphs[&key];
            let glyph_top = baseline - glyph.metrics.height as i32 - glyph.metrics.ymin;
            top = top.min(glyph_top);
            bottom = bottom.max(glyph_top + glyph.metrics.height as i32);
        }
        if top == i32::MAX {
            (0, 0)
        } else {
            (top, bottom)
        }
    }

    pub(crate) fn text_center_bold(&self, center: i32, y: i32, text: &str, scale: u32, color: u32) {
        self.text_bold(
            center - self.text_width(text, scale) as i32 / 2,
            y,
            text,
            scale,
            color,
        );
    }

    pub(crate) fn text_width(&self, text: &str, scale: u32) -> u32 {
        if self.glyphs.is_empty() {
            return text_width(text, scale);
        }
        let scale = scale.clamp(1, 8);
        let mut width: f32 = 0.0;
        for ch in text.chars() {
            let key = if self.glyphs.contains_key(&(ch, scale)) {
                (ch, scale)
            } else {
                ('?', scale)
            };
            width += self.glyphs[&key].metrics.advance_width;
        }
        width.ceil().max(0.0) as u32
    }

    pub(crate) fn present(&self) {
        unsafe {
            let buffer = &*self.buffer.get();
            std::ptr::copy_nonoverlapping(buffer.as_ptr(), self.scanout, buffer.len());
        }
    }

    /// Point the scanout copy at a different framebuffer (double buffering:
    /// draw into the back buffer, page-flip it, then point here at the new
    /// back buffer for the next frame).
    pub fn set_framebuffer(&mut self, fb: *mut u8) {
        self.scanout = fb;
    }

    pub fn save_bmp(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let width = self.width as usize;
        let height = self.height as usize;
        let row_size = (width * 3 + 3) & !3;
        let image_size = row_size * height;
        let mut writer = BufWriter::new(File::create(path)?);

        writer.write_all(b"BM")?;
        writer.write_all(&((54 + image_size) as u32).to_le_bytes())?;
        writer.write_all(&[0; 4])?;
        writer.write_all(&54u32.to_le_bytes())?;
        writer.write_all(&40u32.to_le_bytes())?;
        writer.write_all(&(self.width as i32).to_le_bytes())?;
        writer.write_all(&(self.height as i32).to_le_bytes())?;
        writer.write_all(&1u16.to_le_bytes())?;
        writer.write_all(&24u16.to_le_bytes())?;
        writer.write_all(&0u32.to_le_bytes())?;
        writer.write_all(&(image_size as u32).to_le_bytes())?;
        writer.write_all(&2835u32.to_le_bytes())?;
        writer.write_all(&2835u32.to_le_bytes())?;
        writer.write_all(&0u32.to_le_bytes())?;
        writer.write_all(&0u32.to_le_bytes())?;

        let padding = vec![0u8; row_size - width * 3];
        let buffer = unsafe { &*self.buffer.get() };
        for y in (0..height).rev() {
            for x in 0..width {
                let offset = y * self.stride as usize + x * 4;
                writer.write_all(&buffer[offset..offset + 3])?;
            }
            writer.write_all(&padding)?;
        }
        writer.flush()
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub(crate) fn set_origin_x(&self, x: i32) {
        self.origin_x.set(x);
    }
}

fn font_size(scale: u32) -> f32 {
    match scale {
        0 | 1 => 21.0,
        2 => 22.0,
        3 => 23.0,
        4 => 26.0,
        5 => 28.0,
        6 => 34.0,
        7 => 36.0,
        8 => 40.0,
        value => value as f32 * 5.0,
    }
}

fn load_glyphs() -> (HashMap<(char, u32), CachedGlyph>, [i32; 9]) {
    let configured = std::env::var_os("R1_PANEL_FONT").map(std::path::PathBuf::from);
    let candidates = configured.into_iter().chain([
        std::path::PathBuf::from("/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf"),
        std::path::PathBuf::from("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"),
        std::path::PathBuf::from("/usr/share/fonts/truetype/noto/NotoMono-Regular.ttf"),
        std::path::PathBuf::from("/usr/share/fonts/truetype/noto/NotoSansMono-Regular.ttf"),
    ]);

    let mut glyphs = HashMap::new();
    let mut ascents = [0; 9];
    let mut chars: Vec<char> = (32u8..=126).map(char::from).collect();
    chars.extend(
        "概况服务天时分内存网络存储核心活跃停用运行停止虚拟机电源操作重启关机确定将暂中断关闭需手动开恢复取消确认正常告警系统损耗无数据容器℃·↓↑？，。"
            .chars(),
    );
    chars.sort_unstable();
    chars.dedup();

    for path in candidates {
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        match Font::from_bytes(bytes, FontSettings::default()) {
            Ok(font) => {
                for scale in 1..=8 {
                    let size = font_size(scale);
                    if ascents[scale as usize] == 0 {
                        ascents[scale as usize] = font
                            .horizontal_line_metrics(size)
                            .map(|metrics| metrics.ascent.ceil() as i32)
                            .unwrap_or(size.ceil() as i32);
                    }
                    for &ch in &chars {
                        if glyphs.contains_key(&(ch, scale)) || font.lookup_glyph_index(ch) == 0 {
                            continue;
                        }
                        let (metrics, bitmap) = font.rasterize(ch, size);
                        glyphs.insert((ch, scale), CachedGlyph { metrics, bitmap });
                    }
                }
                log::info!("Loaded UI glyphs from {}", path.display());
            }
            Err(error) => log::warn!("Cannot load font {}: {}", path.display(), error),
        }
    }

    if !glyphs.is_empty() {
        return (glyphs, ascents);
    }
    log::warn!("No TrueType font found; using the built-in bitmap fallback");
    (HashMap::new(), [0; 9])
}

fn text_width(text: &str, scale: u32) -> u32 {
    text.chars().count().saturating_mul(6).saturating_sub(1) as u32 * scale
}

fn glyph(ch: char) -> [u8; 7] {
    match ch.to_ascii_uppercase() {
        'A' => [14, 17, 17, 31, 17, 17, 17],
        'B' => [30, 17, 17, 30, 17, 17, 30],
        'C' => [14, 17, 16, 16, 16, 17, 14],
        'D' => [30, 17, 17, 17, 17, 17, 30],
        'E' => [31, 16, 16, 30, 16, 16, 31],
        'F' => [31, 16, 16, 30, 16, 16, 16],
        'G' => [14, 17, 16, 23, 17, 17, 15],
        'H' => [17, 17, 17, 31, 17, 17, 17],
        'I' => [31, 4, 4, 4, 4, 4, 31],
        'J' => [1, 1, 1, 1, 17, 17, 14],
        'K' => [17, 18, 20, 24, 20, 18, 17],
        'L' => [16, 16, 16, 16, 16, 16, 31],
        'M' => [17, 27, 21, 21, 17, 17, 17],
        'N' => [17, 25, 21, 19, 17, 17, 17],
        'O' => [14, 17, 17, 17, 17, 17, 14],
        'P' => [30, 17, 17, 30, 16, 16, 16],
        'Q' => [14, 17, 17, 17, 21, 18, 13],
        'R' => [30, 17, 17, 30, 20, 18, 17],
        'S' => [15, 16, 16, 14, 1, 1, 30],
        'T' => [31, 4, 4, 4, 4, 4, 4],
        'U' => [17, 17, 17, 17, 17, 17, 14],
        'V' => [17, 17, 17, 17, 17, 10, 4],
        'W' => [17, 17, 17, 21, 21, 21, 10],
        'X' => [17, 17, 10, 4, 10, 17, 17],
        'Y' => [17, 17, 10, 4, 4, 4, 4],
        'Z' => [31, 1, 2, 4, 8, 16, 31],
        '0' => [14, 17, 19, 21, 25, 17, 14],
        '1' => [4, 12, 20, 4, 4, 4, 31],
        '2' => [14, 17, 1, 2, 4, 8, 31],
        '3' => [30, 1, 1, 14, 1, 1, 30],
        '4' => [2, 6, 10, 18, 31, 2, 2],
        '5' => [31, 16, 16, 30, 1, 1, 30],
        '6' => [14, 16, 16, 30, 17, 17, 14],
        '7' => [31, 1, 2, 4, 8, 8, 8],
        '8' => [14, 17, 17, 14, 17, 17, 14],
        '9' => [14, 17, 17, 15, 1, 1, 14],
        ':' => [0, 4, 4, 0, 4, 4, 0],
        '.' => [0, 0, 0, 0, 0, 12, 12],
        ',' => [0, 0, 0, 0, 4, 4, 8],
        '/' => [1, 2, 2, 4, 8, 8, 16],
        '-' => [0, 0, 0, 31, 0, 0, 0],
        '_' => [0, 0, 0, 0, 0, 0, 31],
        '%' => [25, 25, 2, 4, 8, 19, 19],
        '+' => [0, 4, 4, 31, 4, 4, 0],
        '(' => [2, 4, 8, 8, 8, 4, 2],
        ')' => [8, 4, 2, 2, 2, 4, 8],
        _ => [0; 7],
    }
}
