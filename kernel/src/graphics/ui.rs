//! The look of the system, modelled on the Zed editor's dark theme:
//! flat panels, one-pixel borders, a slim title bar with a tab chip, a status
//! bar, rounded corners and a blue accent. Colors are Zed's "One Dark".

use crate::fs::FILESYSTEM;
use crate::graphics::console::CONSOLE;
use crate::graphics::display::Display;
use crate::graphics::font::Font;
use crate::graphics::framebuffer::{self, Framebuffer};
use alloc::format;
use alloc::string::String;
use core::sync::atomic::{AtomicU64, Ordering};
use noto_sans_mono_bitmap::RasterHeight;

/// The editor / terminal background.
pub const BG_EDITOR: u32 = 0x282C34;
/// Title bar and status bar.
pub const BG_BAR: u32 = 0x3B414D;
/// Slightly raised panels.
pub const BG_PANEL: u32 = 0x2F343E;
pub const BORDER: u32 = 0x464B57;
pub const TEXT: u32 = 0xDCE0E5;
pub const TEXT_MUTED: u32 = 0xA9AFBC;
pub const TEXT_FAINT: u32 = 0x5D636F;
pub const ACCENT: u32 = 0x74ADE8;
pub const GREEN: u32 = 0xA1C181;
pub const YELLOW: u32 = 0xDEC184;
pub const RED: u32 = 0xD07277;
pub const PURPLE: u32 = 0xB477CF;
pub const CYAN: u32 = 0x6EB4BF;
pub const ORANGE: u32 = 0xBF956A;

/// True while the desktop is running: windows have no title and status bars of
/// their own (the desktop draws the window frame).
pub static GUI: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

pub fn is_gui() -> bool {
    gui()
}

fn gui() -> bool {
    GUI.load(Ordering::Relaxed)
}

/// Where things go on a screen of a given size.
#[derive(Clone, Copy)]
pub struct Layout {
    pub width: usize,
    pub height: usize,
    /// The font used for the bars.
    pub font: Font,
    pub title_h: usize,
    pub status_h: usize,
}

impl Layout {
    /// Top of the area between the bars.
    pub fn body_y(&self) -> usize {
        if self.title_h == 0 { 0 } else { self.title_h + 1 }
    }

    /// Height of the area between the bars.
    pub fn body_h(&self) -> usize {
        if self.title_h == 0 && self.status_h == 0 {
            return self.height;
        }
        self.height.saturating_sub(self.title_h + self.status_h + 2)
    }
}

pub fn layout(width: usize, height: usize) -> Layout {
    let size = if width >= 2400 {
        RasterHeight::Size24
    } else if width >= 1500 {
        RasterHeight::Size20
    } else {
        RasterHeight::Size16
    };
    let font = Font::regular(size);
    Layout {
        width,
        height,
        font,
        title_h: if gui() { 0 } else { font.height() + 18 },
        // tall enough for the buddy to stand in
        status_h: if gui() { 0 } else { (font.height() + 10).max(46).max(height / 18) },
    }
}

fn isqrt(n: usize) -> usize {
    let mut r = 0;
    while (r + 1) * (r + 1) <= n {
        r += 1;
    }
    r
}

/// A filled rectangle with rounded corners of radius `r`.
pub fn fill_round(fb: &mut Framebuffer, x: usize, y: usize, w: usize, h: usize, r: usize, color: u32) {
    let r = r.min(w / 2).min(h / 2);
    for i in 0..r {
        let dy = r - i;
        let inset = r - isqrt(r * r - (dy - 1) * (dy - 1));
        fb.fill_rect(x + inset, y + i, w - 2 * inset, 1, color);
        fb.fill_rect(x + inset, y + h - 1 - i, w - 2 * inset, 1, color);
    }
    fb.fill_rect(x, y + r, w, h - 2 * r, color);
}

/// The title bar: a tab chip with the title on the left, a hint on the right.
pub fn draw_title(fb: &mut Framebuffer, lay: &Layout, title: &str, hint: &str) {
    fb.fill_rect(0, 0, lay.width, lay.title_h, BG_BAR);
    fb.fill_rect(0, lay.title_h, lay.width, 1, BORDER);

    let font = lay.font;
    let chip_h = font.height() + 8;
    let chip_y = (lay.title_h - chip_h) / 2;
    let chip_x = 10;
    let dot = font.height() / 3;
    let chip_w = font.text_width(title) + dot + font.width() * 2 + 8;
    fill_round(fb, chip_x, chip_y, chip_w, chip_h, 6, BG_EDITOR);
    fill_round(fb, chip_x + 8, chip_y + (chip_h - dot) / 2, dot, dot, dot / 2, ACCENT);
    font.draw_text(fb, chip_x + 8 + dot + font.width() / 2 + 4, chip_y + 4, title, TEXT, BG_EDITOR);

    let hint_w = font.text_width(hint);
    let text_y = (lay.title_h - font.height()) / 2;
    font.draw_text(fb, lay.width.saturating_sub(hint_w + 14), text_y, hint, TEXT_MUTED, BG_BAR);
}

/// The status bar along the bottom.
pub fn draw_status(fb: &mut Framebuffer, lay: &Layout, left: &str, right: &str) {
    let y = lay.height - lay.status_h;
    fb.fill_rect(0, y - 1, lay.width, 1, BORDER);
    fb.fill_rect(0, y, lay.width, lay.status_h, BG_BAR);
    let font = lay.font;
    let text_y = y + (lay.status_h - font.height()) / 2;
    font.draw_text(fb, 12, text_y, left, TEXT_MUTED, BG_BAR);
    let right_w = font.text_width(right);
    font.draw_text(fb, lay.width.saturating_sub(right_w + 12), text_y, right, TEXT_MUTED, BG_BAR);

    // the buddy wanders in the space between the two texts
    let lane_x = 12 + font.text_width(left) + 24;
    let lane_end = lay.width.saturating_sub(right_w + 12 + 24);
    let scale = (lay.status_h.saturating_sub(2)) / 22;
    if scale >= 1 && lane_end > lane_x + 26 * scale {
        // Bit may jump higher than the bar; keep it inside
        fb.set_clip(0, y, lay.width, lay.status_h);
        crate::graphics::buddy::draw(fb, lane_x, lane_end - lane_x, lay.height - 1, scale);
        fb.clear_clip();
        // what Bit is thinking, in a little bubble beside it
        if let Some(text) = crate::graphics::buddy::thought() {
            let bx = crate::graphics::buddy::screen_x(lane_x, lane_end - lane_x, scale) + 26 * scale + 6;
            let w = font.text_width(&text) + 16;
            if bx + w < lane_end {
                fill_round(fb, bx, y + (lay.status_h - font.height() - 8) / 2, w, font.height() + 8, 6, BG_PANEL);
                font.draw_text(fb, bx + 8, y + (lay.status_h - font.height()) / 2, &text, TEXT, BG_PANEL);
            }
        }
    }
}

// ----- the shell's window -----

/// Text for the shell's status bar: the current directory, and some facts.
fn shell_status(width: usize, height: usize) -> (String, String) {
    let cwd = FILESYSTEM
        .lock()
        .as_ref()
        .map(|fs| fs.get_cwd())
        .unwrap_or_else(|| String::from("/"));
    let (h, m, s) = crate::time::split_hms(crate::time::uptime_secs());
    let heap = crate::allocator::heap_used() / 1024;
    (
        format!("{}", cwd),
        format!("heap {}K  |  {}x{}  |  up {:02}:{:02}:{:02}", heap, width, height, h, m, s),
    )
}

/// Draws the shell's title bar and status bar.
pub fn draw_shell_chrome(fb: &mut Framebuffer) {
    if gui() {
        return;
    }
    let lay = layout(fb.width(), fb.height());
    let (left, right) = shell_status(fb.width(), fb.height());
    draw_title(fb, &lay, "terminal", concat!("zero ", env!("CARGO_PKG_VERSION")));
    draw_status(fb, &lay, &left, &right);
}

static LAST_STATUS_SECOND: AtomicU64 = AtomicU64::new(u64::MAX);
static LAST_BUDDY_FRAME: AtomicU64 = AtomicU64::new(0);

/// Redraws just the status bar (after a command, or when the clock moves).
pub fn refresh_status() {
    if gui() {
        return;
    }
    let (w, h) = match framebuffer::with(|fb| (fb.width(), fb.height())) {
        Some(size) => size,
        None => return,
    };
    let (left, right) = shell_status(w, h);
    framebuffer::with(|fb| {
        let lay = layout(fb.width(), fb.height());
        draw_status(fb, &lay, &left, &right);
        fb.flush();
    });
    LAST_STATUS_SECOND.store(crate::time::uptime_secs(), Ordering::Relaxed);
    LAST_BUDDY_FRAME.store(crate::time::uptime_ms(), Ordering::Relaxed);
}

/// Called while the shell waits for a key: keeps the clock in the status bar
/// current and the buddy moving (about ten frames a second).
pub fn tick_status() {
    if gui() {
        return;
    }
    let now = crate::time::uptime_ms();
    if LAST_STATUS_SECOND.load(Ordering::Relaxed) != crate::time::uptime_secs()
        || now.saturating_sub(LAST_BUDDY_FRAME.load(Ordering::Relaxed)) >= 100
    {
        refresh_status();
    }
}

/// Keeps the buddy moving in a status bar that something else owns (a script
/// screen, a 3D demo): redraws just that bar when a frame is due.
pub fn tick_buddy(left: &str, right: &str) {
    if gui() {
        return;
    }
    let now = crate::time::uptime_ms();
    if now.saturating_sub(LAST_BUDDY_FRAME.load(Ordering::Relaxed)) < 100 {
        return;
    }
    LAST_BUDDY_FRAME.store(now, Ordering::Relaxed);
    framebuffer::with(|fb| {
        let lay = layout(fb.width(), fb.height());
        draw_status(fb, &lay, left, right);
        fb.flush();
    });
}

/// Repaints the whole shell window: background, bars and console text.
pub fn restore_shell_screen() {
    framebuffer::with(|fb| {
        fb.clear_to(BG_EDITOR);
        draw_shell_chrome(fb);
        fb.flush();
    });
    LAST_STATUS_SECOND.store(crate::time::uptime_secs(), Ordering::Relaxed);
    CONSOLE.lock().restore();
}

/// Mixes two colors; `t` is 0 for `a` and 255 for `b`.
pub fn lerp(a: u32, b: u32, t: u32) -> u32 {
    let t = t.min(255);
    let ch = |shift: u32| {
        let (x, y) = ((a >> shift) & 255, (b >> shift) & 255);
        ((x * (255 - t) + y * t) / 255) << shift
    };
    ch(16) | ch(8) | ch(0)
}
