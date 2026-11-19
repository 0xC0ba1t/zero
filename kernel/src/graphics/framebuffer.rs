//! The screen.
//!
//! Everything is drawn into a shadow buffer in ordinary RAM and copied to the
//! real framebuffer in one go by [`Framebuffer::flush`]. Reading and writing
//! video memory pixel by pixel is by far the slowest thing the kernel could
//! do, so drawing code never touches it directly; only the rows that changed
//! since the last flush are copied.

use crate::graphics::display::Display;
use alloc::vec;
use alloc::vec::Vec;
use bootloader_api::info::{FrameBufferInfo, PixelFormat};
use spin::Mutex;

pub static FRAMEBUFFER: Mutex<Option<Framebuffer>> = Mutex::new(None);

/// Runs `f` on the framebuffer, if there is one.
pub fn with<R>(f: impl FnOnce(&mut Framebuffer) -> R) -> Option<R> {
    FRAMEBUFFER.lock().as_mut().map(f)
}

pub struct Framebuffer {
    pub info: FrameBufferInfo,
    front: &'static mut [u8],
    back: Vec<u8>,
    /// Bytes per pixel.
    bpp: usize,
    /// Bytes per scan line.
    pitch: usize,
    /// Rows `dirty_lo..dirty_hi` differ between the shadow buffer and the screen.
    dirty_lo: usize,
    dirty_hi: usize,
}

impl Framebuffer {
    pub fn new(info: FrameBufferInfo, buffer: &'static mut [u8]) -> Self {
        let bpp = info.bytes_per_pixel.max(1);
        let pitch = info.stride * bpp;
        let back = vec![0u8; buffer.len()];
        let mut fb = Framebuffer {
            info,
            front: buffer,
            back,
            bpp,
            pitch,
            dirty_lo: 0,
            dirty_hi: 0,
        };
        fb.mark_dirty(0, info.height);
        fb
    }

    #[inline]
    fn mark_dirty(&mut self, lo: usize, hi: usize) {
        if self.dirty_lo == self.dirty_hi {
            self.dirty_lo = lo;
            self.dirty_hi = hi;
        } else {
            self.dirty_lo = self.dirty_lo.min(lo);
            self.dirty_hi = self.dirty_hi.max(hi);
        }
    }

    /// Lays a `0x00RRGGBB` color out the way the screen expects its channels.
    #[inline]
    pub fn pack(&self, color: u32) -> [u8; 4] {
        let r = (color >> 16) as u8;
        let g = (color >> 8) as u8;
        let b = color as u8;
        match self.info.pixel_format {
            PixelFormat::Rgb => [r, g, b, 0],
            PixelFormat::U8 => [((r as u16 + g as u16 + b as u16) / 3) as u8, 0, 0, 0],
            _ => [b, g, r, 0],
        }
    }

    /// Copies the rows that changed to the screen.
    pub fn flush(&mut self) {
        if self.dirty_lo >= self.dirty_hi {
            return;
        }
        let hi = self.dirty_hi.min(self.info.height);
        let start = self.dirty_lo * self.pitch;
        let end = (hi * self.pitch).min(self.front.len());
        if start < end {
            self.front[start..end].copy_from_slice(&self.back[start..end]);
        }
        self.dirty_lo = 0;
        self.dirty_hi = 0;
    }

    /// Moves the picture up by `dy` rows and fills the rows that open up at
    /// the bottom with `fill`.
    pub fn scroll_up(&mut self, dy: usize, fill: u32) {
        let h = self.info.height;
        if dy == 0 {
            return;
        }
        if dy >= h {
            self.clear_to(fill);
            return;
        }
        self.back.copy_within(dy * self.pitch..h * self.pitch, 0);
        self.fill_rect(0, h - dy, self.info.width, dy, fill);
        self.mark_dirty(0, h);
    }

    pub fn clear_to(&mut self, color: u32) {
        let (w, h) = (self.info.width, self.info.height);
        self.fill_rect(0, 0, w, h, color);
    }

    /// Fills `w` pixels of one row starting at `(x, y)`.
    #[inline]
    fn fill_span(&mut self, x: usize, y: usize, w: usize, px: [u8; 4]) {
        let off = y * self.pitch + x * self.bpp;
        let end = off + w * self.bpp;
        if end > self.back.len() {
            return;
        }
        let row = &mut self.back[off..end];
        match self.bpp {
            4 => {
                for c in row.chunks_exact_mut(4) {
                    c.copy_from_slice(&px);
                }
            }
            3 => {
                for c in row.chunks_exact_mut(3) {
                    c.copy_from_slice(&px[..3]);
                }
            }
            n => {
                for c in row.chunks_exact_mut(n) {
                    c.copy_from_slice(&px[..n.min(4)]);
                }
            }
        }
    }

    /// Draws one anti-aliased glyph: `alpha` rows give the coverage of the
    /// foreground color over the background color for each pixel.
    pub fn draw_alpha(&mut self, x: usize, y: usize, alpha: &[&[u8]], fg: u32, bg: u32) {
        let (w, h) = (self.info.width, self.info.height);
        let pfg = self.pack(fg);
        let pbg = self.pack(bg);
        let bpp = self.bpp;
        let mut rows = 0;
        for (dy, row) in alpha.iter().enumerate() {
            let yy = y + dy;
            if yy >= h {
                break;
            }
            rows += 1;
            let n = row.len().min(w.saturating_sub(x));
            let off = yy * self.pitch + x * bpp;
            if off + n * bpp > self.back.len() {
                break;
            }
            let dst = &mut self.back[off..off + n * bpp];
            for (a, d) in row[..n].iter().zip(dst.chunks_exact_mut(bpp)) {
                let a = *a as u32;
                if a == 0 {
                    d.copy_from_slice(&pbg[..bpp.min(4)]);
                } else if a == 255 {
                    d.copy_from_slice(&pfg[..bpp.min(4)]);
                } else {
                    let ia = 255 - a;
                    for c in 0..bpp.min(4) {
                        d[c] = ((pfg[c] as u32 * a + pbg[c] as u32 * ia) / 255) as u8;
                    }
                }
            }
        }
        if rows > 0 {
            self.mark_dirty(y, y + rows);
        }
    }
}

impl Display for Framebuffer {
    #[inline]
    fn put_pixel(&mut self, x: usize, y: usize, color: u32) {
        if x < self.info.width && y < self.info.height {
            let px = self.pack(color);
            let off = y * self.pitch + x * self.bpp;
            let n = self.bpp.min(4);
            if off + n <= self.back.len() {
                self.back[off..off + n].copy_from_slice(&px[..n]);
                self.mark_dirty(y, y + 1);
            }
        }
    }

    fn fill_rect(&mut self, x: usize, y: usize, w: usize, h: usize, color: u32) {
        if x >= self.info.width || y >= self.info.height || w == 0 || h == 0 {
            return;
        }
        let w = w.min(self.info.width - x);
        let h = h.min(self.info.height - y);
        let px = self.pack(color);
        // paint the first row, then copy it down
        self.fill_span(x, y, w, px);
        let first = y * self.pitch + x * self.bpp;
        let len = w * self.bpp;
        for row in 1..h {
            let dst = (y + row) * self.pitch + x * self.bpp;
            if dst + len > self.back.len() {
                break;
            }
            self.back.copy_within(first..first + len, dst);
        }
        self.mark_dirty(y, y + h);
    }

    fn clear(&mut self) {
        self.clear_to(0);
    }

    fn width(&self) -> usize {
        self.info.width
    }

    fn height(&self) -> usize {
        self.info.height
    }

    fn present(&mut self) {
        self.flush();
    }
}

impl Framebuffer {
    /// Draws an anti-aliased glyph stretched to fill a `w` x `h` box, using
    /// bilinear interpolation of its coverage.
    pub fn draw_alpha_scaled(&mut self, x: usize, y: usize, w: usize, h: usize, alpha: &[&[u8]], fg: u32, bg: u32) {
        let sh = alpha.len();
        let sw = alpha.first().map_or(0, |row| row.len());
        if sh == 0 || sw == 0 || w == 0 || h == 0 || x >= self.info.width || y >= self.info.height {
            return;
        }
        let (pfg, pbg) = (self.pack(fg), self.pack(bg));
        let bpp = self.bpp.min(4);
        let w = w.min(self.info.width - x);
        let rows = h.min(self.info.height - y);
        let max_x = ((sw - 1) * 256) as i32;
        let max_y = ((sh - 1) * 256) as i32;
        for dy in 0..rows {
            // source row in 24.8 fixed point, sampled at the pixel centre
            let fy = ((((2 * dy + 1) * sh * 128) / h) as i32 - 128).clamp(0, max_y);
            let (y0, ty) = ((fy >> 8) as usize, (fy & 255) as u32);
            let y1 = (y0 + 1).min(sh - 1);
            let off = (y + dy) * self.pitch + x * self.bpp;
            if off + w * self.bpp > self.back.len() {
                break;
            }
            for dx in 0..w {
                let fx = ((((2 * dx + 1) * sw * 128) / w.max(1)) as i32 - 128).clamp(0, max_x);
                let (x0, tx) = ((fx >> 8) as usize, (fx & 255) as u32);
                let x1 = (x0 + 1).min(sw - 1);
                let top = alpha[y0][x0] as u32 * (256 - tx) + alpha[y0][x1] as u32 * tx;
                let bottom = alpha[y1][x0] as u32 * (256 - tx) + alpha[y1][x1] as u32 * tx;
                let a = (top * (256 - ty) + bottom * ty) >> 16;
                let ia = 255 - a;
                let d = &mut self.back[off + dx * self.bpp..off + dx * self.bpp + bpp];
                for c in 0..bpp {
                    d[c] = ((pfg[c] as u32 * a + pbg[c] as u32 * ia) / 255) as u8;
                }
            }
        }
        self.mark_dirty(y, y + rows);
    }
}
