//! The desktop's drawing layer: a canvas over 32-bit pixels (0x00RRGGBB)
//! with anti-aliased rounded rectangles, circles, lines, soft shadows,
//! blur, scaled images and proportionally spaced text.

use crate::graphics::font::Font;
use alloc::vec;
use alloc::vec::Vec;
use noto_sans_mono_bitmap::RasterHeight;
use spin::Mutex;

pub type Rect = (i32, i32, i32, i32); // x, y, w, h

#[inline]
pub fn blend(d: u32, s: u32, a: u32) -> u32 {
    if a >= 255 {
        return s;
    }
    if a == 0 {
        return d;
    }
    let a1 = a + 1;
    let ia = 256 - a1;
    let rb = (((s & 0xFF00FF) * a1 + (d & 0xFF00FF) * ia) >> 8) & 0xFF00FF;
    let g = (((s & 0x00FF00) * a1 + (d & 0x00FF00) * ia) >> 8) & 0x00FF00;
    rb | g
}

pub fn mix(a: u32, b: u32, t: u32) -> u32 {
    blend(a, b, t.min(255))
}

pub fn rgb(r: u32, g: u32, b: u32) -> u32 {
    (r.min(255) << 16) | (g.min(255) << 8) | b.min(255)
}

pub fn lighten(c: u32, amount: u32) -> u32 {
    blend(c, 0xFFFFFF, amount)
}

pub fn darken(c: u32, amount: u32) -> u32 {
    blend(c, 0x000000, amount)
}

/// Brightness 0..255 of a color.
pub fn luma(c: u32) -> u32 {
    (((c >> 16) & 255) * 3 + ((c >> 8) & 255) * 6 + (c & 255)) / 10
}

pub fn isqrt(n: i64) -> i64 {
    if n <= 0 {
        return 0;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

// ---- corner coverage tables ----------------------------------------------------

/// Coverage (0..255) of each pixel of a quarter circle of radius r, top-left
/// quadrant, row-major r*r; cached per radius.
static CORNERS: Mutex<Vec<Option<&'static [u8]>>> = Mutex::new(Vec::new());

pub fn corner_table(r: i32) -> &'static [u8] {
    let r = r.clamp(0, 96) as usize;
    let mut cache = CORNERS.lock();
    if cache.len() <= r {
        cache.resize(r + 1, None);
    }
    if let Some(t) = cache[r] {
        return t;
    }
    let mut t = vec![0u8; r * r];
    let rr = (r * 8) as i64;
    for y in 0..r {
        for x in 0..r {
            let mut n = 0;
            for sy in 0..4 {
                for sx in 0..4 {
                    // sample point in 1/8 units, relative to the circle centre at (r, r)
                    let px = (x * 8 + sx * 2 + 1) as i64 - rr;
                    let py = (y * 8 + sy * 2 + 1) as i64 - rr;
                    if px * px + py * py <= rr * rr {
                        n += 1;
                    }
                }
            }
            t[y * r + x] = (n * 255 / 16) as u8;
        }
    }
    let t: &'static [u8] = alloc::vec::Vec::leak(t);
    cache[r] = Some(t);
    t
}

// ---- proportional text metrics ---------------------------------------------------

/// (left trim, advance) per glyph for each font; computed on first use.
static METRICS: Mutex<Vec<(u8, u8, Vec<(u8, u8)>)>> = Mutex::new(Vec::new());

fn font_key(f: &Font) -> (u8, u8) {
    (f.height() as u8, if f.is_bold() { 1 } else { 0 })
}

fn glyph_metrics(f: &Font, c: char) -> (i32, i32) {
    let full = f.width() as i32;
    if c == ' ' {
        return (0, full * 5 / 9);
    }
    let code = c as u32;
    if !(33..127).contains(&code) {
        return (0, full);
    }
    let key = font_key(f);
    let mut m = METRICS.lock();
    let idx = match m.iter().position(|(h, b, _)| (*h, *b) == key) {
        Some(i) => i,
        None => {
            let mut table = Vec::with_capacity(94);
            for ch in 33u8..127 {
                let raster = f.raster(ch as char);
                let mut lo = usize::MAX;
                let mut hi = 0;
                for row in raster.iter() {
                    for (x, &a) in row.iter().enumerate() {
                        if a > 40 {
                            lo = lo.min(x);
                            hi = hi.max(x + 1);
                        }
                    }
                }
                if lo == usize::MAX {
                    table.push((0, full as u8));
                } else {
                    let spacing = (f.height() / 9).max(1);
                    table.push((lo as u8, (hi - lo + spacing) as u8));
                }
            }
            m.push((key.0, key.1, table));
            m.len() - 1
        }
    };
    let (lo, adv) = m[idx].2[(code - 33) as usize];
    (lo as i32, adv as i32)
}

pub fn text_width(f: &Font, s: &str) -> i32 {
    s.chars().map(|c| glyph_metrics(f, c).1).sum()
}

pub fn font(px: i32, bold: bool) -> Font {
    let size = if px >= 30 {
        RasterHeight::Size32
    } else if px >= 22 {
        RasterHeight::Size24
    } else if px >= 18 {
        RasterHeight::Size20
    } else {
        RasterHeight::Size16
    };
    if bold { Font::bold(size) } else { Font::regular(size) }
}

/// Cuts text to fit `max_w` pixels, adding an ellipsis.
pub fn ellipsize(f: &Font, s: &str, max_w: i32) -> alloc::string::String {
    if text_width(f, s) <= max_w {
        return alloc::string::String::from(s);
    }
    let dots = text_width(f, "...");
    let mut out = alloc::string::String::new();
    let mut w = 0;
    for c in s.chars() {
        let a = glyph_metrics(f, c).1;
        if w + a + dots > max_w {
            break;
        }
        w += a;
        out.push(c);
    }
    out.push_str("...");
    out
}

/// Splits text into lines no wider than `max_w`.
pub fn wrap(f: &Font, s: &str, max_w: i32) -> Vec<alloc::string::String> {
    let mut lines = Vec::new();
    for para in s.split('\n') {
        let mut line = alloc::string::String::new();
        for word in para.split(' ') {
            let candidate = if line.is_empty() { alloc::string::String::from(word) } else { alloc::format!("{} {}", line, word) };
            if text_width(f, &candidate) > max_w && !line.is_empty() {
                lines.push(core::mem::take(&mut line));
                line = alloc::string::String::from(word);
            } else {
                line = candidate;
            }
        }
        lines.push(line);
    }
    lines
}

// ---- the canvas -------------------------------------------------------------------

pub struct Canvas<'a> {
    pub px: &'a mut [u32],
    pub stride: usize,
    pub w: i32,
    pub h: i32,
    clip: (i32, i32, i32, i32), // x0 y0 x1 y1, absolute
    ox: i32,
    oy: i32,
    stack: Vec<((i32, i32, i32, i32), i32, i32)>,
    /// Multiplies every alpha (for fading whole things).
    pub opacity: u32,
}

impl<'a> Canvas<'a> {
    pub fn new(px: &'a mut [u32], w: usize, h: usize) -> Canvas<'a> {
        Canvas { px, stride: w, w: w as i32, h: h as i32, clip: (0, 0, w as i32, h as i32), ox: 0, oy: 0, stack: Vec::new(), opacity: 255 }
    }

    /// Restricts drawing to a rectangle and moves the origin to its corner.
    pub fn push(&mut self, r: Rect, translate: bool) {
        self.stack.push((self.clip, self.ox, self.oy));
        let (x0, y0) = (r.0 + self.ox, r.1 + self.oy);
        let c = self.clip;
        self.clip = (x0.max(c.0), y0.max(c.1), (x0 + r.2).min(c.2), (y0 + r.3).min(c.3));
        if translate {
            self.ox = x0;
            self.oy = y0;
        }
    }

    pub fn pop(&mut self) {
        if let Some((c, x, y)) = self.stack.pop() {
            self.clip = c;
            self.ox = x;
            self.oy = y;
        }
    }

    pub fn clip_abs(&mut self, r: (i32, i32, i32, i32)) {
        self.clip = (r.0.max(0), r.1.max(0), r.2.min(self.w), r.3.min(self.h));
    }

    /// Moves the origin (for drawing into a band of a bigger picture).
    pub fn shift_origin(&mut self, dx: i32, dy: i32) {
        self.ox += dx;
        self.oy += dy;
    }

    pub fn clip_box(&self) -> (i32, i32, i32, i32) {
        self.clip
    }

    pub fn origin(&self) -> (i32, i32) {
        (self.ox, self.oy)
    }

    #[inline]
    fn a(&self, a: u32) -> u32 {
        if self.opacity >= 255 { a } else { a * self.opacity / 255 }
    }

    #[inline]
    pub fn put(&mut self, x: i32, y: i32, c: u32, a: u32) {
        let (x, y) = (x + self.ox, y + self.oy);
        if x >= self.clip.0 && y >= self.clip.1 && x < self.clip.2 && y < self.clip.3 {
            let i = y as usize * self.stride + x as usize;
            let a = self.a(a);
            self.px[i] = blend(self.px[i], c, a);
        }
    }

    /// A horizontal run of pixels, absolute coordinates, clipped.
    #[inline]
    fn span(&mut self, x0: i32, x1: i32, y: i32, c: u32, a: u32) {
        if y < self.clip.1 || y >= self.clip.3 {
            return;
        }
        let x0 = x0.max(self.clip.0);
        let x1 = x1.min(self.clip.2);
        if x1 <= x0 {
            return;
        }
        let a = self.a(a);
        let row = y as usize * self.stride;
        let s = &mut self.px[row + x0 as usize..row + x1 as usize];
        if a >= 255 {
            s.fill(c);
        } else if a > 0 {
            for p in s.iter_mut() {
                *p = blend(*p, c, a);
            }
        }
    }

    pub fn fill(&mut self, c: u32) {
        let (w, h) = (self.w, self.h);
        let (ox, oy) = (self.ox, self.oy);
        self.rect(-ox, -oy, w, h, c);
    }

    pub fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: u32) {
        self.rect_a(x, y, w, h, c, 255);
    }

    pub fn rect_a(&mut self, x: i32, y: i32, w: i32, h: i32, c: u32, a: u32) {
        let (x, y) = (x + self.ox, y + self.oy);
        for yy in y.max(self.clip.1)..(y + h).min(self.clip.3) {
            self.span(x, x + w, yy, c, a);
        }
    }

    pub fn hline(&mut self, x: i32, y: i32, w: i32, c: u32, a: u32) {
        let (x, y) = (x + self.ox, y + self.oy);
        self.span(x, x + w, y, c, a);
    }

    pub fn vline(&mut self, x: i32, y: i32, h: i32, c: u32, a: u32) {
        self.rect_a(x, y, 1, h, c, a);
    }

    /// An anti-aliased rounded rectangle.
    pub fn round(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, c: u32) {
        self.round_a(x, y, w, h, r, c, 255);
    }

    pub fn round_a(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, c: u32, a: u32) {
        if w <= 0 || h <= 0 {
            return;
        }
        let r = r.min(w / 2).min(h / 2).max(0);
        let (x, y) = (x + self.ox, y + self.oy);
        if r == 0 {
            for yy in y.max(self.clip.1)..(y + h).min(self.clip.3) {
                self.span(x, x + w, yy, c, a);
            }
            return;
        }
        let t = corner_table(r);
        for row in 0..h {
            let yy = y + row;
            if yy < self.clip.1 || yy >= self.clip.3 {
                continue;
            }
            let cy = if row < r { Some(row) } else if row >= h - r { Some(h - 1 - row) } else { None };
            match cy {
                None => self.span(x, x + w, yy, c, a),
                Some(cy) => {
                    let line = &t[cy as usize * r as usize..(cy as usize + 1) * r as usize];
                    // the fully covered middle, then the two corner runs
                    let mut first_full = r;
                    for (i, &cv) in line.iter().enumerate() {
                        if cv == 255 {
                            first_full = i as i32;
                            break;
                        }
                    }
                    self.span(x + first_full, x + w - first_full, yy, c, a);
                    for i in 0..first_full {
                        let cv = line[i as usize] as u32;
                        if cv > 0 {
                            let ca = cv * a / 255;
                            self.put(x + i - self.ox, yy - self.oy, c, ca);
                            self.put(x + w - 1 - i - self.ox, yy - self.oy, c, ca);
                        }
                    }
                }
            }
        }
    }

    /// A rounded rectangle with a one pixel border.
    pub fn round_border(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, fill: u32, border: u32) {
        self.round(x, y, w, h, r, border);
        self.round(x + 1, y + 1, w - 2, h - 2, (r - 1).max(0), fill);
    }

    /// Only the outline of a rounded rectangle, over whatever is there.
    pub fn round_outline(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, c: u32, a: u32) {
        let r = r.min(w / 2).min(h / 2).max(0);
        self.rect_a(x + r, y, w - 2 * r, 1, c, a);
        self.rect_a(x + r, y + h - 1, w - 2 * r, 1, c, a);
        self.rect_a(x, y + r, 1, h - 2 * r, c, a);
        self.rect_a(x + w - 1, y + r, 1, h - 2 * r, c, a);
        if r > 0 {
            let t = corner_table(r);
            let ti = corner_table((r - 1).max(0));
            for cy in 0..r {
                for cx in 0..r {
                    let outer = t[(cy * r + cx) as usize] as i32;
                    let inner = if cy >= 1 && cx >= 1 && r > 1 { ti[((cy - 1) * (r - 1) + (cx - 1)) as usize] as i32 } else { 0 };
                    let cov = (outer - inner).max(0) as u32;
                    if cov > 0 {
                        let ca = cov * a / 255;
                        self.put(x + cx, y + cy, c, ca);
                        self.put(x + w - 1 - cx, y + cy, c, ca);
                        self.put(x + cx, y + h - 1 - cy, c, ca);
                        self.put(x + w - 1 - cx, y + h - 1 - cy, c, ca);
                    }
                }
            }
        }
    }

    pub fn circle(&mut self, cx: i32, cy: i32, r: i32, c: u32) {
        self.round_a(cx - r, cy - r, r * 2, r * 2, r, c, 255);
    }

    pub fn circle_a(&mut self, cx: i32, cy: i32, r: i32, c: u32, a: u32) {
        self.round_a(cx - r, cy - r, r * 2, r * 2, r, c, a);
    }

    /// A vertical gradient inside a rounded rectangle.
    pub fn gradient(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, top: u32, bottom: u32) {
        if h <= 0 {
            return;
        }
        let saved = self.clip;
        for row in 0..h {
            let c = mix(top, bottom, (row * 255 / h.max(1)) as u32);
            let yy = y + row + self.oy;
            self.clip = (saved.0, yy.max(saved.1), saved.2, (yy + 1).min(saved.3));
            if self.clip.3 > self.clip.1 {
                self.round(x, y, w, h, r, c);
            }
        }
        self.clip = saved;
    }

    /// An anti-aliased line, 1 pixel wide (coordinates in 1/16 pixel).
    pub fn line16(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: u32, a: u32) {
        let (dx, dy) = (x1 - x0, y1 - y0);
        let steps = (dx.abs().max(dy.abs()) / 16).max(1);
        // walk along the major axis, splatting coverage onto two pixels
        if dx.abs() >= dy.abs() {
            for i in 0..=steps {
                let x = x0 + dx * i / steps;
                let y = y0 + dy * i / steps;
                let (px, fy) = (x >> 4, y);
                let py = fy >> 4;
                let frac = (fy & 15) as u32 * 16;
                self.put(px, py, c, (255 - frac) * a / 255);
                self.put(px, py + 1, c, frac * a / 255);
            }
        } else {
            for i in 0..=steps {
                let x = x0 + dx * i / steps;
                let y = y0 + dy * i / steps;
                let py = y >> 4;
                let px = x >> 4;
                let frac = (x & 15) as u32 * 16;
                self.put(px, py, c, (255 - frac) * a / 255);
                self.put(px + 1, py, c, frac * a / 255);
            }
        }
    }

    pub fn line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: u32) {
        self.line16(x0 * 16 + 8, y0 * 16 + 8, x1 * 16 + 8, y1 * 16 + 8, c, 255);
    }

    pub fn thick_line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, width: i32, c: u32) {
        let (dx, dy) = (x1 - x0, y1 - y0);
        let len = isqrt((dx * dx + dy * dy) as i64).max(1) as i32;
        let steps = len.max(1);
        let r = (width / 2).max(1);
        for i in 0..=steps {
            self.circle(x0 + dx * i / steps, y0 + dy * i / steps, r, c);
        }
    }

    /// Text; returns the width drawn.
    pub fn text(&mut self, x: i32, y: i32, s: &str, f: &Font, c: u32) -> i32 {
        self.text_a(x, y, s, f, c, 255)
    }

    pub fn text_a(&mut self, x: i32, y: i32, s: &str, f: &Font, c: u32, a: u32) -> i32 {
        let mut cx = x;
        let (ax, ay) = (self.ox, self.oy);
        let fh = f.height() as i32;
        if y + ay + fh < self.clip.1 || y + ay >= self.clip.3 {
            return text_width(f, s);
        }
        let aa = self.a(a);
        for ch in s.chars() {
            let (lo, adv) = glyph_metrics(f, ch);
            if ch != ' ' && cx + ax + adv >= self.clip.0 && cx + ax < self.clip.2 {
                let raster = f.raster(ch);
                for (ry, row) in raster.iter().enumerate() {
                    let yy = y + ay + ry as i32;
                    if yy < self.clip.1 || yy >= self.clip.3 {
                        continue;
                    }
                    let base = yy as usize * self.stride;
                    for (rx, &cov) in row.iter().enumerate() {
                        if cov < 10 {
                            continue;
                        }
                        let xx = cx + ax + rx as i32 - lo;
                        if xx >= self.clip.0 && xx < self.clip.2 {
                            let i = base + xx as usize;
                            self.px[i] = blend(self.px[i], c, cov as u32 * aa / 255);
                        }
                    }
                }
            }
            cx += adv;
        }
        cx - x
    }

    pub fn text_center(&mut self, cx: i32, y: i32, s: &str, f: &Font, c: u32) {
        let w = text_width(f, s);
        self.text(cx - w / 2, y, s, f, c);
    }

    pub fn text_right(&mut self, rx: i32, y: i32, s: &str, f: &Font, c: u32) {
        let w = text_width(f, s);
        self.text(rx - w, y, s, f, c);
    }

    /// Copies 32-bit pixels (opaque).
    pub fn blit(&mut self, x: i32, y: i32, src: &[u32], sw: usize, sh: usize) {
        let (x, y) = (x + self.ox, y + self.oy);
        let x0 = x.max(self.clip.0);
        let x1 = (x + sw as i32).min(self.clip.2);
        if x1 <= x0 {
            return;
        }
        let a = self.a(255);
        for row in 0..sh as i32 {
            let yy = y + row;
            if yy < self.clip.1 || yy >= self.clip.3 {
                continue;
            }
            let s = &src[row as usize * sw + (x0 - x) as usize..row as usize * sw + (x1 - x) as usize];
            let d0 = yy as usize * self.stride + x0 as usize;
            let d = &mut self.px[d0..d0 + (x1 - x0) as usize];
            if a >= 255 {
                d.copy_from_slice(s);
            } else {
                for (p, q) in d.iter_mut().zip(s) {
                    *p = blend(*p, *q, a);
                }
            }
        }
    }

    /// Copies pixels with alpha in the top byte (0xAARRGGBB).
    pub fn blit_alpha(&mut self, x: i32, y: i32, src: &[u32], sw: usize, sh: usize) {
        let (x, y) = (x + self.ox, y + self.oy);
        for row in 0..sh as i32 {
            let yy = y + row;
            if yy < self.clip.1 || yy >= self.clip.3 {
                continue;
            }
            for col in 0..sw as i32 {
                let xx = x + col;
                if xx < self.clip.0 || xx >= self.clip.2 {
                    continue;
                }
                let s = src[row as usize * sw + col as usize];
                let a = self.a(s >> 24);
                if a > 0 {
                    let i = yy as usize * self.stride + xx as usize;
                    self.px[i] = blend(self.px[i], s & 0xFFFFFF, a);
                }
            }
        }
    }

    /// Draws `src` scaled into a rectangle (bilinear when `smooth`), with
    /// `alpha_channel` using the top byte as alpha.
    pub fn blit_scaled(&mut self, dst: Rect, src: &[u32], sw: usize, sh: usize, smooth: bool, alpha_channel: bool) {
        let (dx, dy, dw, dh) = (dst.0 + self.ox, dst.1 + self.oy, dst.2, dst.3);
        if dw <= 0 || dh <= 0 || sw == 0 || sh == 0 {
            return;
        }
        let x0 = dx.max(self.clip.0);
        let x1 = (dx + dw).min(self.clip.2);
        let y0 = dy.max(self.clip.1);
        let y1 = (dy + dh).min(self.clip.3);
        let fx = ((sw as i64) << 16) / dw as i64;
        let fy = ((sh as i64) << 16) / dh as i64;
        let ga = self.a(255);
        for yy in y0..y1 {
            let sy16 = ((yy - dy) as i64 * fy + fy / 2 - 32768).max(0);
            let sy = ((sy16 >> 16) as usize).min(sh - 1);
            let sy2 = (sy + 1).min(sh - 1);
            let ty = ((sy16 >> 8) & 255) as u32;
            let base = yy as usize * self.stride;
            for xx in x0..x1 {
                let sx16 = ((xx - dx) as i64 * fx + fx / 2 - 32768).max(0);
                let sx = ((sx16 >> 16) as usize).min(sw - 1);
                let s = if smooth {
                    let sx2 = (sx + 1).min(sw - 1);
                    let tx = ((sx16 >> 8) & 255) as u32;
                    let a = mix(src[sy * sw + sx], src[sy * sw + sx2], tx);
                    let b = mix(src[sy2 * sw + sx], src[sy2 * sw + sx2], tx);
                    let c = mix(a, b, ty);
                    if alpha_channel {
                        let aa = mix(src[sy * sw + sx] >> 24, src[sy2 * sw + sx2] >> 24, 128);
                        (c & 0xFFFFFF) | (aa << 24)
                    } else {
                        c
                    }
                } else {
                    src[sy * sw + sx]
                };
                let a = if alpha_channel { (s >> 24) * ga / 255 } else { ga };
                let i = base + xx as usize;
                self.px[i] = blend(self.px[i], s & 0xFFFFFF, a);
            }
        }
    }

    /// A soft shadow around a rounded rectangle (drawn outside it only).
    pub fn shadow(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, blur: i32, strength: u32) {
        self.drop_shadow(x, y, w, h, r, blur, strength, 0);
    }

    /// A shadow cast by the rectangle (x, y, w, h) onto a surface `dy` pixels
    /// below it: only what the rectangle itself does not cover is drawn.
    pub fn drop_shadow(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, blur: i32, strength: u32, dy: i32) {
        if blur <= 0 || w <= 0 || h <= 0 {
            return;
        }
        let (ax, ay) = (x + self.ox, y + self.oy);
        let sy = ay + dy;
        let x0 = (ax - blur).max(self.clip.0);
        let x1 = (ax + w + blur).min(self.clip.2);
        let y0 = (sy - blur).max(self.clip.1);
        let y1 = (sy + h + blur).min(self.clip.3);
        let r = r.min(w / 2).min(h / 2);
        let b2 = (blur * blur) as i64;
        for yy in y0..y1 {
            let dyy = if yy < sy + r { sy + r - yy } else if yy >= sy + h - r { yy - (sy + h - r - 1) } else { 0 };
            let in_object_rows = yy >= ay + r && yy < ay + h - r;
            let base = yy as usize * self.stride;
            let mut xx = x0;
            while xx < x1 {
                // the object covers this run: jump over it
                if in_object_rows && xx >= ax && xx < ax + w {
                    xx = ax + w;
                    continue;
                }
                let dx = if xx < ax + r { ax + r - xx } else if xx >= ax + w - r { xx - (ax + w - r - 1) } else { 0 };
                let d = if dx > 0 && dyy > 0 { isqrt((dx * dx + dyy * dyy) as i64) as i32 - r } else { dx.max(dyy) - r };
                let d = d.max(0) as i64;
                if d < blur as i64 {
                    let t = b2 - d * d;
                    let a = self.a(((t * t * strength as i64) / (b2 * b2).max(1)).min(255) as u32);
                    if a > 0 {
                        let i = base + xx as usize;
                        self.px[i] = blend(self.px[i], 0, a);
                    }
                }
                xx += 1;
            }
        }
    }

    /// Blurs a rectangle in place (two box passes each way): frosted glass.
    pub fn blur(&mut self, x: i32, y: i32, w: i32, h: i32, radius: i32) {
        let (ax, ay) = (x + self.ox, y + self.oy);
        let x0 = ax.max(self.clip.0).max(0);
        let y0 = ay.max(self.clip.1).max(0);
        let x1 = (ax + w).min(self.clip.2).min(self.w);
        let y1 = (ay + h).min(self.clip.3).min(self.h);
        if x1 - x0 < 3 || y1 - y0 < 3 || radius <= 0 {
            return;
        }
        let (bw, bh) = ((x1 - x0) as usize, (y1 - y0) as usize);
        let mut buf = vec![0u32; bw.max(bh)];
        for _pass in 0..2 {
            for yy in y0..y1 {
                let row = yy as usize * self.stride + x0 as usize;
                buf[..bw].copy_from_slice(&self.px[row..row + bw]);
                box_line(&buf[..bw], &mut self.px[row..row + bw], 1, radius as usize);
            }
            for xx in x0..x1 {
                for i in 0..bh {
                    buf[i] = self.px[(y0 as usize + i) * self.stride + xx as usize];
                }
                let mut out = vec![0u32; bh];
                box_line(&buf[..bh], &mut out, 1, radius as usize);
                for i in 0..bh {
                    self.px[(y0 as usize + i) * self.stride + xx as usize] = out[i];
                }
            }
        }
    }
}

fn box_line(src: &[u32], dst: &mut [u32], _step: usize, r: usize) {
    let n = src.len();
    let (mut sr, mut sg, mut sb) = (0u32, 0u32, 0u32);
    let window = (2 * r + 1) as u32;
    let get = |i: isize| -> u32 { src[i.clamp(0, n as isize - 1) as usize] };
    for i in -(r as isize)..=(r as isize) {
        let p = get(i);
        sr += (p >> 16) & 255;
        sg += (p >> 8) & 255;
        sb += p & 255;
    }
    for i in 0..n {
        dst[i] = ((sr / window) << 16) | ((sg / window) << 8) | (sb / window);
        let out = get(i as isize - r as isize);
        let inn = get(i as isize + r as isize + 1);
        sr = sr + ((inn >> 16) & 255) - ((out >> 16) & 255);
        sg = sg + ((inn >> 8) & 255) - ((out >> 8) & 255);
        sb = sb + (inn & 255) - (out & 255);
    }
}

/// Ease-out cubic, `t` in 0..=1000.
pub fn ease_out(t: i32) -> i32 {
    let t = t.clamp(0, 1000) as i64;
    let u = 1000 - t;
    (1000 - u * u * u / 1_000_000) as i32
}

pub fn ease_in_out(t: i32) -> i32 {
    let t = t.clamp(0, 1000) as i64;
    if t < 500 {
        (4 * t * t * t / 1_000_000) as i32
    } else {
        let u = 1000 - t;
        (1000 - 4 * u * u * u / 1_000_000) as i32
    }
}

/// Spring-like overshoot, `t` 0..=1000 -> roughly 0..=1000 with a small bounce.
pub fn ease_back(t: i32) -> i32 {
    let t = t.clamp(0, 1000) as i64;
    // (t-1)^2 * ((s+1)(t-1) + s) + 1 with s = 1.4
    let u = t - 1000;
    let v = u * u / 1000 * (2400 * u / 1000 + 1400) / 1000 + 1000;
    v as i32
}

pub fn lerp(a: i32, b: i32, t: i32) -> i32 {
    a + ((b - a) as i64 * t as i64 / 1000) as i32
}
