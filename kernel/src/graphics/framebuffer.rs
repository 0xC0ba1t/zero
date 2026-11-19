//! The screen.
//!
//! Everything is drawn into a shadow buffer in ordinary RAM and copied to the
//! real framebuffer by [`Framebuffer::flush`]. Reading and writing video
//! memory pixel by pixel is by far the slowest thing the kernel could do, so
//! drawing code never touches it directly; only the rows that changed since
//! the last flush are copied.
//!
//! The shadow buffer has its own size, the *logical resolution*. When it is
//! smaller than the physical screen, `flush` scales it up (nearest neighbour,
//! aspect ratio kept, centred, black bars) so the resolution can be changed
//! at any time without help from the firmware.

use crate::graphics::display::Display;
use alloc::vec;
use alloc::vec::Vec;
use bootloader_api::info::{FrameBufferInfo, PixelFormat};
use spin::Mutex;

pub static FRAMEBUFFER: Mutex<Option<Framebuffer>> = Mutex::new(None);

/// Runs `f` on the framebuffer, if there is one.
///
/// A thread that owns a window (see `sched::Context`) gets that window's
/// surface instead of the screen, so code written for the screen (the
/// console, the games) draws into its window without knowing.
pub fn with<R>(f: impl FnOnce(&mut Framebuffer) -> R) -> Option<R> {
    if let Some(ctx) = crate::sched::context() {
        let mut surface = ctx.surface.lock();
        return Some(f(&mut surface));
    }
    FRAMEBUFFER.lock().as_mut().map(f)
}

/// Runs `f` on the real screen, whatever thread this is.
pub fn with_screen<R>(f: impl FnOnce(&mut Framebuffer) -> R) -> Option<R> {
    FRAMEBUFFER.lock().as_mut().map(f)
}

pub struct Framebuffer {
    /// The physical screen.
    pub info: FrameBufferInfo,
    front: &'static mut [u8],
    back: Vec<u8>,
    /// Bytes per pixel.
    bpp: usize,
    /// Logical size: what all drawing code sees.
    width: usize,
    height: usize,
    /// Bytes per scan line of the shadow buffer.
    pitch: usize,
    /// Bytes per scan line of the real screen.
    front_pitch: usize,
    /// Logical to physical scale, 16.16 fixed point.
    scale: u64,
    /// Size and position of the scaled picture on the physical screen.
    dest_w: usize,
    dest_h: usize,
    off_x: usize,
    off_y: usize,
    /// For each physical column of the picture, the logical column it shows.
    x_map: Vec<u32>,
    /// Drawing is limited to this rectangle: (x0, y0, x1, y1).
    clip: (usize, usize, usize, usize),
    /// Rows `dirty_lo..dirty_hi` differ between the shadow buffer and the screen.
    dirty_lo: usize,
    dirty_hi: usize,
    /// Counts changes, so the desktop can tell which windows need repainting.
    version: u64,
}

impl Framebuffer {
    pub fn new(info: FrameBufferInfo, buffer: &'static mut [u8]) -> Self {
        let bpp = info.bytes_per_pixel.max(1);
        let mut fb = Framebuffer {
            info,
            front: buffer,
            back: Vec::new(),
            bpp,
            width: 0,
            height: 0,
            pitch: 0,
            front_pitch: info.stride * bpp,
            scale: 65536,
            dest_w: 0,
            dest_h: 0,
            off_x: 0,
            off_y: 0,
            x_map: Vec::new(),
            clip: (0, 0, 0, 0),
            dirty_lo: 0,
            dirty_hi: 0,
            version: 0,
        };
        fb.configure(info.width, info.height);
        fb
    }

    /// Sets up the shadow buffer and the scaling for a logical size.
    fn configure(&mut self, w: usize, h: usize) {
        let (pw, ph) = (self.info.width, self.info.height);
        self.width = w;
        self.height = h;
        self.clip = (0, 0, w, h);
        self.pitch = w * self.bpp;
        self.back = vec![0u8; self.pitch * h];
        if w == pw && h == ph {
            self.scale = 65536;
            self.dest_w = w;
            self.dest_h = h;
            self.off_x = 0;
            self.off_y = 0;
            self.x_map = Vec::new();
        } else {
            let sx = pw as u64 * 65536 / w as u64;
            let sy = ph as u64 * 65536 / h as u64;
            self.scale = sx.min(sy);
            self.dest_w = ((w as u64 * self.scale) >> 16).min(pw as u64) as usize;
            self.dest_h = ((h as u64 * self.scale) >> 16).min(ph as u64) as usize;
            self.off_x = (pw - self.dest_w) / 2;
            self.off_y = (ph - self.dest_h) / 2;
            let scale = self.scale;
            self.x_map = (0..self.dest_w)
                .map(|dx| (((dx as u64) << 16) / scale).min(w as u64 - 1) as u32)
                .collect();
        }
        // black bars around the picture
        self.front.fill(0);
        self.dirty_lo = 0;
        self.dirty_hi = 0;
        self.mark_dirty(0, h);
    }

    /// Changes the logical resolution. It cannot be bigger than the physical
    /// screen. Returns `false` (changing nothing) for an unusable size.
    pub fn set_logical(&mut self, w: usize, h: usize) -> bool {
        if w < 320 || h < 200 || w > self.info.width || h > self.info.height {
            return false;
        }
        self.configure(w, h);
        self.flush();
        true
    }

    /// The physical screen size.
    pub fn physical(&self) -> (usize, usize) {
        (self.info.width, self.info.height)
    }

    /// How much the logical picture is magnified, in percent.
    pub fn scale_percent(&self) -> usize {
        ((self.scale * 100) >> 16) as usize
    }

    #[inline]
    fn mark_dirty(&mut self, lo: usize, hi: usize) {
        self.version = self.version.wrapping_add(1);
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
        let lo = self.dirty_lo;
        let hi = self.dirty_hi.min(self.height);
        self.dirty_lo = 0;
        self.dirty_hi = 0;
        let bpp = self.bpp;

        if self.x_map.is_empty() {
            // same size: one copy per row (the screen's rows may be padded)
            for y in lo..hi {
                let src = y * self.pitch;
                let dst = y * self.front_pitch;
                if dst + self.pitch <= self.front.len() {
                    self.front[dst..dst + self.pitch].copy_from_slice(&self.back[src..src + self.pitch]);
                }
            }
            return;
        }

        // scaled: every physical row picks the logical row it shows
        let scale = self.scale;
        let dy_lo = ((lo as u64 * scale) >> 16) as usize;
        let dy_hi = (((hi as u64 * scale) + 65535) >> 16).min(self.dest_h as u64) as usize;
        let row_bytes = self.dest_w * bpp;
        let mut last_sy = usize::MAX;
        let mut prev_dst = 0;
        for dy in dy_lo..dy_hi {
            let sy = ((((dy as u64) << 16) / scale) as usize).min(self.height - 1);
            let dst = (dy + self.off_y) * self.front_pitch + self.off_x * bpp;
            if dst + row_bytes > self.front.len() {
                break;
            }
            if sy == last_sy {
                // same source row as the row above: copy that row
                self.front.copy_within(prev_dst..prev_dst + row_bytes, dst);
            } else {
                let src_row = &self.back[sy * self.pitch..(sy + 1) * self.pitch];
                let out = &mut self.front[dst..dst + row_bytes];
                match bpp {
                    4 => {
                        for (o, &mx) in out.chunks_exact_mut(4).zip(self.x_map.iter()) {
                            let s = mx as usize * 4;
                            o.copy_from_slice(&src_row[s..s + 4]);
                        }
                    }
                    3 => {
                        for (o, &mx) in out.chunks_exact_mut(3).zip(self.x_map.iter()) {
                            let s = mx as usize * 3;
                            o.copy_from_slice(&src_row[s..s + 3]);
                        }
                    }
                    n => {
                        for (o, &mx) in out.chunks_exact_mut(n).zip(self.x_map.iter()) {
                            let s = mx as usize * n;
                            o.copy_from_slice(&src_row[s..s + n]);
                        }
                    }
                }
                last_sy = sy;
            }
            prev_dst = dst;
        }
    }

    /// Moves the picture up by `dy` rows and fills the rows that open up at
    /// the bottom with `fill`.
    pub fn scroll_up(&mut self, dy: usize, fill: u32) {
        let h = self.height;
        self.scroll_rect_up(0, h, dy, fill);
    }

    /// Limits all drawing to a rectangle (until [`Framebuffer::clear_clip`]).
    pub fn set_clip(&mut self, x: usize, y: usize, w: usize, h: usize) {
        self.clip = (x.min(self.width), y.min(self.height), (x + w).min(self.width), (y + h).min(self.height));
    }

    /// Lets drawing reach the whole screen again.
    pub fn clear_clip(&mut self) {
        self.clip = (0, 0, self.width, self.height);
    }

    /// Scrolls the full-width band of `h` rows starting at row `y` down by `dy` rows,
    /// filling the rows that open up at the top with `fill`.
    pub fn scroll_rect_down(&mut self, y: usize, h: usize, dy: usize, fill: u32) {
        if dy == 0 || y >= self.height {
            return;
        }
        let h = h.min(self.height - y);
        let w = self.width;
        if dy >= h {
            self.fill_rect(0, y, w, h, fill);
            return;
        }
        self.back.copy_within(y * self.pitch..(y + h - dy) * self.pitch, (y + dy) * self.pitch);
        self.fill_rect(0, y, w, dy, fill);
        self.mark_dirty(y, y + h);
    }

    /// Scrolls the full-width band of `h` rows starting at row `y` up by `dy` rows.
    pub fn scroll_rect_up(&mut self, y: usize, h: usize, dy: usize, fill: u32) {
        if dy == 0 || y >= self.height {
            return;
        }
        let h = h.min(self.height - y);
        if dy >= h {
            let w = self.width;
            self.fill_rect(0, y, w, h, fill);
            return;
        }
        self.back.copy_within((y + dy) * self.pitch..(y + h) * self.pitch, y * self.pitch);
        let w = self.width;
        self.fill_rect(0, y + h - dy, w, dy, fill);
        self.mark_dirty(y, y + h);
    }

    pub fn clear_to(&mut self, color: u32) {
        let (w, h) = (self.width, self.height);
        self.fill_rect(0, 0, w, h, color);
    }

    /// A copy of the picture as it is now (see [`Framebuffer::restore`]).
    pub fn snapshot(&self) -> Vec<u8> {
        self.back.clone()
    }

    /// Puts back a picture taken with [`Framebuffer::snapshot`].
    pub fn restore(&mut self, snapshot: &[u8]) {
        if snapshot.len() == self.back.len() {
            self.back.copy_from_slice(snapshot);
            let h = self.height;
            self.mark_dirty(0, h);
        }
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

    /// Mixes `color` into the pixel at `(x, y)`; `alpha` 255 replaces it, 0 leaves it.
    #[inline]
    pub fn blend_pixel(&mut self, x: usize, y: usize, color: u32, alpha: u32) {
        if x >= self.width || y >= self.height || alpha == 0 {
            return;
        }
        let p = self.pack(color);
        let n = self.bpp.min(4);
        let off = y * self.pitch + x * self.bpp;
        if off + n > self.back.len() {
            return;
        }
        let d = &mut self.back[off..off + n];
        if alpha >= 255 {
            store(d, &p, n);
        } else {
            let ia = 255 - alpha;
            for c in 0..n {
                d[c] = ((p[c] as u32 * alpha + d[c] as u32 * ia) / 255) as u8;
            }
        }
        self.mark_dirty(y, y + 1);
    }

    /// Draws `color` wherever `mask` (a `w` x `h` array, row by row) is non-zero.
    /// The shape may hang over the edges of the screen.
    pub fn blit_mask(&mut self, x: i32, y: i32, w: usize, h: usize, mask: &[u8], color: u32) {
        let p = self.pack(color);
        let n = self.bpp.min(4);
        let (sw, sh) = (self.width as i32, self.height as i32);
        let mut touched = false;
        for ry in 0..h {
            let yy = y + ry as i32;
            if yy < 0 || yy >= sh {
                continue;
            }
            let row = &mask[ry * w..(ry + 1) * w];
            for (rx, &m) in row.iter().enumerate() {
                let xx = x + rx as i32;
                if m != 0 && xx >= 0 && xx < sw {
                    let off = yy as usize * self.pitch + xx as usize * self.bpp;
                    store(&mut self.back[off..off + n], &p, n);
                }
            }
            touched = true;
        }
        if touched {
            let lo = y.max(0) as usize;
            let hi = ((y + h as i32).min(sh)).max(0) as usize;
            self.mark_dirty(lo, hi);
        }
    }

    /// Draws one anti-aliased glyph: `alpha` rows give the coverage of the
    /// foreground color over the background color for each pixel.
    pub fn draw_alpha(&mut self, x: usize, y: usize, alpha: &[&[u8]], fg: u32, bg: u32) {
        let (w, h) = (self.width, self.height);
        let pfg = self.pack(fg);
        let pbg = self.pack(bg);
        let bpp = self.bpp;
        let (cx0, cy0, cx1, cy1) = self.clip;
        let (mut lo, mut hi) = (usize::MAX, 0);
        for (dy, row) in alpha.iter().enumerate() {
            let yy = y + dy;
            if yy >= h.min(cy1) {
                break;
            }
            if yy < cy0 || x >= cx1 || x < cx0 {
                continue;
            }
            lo = lo.min(yy);
            hi = hi.max(yy + 1);
            let n = row.len().min(w.min(cx1).saturating_sub(x));
            let off = yy * self.pitch + x * bpp;
            if off + n * bpp > self.back.len() {
                break;
            }
            let dst = &mut self.back[off..off + n * bpp];
            for (a, d) in row[..n].iter().zip(dst.chunks_exact_mut(bpp)) {
                let a = *a as u32;
                if a == 0 {
                    store(d, &pbg, bpp.min(4));
                } else if a == 255 {
                    store(d, &pfg, bpp.min(4));
                } else {
                    let ia = 255 - a;
                    for c in 0..bpp.min(4) {
                        d[c] = ((pfg[c] as u32 * a + pbg[c] as u32 * ia) / 255) as u8;
                    }
                }
            }
        }
        if lo < hi {
            self.mark_dirty(lo, hi);
        }
    }

    /// Draws an anti-aliased glyph stretched to a `w` x `h` box, using
    /// bilinear interpolation of its coverage.
    pub fn draw_alpha_scaled(&mut self, x: usize, y: usize, w: usize, h: usize, alpha: &[&[u8]], fg: u32, bg: u32) {
        let sh = alpha.len();
        let sw = alpha.first().map_or(0, |row| row.len());
        if sh == 0 || sw == 0 || w == 0 || h == 0 || x >= self.width || y >= self.height {
            return;
        }
        let (pfg, pbg) = (self.pack(fg), self.pack(bg));
        let bpp = self.bpp.min(4);
        let wc = w.min(self.width - x);
        let rows = h.min(self.height - y);
        let max_x = ((sw - 1) * 256) as i32;
        let max_y = ((sh - 1) * 256) as i32;
        for dy in 0..rows {
            // source row in 24.8 fixed point, sampled at the pixel centre
            let fy = ((((2 * dy + 1) * sh * 128) / h) as i32 - 128).clamp(0, max_y);
            let (y0, ty) = ((fy >> 8) as usize, (fy & 255) as u32);
            let y1 = (y0 + 1).min(sh - 1);
            let off = (y + dy) * self.pitch + x * self.bpp;
            if off + wc * self.bpp > self.back.len() {
                break;
            }
            for dx in 0..wc {
                let fx = ((((2 * dx + 1) * sw * 128) / w) as i32 - 128).clamp(0, max_x);
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

impl Display for Framebuffer {
    #[inline]
    fn put_pixel(&mut self, x: usize, y: usize, color: u32) {
        if x < self.width && y < self.height {
            let px = self.pack(color);
            let off = y * self.pitch + x * self.bpp;
            let n = self.bpp.min(4);
            if off + n <= self.back.len() {
                store(&mut self.back[off..off + n], &px, n);
                self.mark_dirty(y, y + 1);
            }
        }
    }

    fn fill_rect(&mut self, x: usize, y: usize, w: usize, h: usize, color: u32) {
        let (cx0, cy0, cx1, cy1) = self.clip;
        let (x0, y0) = (x.max(cx0), y.max(cy0));
        let (x1, y1) = (x.saturating_add(w).min(cx1), y.saturating_add(h).min(cy1));
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let (x, y, w, h) = (x0, y0, x1 - x0, y1 - y0);
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
        self.width
    }

    fn height(&self) -> usize {
        self.height
    }

    fn present(&mut self) {
        self.flush();
    }
}

/// Writes the first `n` bytes of a packed pixel. Plain byte stores: a
/// `copy_from_slice` with a length only known at run time becomes a call to
/// `memcpy`, which is far too slow to do once per pixel.
#[inline(always)]
fn store(d: &mut [u8], p: &[u8; 4], n: usize) {
    match n {
        4 => {
            d[0] = p[0];
            d[1] = p[1];
            d[2] = p[2];
            d[3] = p[3];
        }
        3 => {
            d[0] = p[0];
            d[1] = p[1];
            d[2] = p[2];
        }
        _ => {
            for i in 0..n {
                d[i] = p[i];
            }
        }
    }
}

impl Framebuffer {
    /// Renders a glyph over its background into ready-to-copy pixel bytes: a
    /// `w` x `h` tile with the glyph's first row at row `top`.
    pub fn make_tile(&self, w: usize, h: usize, top: usize, alpha: &[&[u8]], fg: u32, bg: u32) -> Vec<u8> {
        let bpp = self.bpp;
        let (pfg, pbg) = (self.pack(fg), self.pack(bg));
        let n = bpp.min(4);
        let mut tile = vec![0u8; w * h * bpp];
        for px in tile.chunks_exact_mut(bpp) {
            store(px, &pbg, n);
        }
        for (dy, row) in alpha.iter().enumerate() {
            let yy = top + dy;
            if yy >= h {
                break;
            }
            for (dx, &a) in row.iter().enumerate().take(w) {
                let a = a as u32;
                if a == 0 {
                    continue;
                }
                let d = &mut tile[(yy * w + dx) * bpp..(yy * w + dx) * bpp + n];
                if a == 255 {
                    store(d, &pfg, n);
                } else {
                    let ia = 255 - a;
                    for c in 0..n {
                        d[c] = ((pfg[c] as u32 * a + pbg[c] as u32 * ia) / 255) as u8;
                    }
                }
            }
        }
        tile
    }

    /// Copies a tile made by [`Framebuffer::make_tile`] to `(x, y)`, one row at a time.
    pub fn blit_tile(&mut self, x: usize, y: usize, w: usize, h: usize, tile: &[u8]) {
        let bpp = self.bpp;
        let (cx0, cy0, cx1, cy1) = self.clip;
        let x0 = x.max(cx0);
        let x1 = (x + w).min(cx1);
        if x0 >= x1 {
            return;
        }
        let (mut lo, mut hi) = (usize::MAX, 0);
        for ry in 0..h {
            let yy = y + ry;
            if yy < cy0 || yy >= cy1 {
                continue;
            }
            let src = (ry * w + (x0 - x)) * bpp;
            let len = (x1 - x0) * bpp;
            let off = yy * self.pitch + x0 * bpp;
            if off + len > self.back.len() || src + len > tile.len() {
                continue;
            }
            self.back[off..off + len].copy_from_slice(&tile[src..src + len]);
            lo = lo.min(yy);
            hi = hi.max(yy + 1);
        }
        if lo < hi {
            self.mark_dirty(lo, hi);
        }
    }
}

impl Framebuffer {
    /// Draws a `sw` x `sh` picture of 0x00RRGGBB pixels stretched into the
    /// `dw` x `dh` rectangle at `(dx, dy)`. `columns` gives, for each of the
    /// `dw` destination columns, the source column to take.
    pub fn draw_pixels_scaled(&mut self, src: &[u32], sw: usize, sh: usize, dx: usize, dy: usize, dw: usize, dh: usize, columns: &[u32]) {
        if src.len() < sw * sh || columns.len() < dw || dh == 0 {
            return;
        }
        let bpp = self.bpp;
        let rgb = self.info.pixel_format == PixelFormat::Rgb;
        let (_, cy0, cx1, cy1) = self.clip;
        let mut lo = usize::MAX;
        let mut hi = 0;
        for row in 0..dh {
            let y = dy + row;
            if y < cy0 || y >= cy1 {
                continue;
            }
            let source = &src[(row * sh / dh) * sw..][..sw];
            let off = y * self.pitch + dx * bpp;
            let width = dw.min(cx1.saturating_sub(dx));
            if off + width * bpp > self.back.len() {
                break;
            }
            let dst = &mut self.back[off..off + width * bpp];
            if bpp == 4 && !rgb {
                for (d, &c) in dst.chunks_exact_mut(4).zip(columns.iter()) {
                    let v = source[c as usize] & 0x00FF_FFFF;
                    d[0] = v as u8;
                    d[1] = (v >> 8) as u8;
                    d[2] = (v >> 16) as u8;
                    d[3] = 0;
                }
            } else {
                for (d, &c) in dst.chunks_exact_mut(bpp).zip(columns.iter()) {
                    let v = source[c as usize];
                    let (r, g, b) = ((v >> 16) as u8, (v >> 8) as u8, v as u8);
                    if rgb {
                        d[0] = r;
                        d[1] = g;
                        d[2] = b;
                    } else {
                        d[0] = b;
                        d[1] = g;
                        d[2] = r;
                    }
                }
            }
            lo = lo.min(y);
            hi = hi.max(y + 1);
        }
        if lo < hi {
            self.mark_dirty(lo, hi);
        }
    }
}

// ---------------------------------------------------------------- desktop support

/// A rectangle on the screen: `(x0, y0, x1, y1)`, the end excluded.
pub type Rect = (i32, i32, i32, i32);

impl Framebuffer {
    /// A surface that is not on the screen: a window's pixels. It has the
    /// same pixel format as `like`, so the desktop can copy it row by row.
    pub fn offscreen(like: &Framebuffer, w: usize, h: usize) -> Framebuffer {
        let mut info = like.info;
        info.width = w;
        info.height = h;
        info.stride = w;
        info.byte_len = w * h * like.bpp;
        Framebuffer::new(info, &mut [])
    }

    /// Limits drawing to a screen rectangle.
    pub fn set_clip_rect(&mut self, r: Rect) {
        self.clip = (r.0.max(0) as usize, r.1.max(0) as usize, (r.2.max(0) as usize).min(self.width), (r.3.max(0) as usize).min(self.height));
    }

    /// The current drawing limit: (x0, y0, x1, y1).
    pub fn clip_bounds(&self) -> (usize, usize, usize, usize) {
        self.clip
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn bytes_per_pixel(&self) -> usize {
        self.bpp
    }

    pub fn pitch(&self) -> usize {
        self.pitch
    }

    pub fn pixels(&self) -> &[u8] {
        &self.back
    }

    pub fn pixels_mut(&mut self) -> &mut [u8] {
        &mut self.back
    }

    /// Marks rows `lo..hi` as changed (they will be copied to the screen by `flush`).
    pub fn damage_rows(&mut self, lo: usize, hi: usize) {
        self.mark_dirty(lo.min(self.height), hi.min(self.height));
    }

    /// Copies a `w`x`h` piece of another pixel buffer (same format, `src_pitch`
    /// bytes per row) to `(dx, dy)`, limited to `clip` and the screen.
    pub fn blit_bytes(&mut self, src: &[u8], src_pitch: usize, sx: usize, sy: usize, w: usize, h: usize, dx: i32, dy: i32, clip: Rect) {
        let bpp = self.bpp;
        let x0 = dx.max(clip.0).max(0);
        let y0 = dy.max(clip.1).max(0);
        let x1 = (dx + w as i32).min(clip.2).min(self.width as i32);
        let y1 = (dy + h as i32).min(clip.3).min(self.height as i32);
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let n = (x1 - x0) as usize * bpp;
        for y in y0..y1 {
            let s = (sy + (y - dy) as usize) * src_pitch + (sx + (x0 - dx) as usize) * bpp;
            let d = y as usize * self.pitch + x0 as usize * bpp;
            if s + n > src.len() || d + n > self.back.len() {
                continue;
            }
            self.back[d..d + n].copy_from_slice(&src[s..s + n]);
        }
        self.mark_dirty(y0 as usize, y1 as usize);
    }

    /// Like `blit_bytes`, but the rectangle has rounded corners of radius `r`
    /// (the corners are left untouched).
    pub fn blit_rounded(&mut self, src: &[u8], src_pitch: usize, w: usize, h: usize, dx: i32, dy: i32, r: usize, round_top: bool, clip: Rect) {
        let r = r.min(w / 2).min(h / 2);
        let bpp = self.bpp;
        for row in 0..h {
            let inset = if round_top && row < r {
                corner_inset(r, r - row)
            } else if row + r >= h {
                corner_inset(r, row + r - h + 1)
            } else {
                0
            };
            let y = dy + row as i32;
            if y < clip.1 || y >= clip.3 || y < 0 || y >= self.height as i32 {
                continue;
            }
            let x0 = (dx + inset as i32).max(clip.0).max(0);
            let x1 = (dx + (w - inset) as i32).min(clip.2).min(self.width as i32);
            if x1 <= x0 {
                continue;
            }
            let n = (x1 - x0) as usize * bpp;
            let s = row * src_pitch + (x0 - dx) as usize * bpp;
            let d = y as usize * self.pitch + x0 as usize * bpp;
            if s + n > src.len() || d + n > self.back.len() {
                continue;
            }
            self.back[d..d + n].copy_from_slice(&src[s..s + n]);
        }
        let lo = dy.max(clip.1).max(0) as usize;
        let hi = (dy + h as i32).min(clip.3).min(self.height as i32).max(0) as usize;
        if hi > lo {
            self.mark_dirty(lo, hi);
        }
    }

    /// Blends a flat color over a rectangle (`alpha` 0..=255), limited to `clip`.
    pub fn tint_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: u32, alpha: u32, clip: Rect) {
        let bpp = self.bpp;
        let x0 = x.max(clip.0).max(0);
        let y0 = y.max(clip.1).max(0);
        let x1 = (x + w).min(clip.2).min(self.width as i32);
        let y1 = (y + h).min(clip.3).min(self.height as i32);
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let c = self.pack(color);
        let a = alpha.min(255);
        let rgb = self.info.pixel_format != PixelFormat::U8;
        let n = if rgb { bpp.min(3) } else { 1 };
        if a >= 255 {
            // opaque: plain stores, one row built then copied
            let rows = (y1 - y0) as usize;
            let width = (x1 - x0) as usize;
            for yy in 0..rows {
                let base = (y0 as usize + yy) * self.pitch + x0 as usize * bpp;
                if base + width * bpp > self.back.len() {
                    break;
                }
                for px in self.back[base..base + width * bpp].chunks_exact_mut(bpp) {
                    px[..n].copy_from_slice(&c[..n]);
                }
            }
            self.mark_dirty(y0 as usize, y1 as usize);
            return;
        }
        // translucent: a lookup table per channel makes each pixel three loads
        let ia = 255 - a;
        let mut lut = [[0u8; 256]; 3];
        for k in 0..n {
            let add = c[k] as u32 * a + 127;
            for (v, slot) in lut[k].iter_mut().enumerate() {
                *slot = ((v as u32 * ia + add) / 255) as u8;
            }
        }
        for yy in y0..y1 {
            let base = yy as usize * self.pitch + x0 as usize * bpp;
            let width = (x1 - x0) as usize;
            if base + width * bpp > self.back.len() {
                break;
            }
            for px in self.back[base..base + width * bpp].chunks_exact_mut(bpp) {
                for k in 0..n {
                    px[k] = lut[k][px[k] as usize];
                }
            }
        }
        self.mark_dirty(y0 as usize, y1 as usize);
    }

    /// Fills a rectangle with rounded corners with a flat color.
    pub fn fill_rounded(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, color: u32, clip: Rect) {
        let r = r.min(w / 2).min(h / 2).max(0);
        let c = self.pack(color);
        let bpp = self.bpp;
        for row in 0..h {
            let inset = if row < r {
                corner_inset(r as usize, (r - row) as usize) as i32
            } else if row + r >= h {
                corner_inset(r as usize, (row + r - h + 1) as usize) as i32
            } else {
                0
            };
            let yy = y + row;
            if yy < clip.1 || yy >= clip.3 || yy < 0 || yy >= self.height as i32 {
                continue;
            }
            let x0 = (x + inset).max(clip.0).max(0);
            let x1 = (x + w - inset).min(clip.2).min(self.width as i32);
            for xx in x0..x1 {
                let o = yy as usize * self.pitch + xx as usize * bpp;
                if o + bpp <= self.back.len() {
                    self.back[o..o + bpp].copy_from_slice(&c[..bpp]);
                }
            }
        }
        let lo = y.max(clip.1).max(0) as usize;
        let hi = (y + h).min(clip.3).min(self.height as i32).max(0) as usize;
        if hi > lo {
            self.mark_dirty(lo, hi);
        }
    }
}

/// How many pixels a row `d` rows from the edge of a corner of radius `r` is cut.
fn corner_inset(r: usize, d: usize) -> usize {
    // the circle's edge: r - sqrt(r^2 - (r-d)^2)... with d counted from the top as r..1
    let dy = d as i64; // distance from the arc's centre row: 1..=r
    let r = r as i64;
    let rr = r * r;
    let mut x = r;
    while x > 0 && x * x + (dy - 1) * (dy - 1) > rr {
        x -= 1;
    }
    (r - x) as usize
}
