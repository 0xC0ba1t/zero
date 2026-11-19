//! The window manager and compositor: the desktop thread.

use super::apps::{self, App, AppInfo};
use super::draw::{self, accent, BG, BG_PANEL, BORDER, TEXT, TEXT_FAINT, TEXT_MUTED};
use super::Event;
use crate::graphics::display::Display;
use crate::graphics::font::Font;
use crate::graphics::framebuffer::{self, Framebuffer, Rect};
use crate::graphics::ui;
use crate::input::{self, Key};
use crate::sched::{self, Context};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use pc_keyboard::KeyCode;

// ---- settings the Settings app changes -------------------------------------

/// Which wallpaper (an index into [`WALLPAPERS`]).
pub static WALLPAPER: AtomicUsize = AtomicUsize::new(0);
/// Windows tile themselves automatically.
pub static TILING: AtomicBool = AtomicBool::new(false);
/// The dock icons grow near the pointer.
pub static MAGNIFY: AtomicBool = AtomicBool::new(true);
/// The accent colors the user can choose from.
pub const ACCENTS: [(&str, u32); 6] = [
    ("Blue", 0x61AFEF),
    ("Green", 0x98C379),
    ("Purple", 0xC678DD),
    ("Orange", 0xD19A66),
    ("Pink", 0xFF79A8),
    ("Teal", 0x56B6C2),
];

pub const WALLPAPERS: [&str; 5] = ["Dusk", "Ocean", "Forest", "Graphite", "Sunrise"];

/// Reads the saved desktop settings.
pub fn load_settings() {
    if let Some(v) = crate::settings::get("wallpaper") {
        WALLPAPER.store(v.parse().unwrap_or(0).min(WALLPAPERS.len() - 1), Ordering::Relaxed);
    }
    if let Some(v) = crate::settings::get("tiling") {
        TILING.store(v == "1", Ordering::Relaxed);
    }
    if let Some(v) = crate::settings::get("magnify") {
        MAGNIFY.store(v != "0", Ordering::Relaxed);
    }
    if let Some(v) = crate::settings::get("accent") {
        if let Ok(c) = u32::from_str_radix(&v, 16) {
            draw::set_accent(c);
        }
    }
}

// ---- geometry ----------------------------------------------------------------

/// x, y, width, height
type Xywh = (i32, i32, i32, i32);

fn to_rect(r: Xywh) -> Rect {
    (r.0, r.1, r.0 + r.2, r.1 + r.3)
}

fn union(a: Option<Rect>, b: Rect) -> Rect {
    match a {
        None => b,
        Some(a) => (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)),
    }
}

fn intersects(a: Rect, b: Rect) -> bool {
    a.0 < b.2 && b.0 < a.2 && a.1 < b.3 && b.1 < a.3
}

fn within(x: i32, y: i32, r: Xywh) -> bool {
    x >= r.0 && y >= r.1 && x < r.0 + r.2 && y < r.1 + r.3
}

const EDGE_L: u8 = 1;
const EDGE_R: u8 = 2;
const EDGE_T: u8 = 4;
const EDGE_B: u8 = 8;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Part {
    Close,
    Minimize,
    Maximize,
    Title,
    Edge(u8),
    Content,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Hit {
    Desktop,
    Menubar,
    Dock(usize),
    Window(u32, Part),
}

#[derive(Clone, Copy)]
enum Drag {
    None,
    Move { id: u32, ox: i32, oy: i32, moved: bool },
    Resize { id: u32, edge: u8, mouse: (i32, i32), start: Xywh },
    /// The button went down inside a window's content: moves and the release go to it.
    Content { id: u32 },
}

struct Win {
    id: u32,
    app: App,
    title: String,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    ctx: Arc<Context>,
    done: Arc<AtomicBool>,
    minimized: bool,
    maximized: bool,
    snapped: bool,
    restore: Xywh,
    last_version: u64,
    keep_alive: bool,
    /// The mouse is captured by the program (Doom): all movement goes to it.
    captured: bool,
}

struct Launcher {
    query: String,
    selected: usize,
}

pub struct Desktop {
    sw: i32,
    sh: i32,
    s: i32,
    wins: Vec<Win>,
    next_id: u32,
    mouse: (i32, i32),
    buttons: u8,
    drag: Drag,
    wallpaper: Vec<u8>,
    wallpaper_index: usize,
    damage: Vec<Rect>,
    hover_dock: Option<usize>,
    dock_mouse_x: i32,
    snap_preview: Option<Xywh>,
    menu_open: bool,
    menu_hover: Option<usize>,
    launcher: Option<Launcher>,
    last_click: (u64, u32),
    last_compose: u64,
    /// Average time to compose a frame, in hundredths of a millisecond.
    pub compose_ms: u32,
    last_second: u64,
    clock: (u8, u8, u8, u8, u8),
    tiling: bool,
    quit: Option<bool>,
    f_small: Font,
    f_menu: Font,
    f_menu_bold: Font,
}

const MENU_ITEMS: [&str; 5] = ["About zero", "Settings", "Terminal", "Restart", "Shut Down"];

pub fn run() {
    // Very large screens: draw at half size (crisp 2x scaling by the display layer),
    // which keeps the interface readable and makes every frame 4x cheaper.
    framebuffer::with_screen(|fb| {
        let (pw, ph) = fb.physical();
        if pw >= 2000 && fb.width() == pw {
            fb.set_logical(pw / 2, ph / 2);
        }
    });
    let Some((sw, sh)) = framebuffer::with_screen(|fb| (fb.width() as i32, fb.height() as i32)) else { return };
    load_settings();
    ui::GUI.store(true, Ordering::SeqCst);
    let mouse = crate::mouse::enable();
    crate::serial_println!("[gui] mouse: {}", if mouse { "yes" } else { "no" });
    crate::mouse::keep_enabled(true);
    let cpus = crate::smp::start();
    crate::serial_println!("[smp] {} processor(s)", cpus);
    crate::net::start();
    crate::audio::start();
    let mut d = Desktop::new(sw, sh);
    d.open(App::Terminal);
    sched::enable_preemption(true);
    d.main_loop();
    sched::enable_preemption(false);
}

impl Desktop {
    fn new(sw: i32, sh: i32) -> Desktop {
        let s = (sw / 1280).max(1).min(3);
        let px = |v: i32| (v * s) as usize;
        let mut d = Desktop {
            sw,
            sh,
            s,
            wins: Vec::new(),
            next_id: 1,
            mouse: (sw / 2, sh / 2),
            buttons: 0,
            drag: Drag::None,
            wallpaper: Vec::new(),
            wallpaper_index: usize::MAX,
            damage: Vec::new(),
            hover_dock: None,
            dock_mouse_x: 0,
            snap_preview: None,
            menu_open: false,
            menu_hover: None,
            launcher: None,
            last_click: (0, 0),
            last_compose: 0,
            compose_ms: 0,
            last_second: u64::MAX,
            clock: (0, 0, 0, 0, 0),
            tiling: TILING.load(Ordering::Relaxed),
            quit: None,
            f_small: draw::font(px(15), false),
            f_menu: draw::font(px(15), false),
            f_menu_bold: draw::font(px(15), true),
        };
        d.make_wallpaper();
        d.damage_all();
        d
    }

    // ---- sizes -------------------------------------------------------------

    fn menu_h(&self) -> i32 {
        26 * self.s
    }

    fn title_h(&self) -> i32 {
        30 * self.s
    }

    fn radius(&self) -> i32 {
        10 * self.s
    }

    fn dock_base(&self) -> i32 {
        46 * self.s
    }

    fn work_area(&self) -> Xywh {
        let reserve = self.dock_base() + 36 * self.s;
        (0, self.menu_h(), self.sw, self.sh - self.menu_h() - reserve)
    }

    fn damage_rect(&mut self, r: Rect) {
        let mut r = (r.0.max(0), r.1.max(0), r.2.min(self.sw), r.3.min(self.sh));
        if r.2 <= r.0 || r.3 <= r.1 {
            return;
        }
        // merge with rectangles it touches (or that cost little extra to merge)
        let area = |q: Rect| ((q.2 - q.0) as i64) * ((q.3 - q.1) as i64);
        let mut i = 0;
        while i < self.damage.len() {
            let d = self.damage[i];
            let u = union(Some(d), r);
            if intersects(d, r) && area(u) <= (area(d) + area(r)) * 3 / 2 + 4000 {
                r = u;
                self.damage.swap_remove(i);
                i = 0;
            } else {
                i += 1;
            }
        }
        self.damage.push(r);
        if self.damage.len() > 6 {
            let all = self.damage.iter().fold(None, |acc, q| Some(union(acc, *q))).unwrap();
            self.damage.clear();
            self.damage.push(all);
        }
    }

    /// The dock shows which apps are running: repaint it when that changes.
    fn dock_dirty(&mut self) {
        let z = self.dock_zone();
        self.damage_rect(z);
    }

    fn damage_all(&mut self) {
        self.damage.clear();
        self.damage.push((0, 0, self.sw, self.sh));
    }

    // ---- the wallpaper -----------------------------------------------------

    fn make_wallpaper(&mut self) {
        let index = WALLPAPER.load(Ordering::Relaxed);
        if index == self.wallpaper_index && !self.wallpaper.is_empty() {
            return;
        }
        self.wallpaper_index = index;
        let (sw, sh) = (self.sw, self.sh);
        // (top, bottom, glow 1: x%, y%, radius%, color, glow 2)
        let (top, bottom, glows): (u32, u32, [(i32, i32, i32, u32); 2]) = match index {
            1 => (0x0B1D33, 0x0F4C5C, [(25, 30, 55, 0x1E90FF), (80, 85, 45, 0x00C2A8)]),
            2 => (0x0E1F17, 0x24432E, [(20, 25, 50, 0x3C8D5A), (85, 80, 40, 0x9ACD32)]),
            3 => (0x16181D, 0x2B2F38, [(30, 20, 50, 0x5C6370), (80, 90, 40, 0x3D4350)]),
            4 => (0x2A1736, 0x7A3E4B, [(25, 80, 55, 0xFF9E64), (80, 25, 45, 0xF7768E)]),
            _ => (0x14162B, 0x3A2A55, [(22, 25, 55, 0x4C6EF5), (82, 85, 45, 0xC056A0)]),
        };
        let like = framebuffer::with_screen(|fb| (fb.pitch(), fb.bytes_per_pixel(), fb.pack(0x123456))).unwrap_or((0, 4, [0; 4]));
        let (pitch, bpp, _) = like;
        let mut data = alloc::vec![0u8; pitch * sh as usize];
        let rgb = framebuffer::with_screen(|fb| fb.pack(0x112233)[0] == 0x11).unwrap_or(false);
        let started = crate::time::uptime_ms();
        // every core paints bands of rows
        struct Out(*mut u8);
        impl Out {
            fn ptr(&self) -> *mut u8 {
                self.0
            }
        }
        unsafe impl Sync for Out {}
        let out = Out(data.as_mut_ptr());
        const BAND: i32 = 8;
        let band_fn = |band: usize| {
            let y0 = band as i32 * BAND;
            for y in y0..(y0 + BAND).min(sh) {
                let base = ui::lerp(top, bottom, (y * 255 / sh) as u32);
                for x in 0..sw {
                    let mut c = base;
                    for (gx, gy, gr, color) in glows {
                        let (cx, cy) = (sw * gx / 100, sh * gy / 100);
                        let radius = (sw.max(sh) * gr / 100).max(1) as i64;
                        let (dx, dy) = ((x - cx) as i64, (y - cy) as i64);
                        let d2 = dx * dx + dy * dy;
                        let r2 = radius * radius;
                        if d2 < r2 {
                            let t = (r2 - d2) * 255 / r2;
                            let strength = (t * t / 255 * 90 / 255) as u32;
                            c = ui::lerp(c, color, strength);
                        }
                    }
                    let mut px = pack_color(c, bpp);
                    if rgb && bpp >= 3 {
                        px.swap(0, 2);
                    }
                    let o = y as usize * pitch + x as usize * bpp;
                    unsafe { core::ptr::copy_nonoverlapping(px.as_ptr(), out.ptr().add(o), bpp.min(4)) };
                }
            }
        };
        crate::smp::parallel_for(((sh + BAND - 1) / BAND) as usize, &band_fn);
        crate::serial_println!("[gui] wallpaper {}x{} in {} ms on {} processor(s)", sw, sh, crate::time::uptime_ms() - started, crate::smp::cpu_count());
        self.wallpaper = data;
        self.damage_all();
    }

    // ---- windows -----------------------------------------------------------

    fn win_index(&self, id: u32) -> Option<usize> {
        self.wins.iter().position(|w| w.id == id)
    }

    fn visible_wins(&self) -> impl Iterator<Item = &Win> {
        self.wins.iter().filter(|w| !w.minimized)
    }

    fn focused_id(&self) -> Option<u32> {
        self.wins.iter().rev().find(|w| !w.minimized).map(|w| w.id)
    }

    fn win_rect(w: &Win) -> Xywh {
        (w.x, w.y, w.w, w.h)
    }

    /// The area a window's shadow and frame cover.
    fn win_damage(&self, w: &Win) -> Rect {
        let m = 18 * self.s;
        (w.x - m, w.y - m / 2, w.x + w.w + m, w.y + w.h + m * 2)
    }

    fn open(&mut self, app: App) {
        let info = match apps::info(app) {
            Some(i) => i,
            None => return,
        };
        if info.single {
            if let Some(i) = self.wins.iter().position(|w| w.app == app) {
                let id = self.wins[i].id;
                self.restore(id);
                self.focus(id);
                return;
            }
        }
        let (cw, ch) = (info.size.0 * self.s, info.size.1 * self.s);
        let work = self.work_area();
        let w = (cw).min(work.2 - 20 * self.s);
        let title_h = self.title_h();
        let h = (ch + title_h).min(work.3 - 10 * self.s);
        let n = self.wins.len() as i32;
        let x = (work.0 + (work.2 - w) / 2 + (n % 6 - 3) * 24 * self.s).clamp(work.0, (work.0 + work.2 - w).max(work.0));
        let y = (work.1 + (work.3 - h) / 3 + (n % 5) * 18 * self.s).clamp(work.1, (work.1 + work.3 - h).max(work.1));
        let surface = match framebuffer::with_screen(|fb| Framebuffer::offscreen(fb, w as usize, (h - title_h) as usize)) {
            Some(s) => s,
            None => return,
        };
        let ctx = Context::new(surface);
        // start with a calm background so nothing flashes
        {
            let mut s = ctx.surface.lock();
            let (sw_, sh_) = (s.width() as i32, s.height() as i32);
            draw::rect(&mut s, 0, 0, sw_, sh_, BG);
        }
        crate::audio::effect(crate::audio::Effect::Open);
        let done = apps::start(app, ctx.clone());
        let id = self.next_id;
        self.next_id += 1;
        self.wins.push(Win {
            id,
            app,
            title: String::from(info.name),
            x,
            y,
            w,
            h,
            ctx,
            done,
            minimized: false,
            maximized: false,
            snapped: false,
            restore: (x, y, w, h),
            last_version: u64::MAX,
            keep_alive: info.keep_alive,
            captured: false,
        });
        self.focus(id);
        if self.tiling {
            self.retile();
        }
        let r = self.win_damage(self.wins.last().unwrap());
        self.damage_rect(r);
        self.dock_dirty();
    }

    fn focus(&mut self, id: u32) {
        let Some(i) = self.win_index(id) else { return };
        let old = self.focused_id();
        if self.wins[i].minimized {
            self.wins[i].minimized = false;
            self.wins[i].ctx.visible.store(true, Ordering::Relaxed);
        }
        let w = self.wins.remove(i);
        self.wins.push(w);
        for w in self.wins.iter() {
            w.ctx.focused.store(w.id == id, Ordering::Relaxed);
        }
        // the focused window's frame looks different
        for wid in [old, Some(id)].into_iter().flatten() {
            if let Some(i) = self.win_index(wid) {
                let r = self.win_damage(&self.wins[i]);
                self.damage_rect(r);
            }
        }
    }

    fn focus_next_visible(&mut self) {
        // after a window goes away, the next one in front gets the keyboard
        if let Some(id) = self.focused_id() {
            for w in self.wins.iter() {
                w.ctx.focused.store(w.id == id, Ordering::Relaxed);
            }
        }
    }

    fn restore(&mut self, id: u32) {
        if let Some(i) = self.win_index(id) {
            if self.wins[i].minimized {
                self.wins[i].minimized = false;
                self.wins[i].ctx.visible.store(true, Ordering::Relaxed);
                let r = self.win_damage(&self.wins[i]);
                self.damage_rect(r);
            }
        }
    }

    fn minimize(&mut self, id: u32) {
        if let Some(i) = self.win_index(id) {
            let r = self.win_damage(&self.wins[i]);
            self.wins[i].minimized = true;
            self.wins[i].captured = false;
            self.wins[i].ctx.visible.store(false, Ordering::Relaxed);
            self.wins[i].ctx.focused.store(false, Ordering::Relaxed);
            self.damage_rect(r);
            self.focus_next_visible();
            if self.tiling {
                self.retile();
            }
        }
    }

    fn close(&mut self, id: u32) {
        let Some(i) = self.win_index(id) else { return };
        if self.wins[i].keep_alive {
            self.minimize(id);
            return;
        }
        crate::audio::effect(crate::audio::Effect::Close);
        let r = self.win_damage(&self.wins[i]);
        self.wins[i].ctx.closed.store(true, Ordering::SeqCst);
        self.wins[i].ctx.focused.store(false, Ordering::SeqCst);
        // the program notices and ends; its window disappears when it has
        self.wins[i].minimized = true;
        self.damage_rect(r);
        self.dock_dirty();
        self.focus_next_visible();
    }

    /// Moves and resizes a window (recreating its surface if the size changed).
    fn set_rect(&mut self, id: u32, r: Xywh) {
        let Some(i) = self.win_index(id) else { return };
        let title_h = self.title_h();
        let old_damage = self.win_damage(&self.wins[i]);
        let (min_w, min_h) = (200 * self.s, title_h + 100 * self.s);
        let (w, h) = (r.2.max(min_w), r.3.max(min_h));
        let size_changed = w != self.wins[i].w || h != self.wins[i].h;
        {
            let win = &mut self.wins[i];
            win.x = r.0;
            win.y = r.1;
            win.w = w;
            win.h = h;
        }
        if size_changed {
            let ctx = self.wins[i].ctx.clone();
            let (cw, ch) = (w as usize, (h - title_h) as usize);
            {
                let mut surface = ctx.surface.lock();
                let fresh = Framebuffer::offscreen(&surface, cw, ch);
                *surface = fresh;
                let (sw_, sh_) = (surface.width() as i32, surface.height() as i32);
                draw::rect(&mut surface, 0, 0, sw_, sh_, BG);
            }
            let mut q = ctx.events.lock();
            q.retain(|e| !matches!(e, Event::Resize { .. }));
            q.push_back(Event::Resize { w: cw, h: ch });
        }
        self.wins[i].last_version = u64::MAX;
        let new_damage = self.win_damage(&self.wins[i]);
        self.damage_rect(old_damage);
        self.damage_rect(new_damage);
    }

    fn toggle_maximize(&mut self, id: u32) {
        let Some(i) = self.win_index(id) else { return };
        if self.wins[i].maximized || self.wins[i].snapped {
            let r = self.wins[i].restore;
            self.wins[i].maximized = false;
            self.wins[i].snapped = false;
            self.set_rect(id, r);
        } else {
            let cur = Self::win_rect(&self.wins[i]);
            self.wins[i].restore = cur;
            self.wins[i].maximized = true;
            let work = self.work_area();
            self.set_rect(id, work);
        }
    }

    fn snap(&mut self, id: u32, r: Xywh) {
        let Some(i) = self.win_index(id) else { return };
        if !self.wins[i].maximized && !self.wins[i].snapped {
            self.wins[i].restore = Self::win_rect(&self.wins[i]);
        }
        self.wins[i].snapped = true;
        self.wins[i].maximized = false;
        self.set_rect(id, r);
    }

    /// Where a window dragged to this pointer position would snap, if anywhere.
    fn snap_target(&self, x: i32, y: i32) -> Option<Xywh> {
        let (sw, _sh) = (self.sw, self.sh);
        let work = self.work_area();
        let edge = 4 * self.s;
        let corner = 70 * self.s;
        let (l, t, w, h) = work;
        let (hw, hh) = (w / 2, h / 2);
        let near_left = x <= edge;
        let near_right = x >= sw - 1 - edge;
        let near_top = y <= self.menu_h() + edge;
        if near_left && y <= t + corner {
            Some((l, t, hw, hh))
        } else if near_right && y <= t + corner {
            Some((l + w - hw, t, hw, hh))
        } else if near_left && y >= t + h - corner {
            Some((l, t + hh, hw, h - hh))
        } else if near_right && y >= t + h - corner {
            Some((l + w - hw, t + hh, hw, h - hh))
        } else if near_left {
            Some((l, t, hw, h))
        } else if near_right {
            Some((l + w - hw, t, hw, h))
        } else if near_top {
            Some(work)
        } else {
            None
        }
    }

    /// Master and stack tiling with gaps, like a tiling window manager.
    fn retile(&mut self) {
        let work = self.work_area();
        let gap = 8 * self.s;
        let mut ids: Vec<u32> = self.wins.iter().filter(|w| !w.minimized).map(|w| w.id).collect();
        ids.sort();
        let n = ids.len() as i32;
        if n == 0 {
            return;
        }
        let (x, y, w, h) = (work.0 + gap, work.1 + gap, work.2 - 2 * gap, work.3 - 2 * gap);
        for (k, id) in ids.iter().enumerate() {
            let r = if n == 1 {
                (x, y, w, h)
            } else {
                let master_w = w * 55 / 100;
                if k == 0 {
                    (x, y, master_w - gap / 2, h)
                } else {
                    let rows = n - 1;
                    let rh = (h - (rows - 1) * gap) / rows;
                    (x + master_w + gap / 2, y + (k as i32 - 1) * (rh + gap), w - master_w - gap / 2, rh)
                }
            };
            if let Some(i) = self.win_index(*id) {
                self.wins[i].maximized = false;
                self.wins[i].snapped = true;
                if !self.wins[i].snapped {
                    self.wins[i].restore = Self::win_rect(&self.wins[i]);
                }
            }
            self.set_rect(*id, r);
        }
    }

    // ---- hit testing -------------------------------------------------------

    fn dock_items() -> Vec<Option<&'static AppInfo>> {
        apps::DOCK.iter().map(|i| i.as_ref()).collect()
    }

    /// The dock's bar, and each icon's rectangle, for the pointer at `mx`.
    fn dock_layout(&self, mx: Option<i32>) -> (Xywh, Vec<Option<Xywh>>) {
        let items = Self::dock_items();
        let base = self.dock_base();
        let pad = 10 * self.s;
        let gap = 8 * self.s;
        let sep = 16 * self.s;
        let magnify = MAGNIFY.load(Ordering::Relaxed);
        // unmagnified centres
        let mut total0 = 2 * pad;
        for (k, it) in items.iter().enumerate() {
            total0 += if it.is_some() { base } else { sep };
            if k + 1 < items.len() {
                total0 += gap;
            }
        }
        let start0 = (self.sw - total0) / 2;
        let mut sizes: Vec<i32> = Vec::new();
        let mut x = start0 + pad;
        for it in items.iter() {
            let (size, centre) = if it.is_some() {
                let c = x + base / 2;
                let f = match mx {
                    Some(mx) if magnify => {
                        let d = (mx - c).abs();
                        let reach = 3 * base;
                        if d < reach { (reach - d) * 100 / reach } else { 0 }
                    }
                    _ => 0,
                };
                (base + base * f * 55 / 10000, c)
            } else {
                (sep, x + sep / 2)
            };
            let _ = centre;
            sizes.push(size);
            x += (if it.is_some() { base } else { sep }) + gap;
        }
        let total: i32 = sizes.iter().sum::<i32>() + gap * (items.len() as i32 - 1) + 2 * pad;
        let bar_h = base + 2 * pad;
        let bar_y = self.sh - 10 * self.s - bar_h;
        let bar = ((self.sw - total) / 2, bar_y, total, bar_h);
        let mut rects = Vec::new();
        let mut x = bar.0 + pad;
        for (k, it) in items.iter().enumerate() {
            let size = sizes[k];
            if it.is_some() {
                rects.push(Some((x, bar_y + pad + base - size, size, size)));
            } else {
                rects.push(None);
            }
            x += size + gap;
        }
        (bar, rects)
    }

    fn dock_zone(&self) -> Rect {
        let (bar, _) = self.dock_layout(None);
        let m = 60 * self.s;
        (bar.0 - m, bar.1 - self.dock_base() * 7 / 10 - 40 * self.s, bar.0 + bar.2 + m, self.sh)
    }

    fn hit(&self, x: i32, y: i32) -> Hit {
        // windows come first (they are in front of the desktop, below the dock and menu)
        if y < self.menu_h() {
            return Hit::Menubar;
        }
        let (bar, rects) = self.dock_layout(self.dock_mx());
        if within(x, y, bar) || rects.iter().flatten().any(|r| within(x, y, *r)) {
            for (k, r) in rects.iter().enumerate() {
                if let Some(r) = r {
                    if x >= r.0 && x < r.0 + r.2 {
                        return Hit::Dock(k);
                    }
                }
            }
            return Hit::Desktop;
        }
        for w in self.wins.iter().rev() {
            if w.minimized {
                continue;
            }
            let b = 5 * self.s;
            let outer = (w.x - b, w.y - b, w.w + 2 * b, w.h + 2 * b);
            if !within(x, y, outer) {
                continue;
            }
            let rx = x - w.x;
            let ry = y - w.y;
            let resizable = !self.tiling && !w.maximized;
            if resizable {
                let mut edge = 0;
                if rx < b / 2 + 1 {
                    edge |= EDGE_L;
                }
                if rx >= w.w - b {
                    edge |= EDGE_R;
                }
                if ry >= w.h - b {
                    edge |= EDGE_B;
                }
                if ry < 3 * self.s && ry >= -b {
                    edge |= EDGE_T;
                }
                if rx < 0 {
                    edge |= EDGE_L;
                }
                if rx >= w.w {
                    edge |= EDGE_R;
                }
                if ry >= w.h {
                    edge |= EDGE_B;
                }
                if edge != 0 {
                    return Hit::Window(w.id, Part::Edge(edge));
                }
            }
            if rx < 0 || ry < 0 || rx >= w.w || ry >= w.h {
                continue;
            }
            if ry < self.title_h() {
                let r = 6 * self.s;
                let cy = self.title_h() / 2;
                for (k, part) in [Part::Close, Part::Minimize, Part::Maximize].into_iter().enumerate() {
                    let cx = 16 * self.s + k as i32 * 20 * self.s;
                    if (rx - cx) * (rx - cx) + (ry - cy) * (ry - cy) <= (r + 3 * self.s) * (r + 3 * self.s) {
                        return Hit::Window(w.id, part);
                    }
                }
                return Hit::Window(w.id, Part::Title);
            }
            return Hit::Window(w.id, Part::Content);
        }
        Hit::Desktop
    }

    fn dock_mx(&self) -> Option<i32> {
        if self.hover_dock.is_some() || self.in_dock_zone(self.mouse.0, self.mouse.1) {
            Some(self.mouse.0)
        } else {
            None
        }
    }

    fn in_dock_zone(&self, x: i32, y: i32) -> bool {
        let (bar, _) = self.dock_layout(None);
        within(x, y, (bar.0 - 10 * self.s, bar.1 - 10 * self.s, bar.2 + 20 * self.s, bar.3 + 20 * self.s))
    }

    fn menu_rect(&self) -> Xywh {
        let w = 200 * self.s;
        let item_h = 28 * self.s;
        (8 * self.s, self.menu_h(), w, MENU_ITEMS.len() as i32 * item_h + 10 * self.s)
    }

    // ---- the main loop -----------------------------------------------------

    fn main_loop(&mut self) {
        loop {
            self.poll_mouse();
            self.poll_keys();
            self.housekeeping();
            if self.quit.is_some() {
                break;
            }
            let now = crate::time::uptime_ms();
            // windows whose programs drew something need compositing
            for i in 0..self.wins.len() {
                let v = self.wins[i].ctx.surface.lock().version();
                if v != self.wins[i].last_version && !self.wins[i].minimized {
                    self.wins[i].last_version = v;
                    let r = self.content_rect(i);
                    self.damage_rect(r);
                }
            }
            if !self.damage.is_empty() && now >= self.last_compose + 6 {
                self.last_compose = now;
                self.compose();
                sched::note_progress();
            }
            sched::yield_idle();
        }
    }

    fn content_rect(&self, i: usize) -> Rect {
        let w = &self.wins[i];
        (w.x, w.y + self.title_h(), w.x + w.w, w.y + w.h)
    }

    /// Slow things: the clock, programs that ended, settings that changed.
    fn housekeeping(&mut self) {
        let secs = crate::time::uptime_secs();
        if secs != self.last_second {
            self.last_second = secs;
            let c = read_rtc();
            if c != self.clock {
                self.clock = c;
                let r = (self.sw / 2, 0, self.sw, self.menu_h());
                self.damage_rect(r);
            }
            sched::reap();
        }
        // programs that finished
        let mut gone = Vec::new();
        for w in self.wins.iter() {
            if w.done.load(Ordering::SeqCst) {
                gone.push(w.id);
            }
        }
        for id in gone {
            if let Some(i) = self.win_index(id) {
                let r = self.win_damage(&self.wins[i]);
                self.wins.remove(i);
                self.damage_rect(r);
                self.dock_dirty();
                if self.tiling {
                    self.retile();
                }
            }
            self.focus_next_visible();
        }
        if WALLPAPER.load(Ordering::Relaxed) != self.wallpaper_index {
            self.make_wallpaper();
        }
        let tiling = TILING.load(Ordering::Relaxed);
        if tiling != self.tiling {
            self.tiling = tiling;
            if tiling {
                self.retile();
            }
            self.damage_all();
        }
    }

    // ---- mouse -------------------------------------------------------------

    fn poll_mouse(&mut self) {
        let Some((dx, dy, b)) = crate::mouse::take() else { return };
        // a program that captured the mouse gets the raw movement
        if let Some(w) = self.wins.iter().find(|w| w.captured && !w.minimized) {
            let id = w.id;
            let ctx = w.ctx.clone();
            let mut q = ctx.events.lock();
            if q.len() < 256 {
                q.push_back(Event::MouseRaw { dx, dy, buttons: b });
            }
            drop(q);
            self.buttons = b;
            let _ = id;
            return;
        }
        let accel = |d: i32| d + d.signum() * (d * d / 28).min(d.abs() * 2);
        let old = self.mouse;
        let nx = (old.0 + accel(dx)).clamp(0, self.sw - 1);
        let ny = (old.1 - accel(dy)).clamp(0, self.sh - 1);
        self.mouse = (nx, ny);
        let prev = self.buttons;
        self.buttons = b;
        if (nx, ny) != old {
            self.damage_pointer(old);
            self.damage_pointer((nx, ny));
            self.on_move();
        }
        if b & 1 != 0 && prev & 1 == 0 {
            self.on_press(0);
        }
        if b & 1 == 0 && prev & 1 != 0 {
            self.on_release(0);
        }
        if b & 2 != 0 && prev & 2 == 0 {
            self.on_press(1);
        }
        if b & 2 == 0 && prev & 2 != 0 {
            self.on_release(1);
        }
    }

    fn damage_pointer(&mut self, p: (i32, i32)) {
        let (w, h) = draw::pointer_size(self.s);
        self.damage_rect((p.0, p.1, p.0 + w + 1, p.1 + h + 1));
    }

    fn local(&self, id: u32, x: i32, y: i32) -> (i32, i32) {
        match self.win_index(id) {
            Some(i) => (x - self.wins[i].x, y - self.wins[i].y - self.title_h()),
            None => (0, 0),
        }
    }

    fn send(&self, id: u32, ev: Event) {
        if let Some(i) = self.win_index(id) {
            let mut q = self.wins[i].ctx.events.lock();
            if q.len() >= 256 {
                if let Some(Event::MouseMove { .. }) = q.back() {
                    q.pop_back();
                } else {
                    return;
                }
            }
            q.push_back(ev);
        }
    }

    fn on_move(&mut self) {
        let (mx, my) = self.mouse;
        match self.drag {
            Drag::Move { id, ox, oy, ref mut moved } => {
                *moved = true;
                if let Some(i) = self.win_index(id) {
                    if self.wins[i].maximized || self.wins[i].snapped {
                        // dragging a maximized window takes it back to its old size under the pointer
                        let r = self.wins[i].restore;
                        self.wins[i].maximized = false;
                        self.wins[i].snapped = false;
                        let nx = mx - r.2 / 2;
                        self.set_rect(id, (nx, my - 12 * self.s, r.2, r.3));
                        self.drag = Drag::Move { id, ox: r.2 / 2, oy: 12 * self.s, moved: true };
                        return;
                    }
                    if self.tiling {
                        return;
                    }
                    let (w, h) = (self.wins[i].w, self.wins[i].h);
                    let ny = (my - oy).max(self.menu_h());
                    self.set_rect(id, (mx - ox, ny, w, h));
                }
                let target = self.snap_target(mx, my);
                if target != self.snap_preview {
                    if let Some(old) = self.snap_preview {
                        self.damage_rect(to_rect(old));
                    }
                    if let Some(new) = target {
                        self.damage_rect(to_rect(new));
                    }
                    self.snap_preview = target;
                }
            }
            Drag::Resize { id, edge, mouse, start } => {
                let (dx, dy) = (mx - mouse.0, my - mouse.1);
                let (mut x, mut y, mut w, mut h) = start;
                if edge & EDGE_R != 0 {
                    w = start.2 + dx;
                }
                if edge & EDGE_B != 0 {
                    h = start.3 + dy;
                }
                if edge & EDGE_L != 0 {
                    w = start.2 - dx;
                    x = start.0 + dx;
                }
                if edge & EDGE_T != 0 {
                    h = start.3 - dy;
                    y = start.1 + dy;
                }
                let (min_w, min_h) = (200 * self.s, self.title_h() + 100 * self.s);
                if w < min_w {
                    if edge & EDGE_L != 0 {
                        x = start.0 + start.2 - min_w;
                    }
                    w = min_w;
                }
                if h < min_h {
                    if edge & EDGE_T != 0 {
                        y = start.1 + start.3 - min_h;
                    }
                    h = min_h;
                }
                if let Some(i) = self.win_index(id) {
                    self.wins[i].maximized = false;
                    self.wins[i].snapped = false;
                }
                self.set_rect(id, (x, y, w, h));
            }
            Drag::Content { id } => {
                let (lx, ly) = self.local(id, mx, my);
                self.send(id, Event::MouseMove { x: lx, y: ly });
            }
            Drag::None => {
                // hover effects: dock magnification, menu highlight, windows under the pointer
                let hit = self.hit(mx, my);
                let was_dock = self.hover_dock;
                self.hover_dock = if let Hit::Dock(k) = hit { Some(k) } else { None };
                let zone_now = self.in_dock_zone(mx, my);
                if zone_now || was_dock.is_some() {
                    let z = self.dock_zone();
                    self.damage_rect(z);
                }
                if self.menu_open {
                    let r = self.menu_rect();
                    let item_h = 28 * self.s;
                    let hover = if within(mx, my, r) { Some(((my - r.1 - 5 * self.s) / item_h).max(0) as usize) } else { None };
                    if hover != self.menu_hover {
                        self.menu_hover = hover;
                        self.damage_rect(to_rect(r));
                    }
                }
                if let Hit::Window(id, Part::Content) = hit {
                    if self.focused_id() == Some(id) || true {
                        let (lx, ly) = self.local(id, mx, my);
                        self.send(id, Event::MouseMove { x: lx, y: ly });
                    }
                }
            }
        }
    }

    fn on_press(&mut self, button: u8) {
        let (mx, my) = self.mouse;
        let now = crate::time::uptime_ms();
        // the launcher swallows clicks outside of it
        if self.launcher.is_some() {
            self.launcher = None;
            self.damage_all();
            return;
        }
        if self.menu_open {
            let r = self.menu_rect();
            if within(mx, my, r) {
                let item_h = 28 * self.s;
                let k = ((my - r.1 - 5 * self.s) / item_h).max(0) as usize;
                self.menu_open = false;
                self.damage_rect((0, 0, r.0 + r.2 + 20, r.1 + r.3 + 20));
                self.menu_action(k);
                return;
            }
            self.menu_open = false;
            self.damage_rect((0, 0, r.0 + r.2 + 20, r.1 + r.3 + 20));
            if matches!(self.hit(mx, my), Hit::Menubar) {
                return;
            }
        }
        match self.hit(mx, my) {
            Hit::Menubar => {
                if button == 0 && mx < 90 * self.s {
                    self.menu_open = true;
                    self.menu_hover = None;
                    let r = self.menu_rect();
                    self.damage_rect(to_rect(r));
                }
            }
            Hit::Dock(k) => {
                if button == 0 {
                    if let Some(Some(info)) = Self::dock_items().get(k) {
                        let app = info.app;
                        self.launch_or_toggle(app);
                    }
                }
            }
            Hit::Window(id, part) => {
                let focused = self.focused_id() == Some(id);
                if !focused {
                    self.focus(id);
                }
                match part {
                    Part::Close => self.close(id),
                    Part::Minimize => self.minimize(id),
                    Part::Maximize => self.toggle_maximize(id),
                    Part::Title => {
                        if button == 0 {
                            let double = now < self.last_click.0 + 400 && self.last_click.1 == id;
                            self.last_click = (now, id);
                            if double {
                                self.toggle_maximize(id);
                            } else if let Some(i) = self.win_index(id) {
                                let (ox, oy) = (mx - self.wins[i].x, my - self.wins[i].y);
                                self.drag = Drag::Move { id, ox, oy, moved: false };
                            }
                        }
                    }
                    Part::Edge(edge) => {
                        if let Some(i) = self.win_index(id) {
                            let start = Self::win_rect(&self.wins[i]);
                            self.drag = Drag::Resize { id, edge, mouse: (mx, my), start };
                        }
                    }
                    Part::Content => {
                        // a program that wants the mouse (Doom) captures it on the first click
                        if let Some(i) = self.win_index(id) {
                            if self.wins[i].ctx.raw_mouse.load(Ordering::Relaxed) && !self.wins[i].captured && button == 0 {
                                self.wins[i].captured = true;
                                self.damage_pointer(self.mouse);
                                return;
                            }
                        }
                        let (lx, ly) = self.local(id, mx, my);
                        self.send(id, Event::MouseDown { x: lx, y: ly, button });
                        if button == 0 {
                            self.drag = Drag::Content { id };
                        }
                    }
                }
            }
            Hit::Desktop => {}
        }
    }

    fn on_release(&mut self, button: u8) {
        let (mx, my) = self.mouse;
        match self.drag {
            Drag::Move { id, moved, .. } => {
                if moved && button == 0 {
                    if let Some(target) = self.snap_preview.take() {
                        self.damage_rect(to_rect(target));
                        let work = self.work_area();
                        if target == work {
                            self.snap(id, work);
                            if let Some(i) = self.win_index(id) {
                                self.wins[i].maximized = true;
                                self.wins[i].snapped = false;
                            }
                        } else {
                            self.snap(id, target);
                        }
                    }
                }
                self.drag = Drag::None;
            }
            Drag::Resize { .. } => self.drag = Drag::None,
            Drag::Content { id } => {
                let (lx, ly) = self.local(id, mx, my);
                self.send(id, Event::MouseUp { x: lx, y: ly, button });
                self.drag = Drag::None;
            }
            Drag::None => {
                if let Hit::Window(id, Part::Content) = self.hit(mx, my) {
                    let (lx, ly) = self.local(id, mx, my);
                    self.send(id, Event::MouseUp { x: lx, y: ly, button });
                }
            }
        }
        if let Some(t) = self.snap_preview.take() {
            self.damage_rect(to_rect(t));
        }
    }

    fn launch_or_toggle(&mut self, app: App) {
        let existing = self.wins.iter().position(|w| w.app == app && !w.ctx.is_closed());
        if let Some(i) = existing {
            let id = self.wins[i].id;
            if self.wins[i].minimized {
                self.restore(id);
                self.focus(id);
            } else if self.focused_id() == Some(id) {
                self.minimize(id);
            } else {
                self.focus(id);
            }
            return;
        }
        self.open(app);
    }

    fn menu_action(&mut self, k: usize) {
        match k {
            0 | 1 => self.open(App::Settings),
            2 => self.open(App::Terminal),
            3 => self.quit = Some(true),
            4 => self.quit = Some(false),
            _ => {}
        }
        if let Some(restart) = self.quit {
            // nothing to save: the desktop keeps no state of its own
            if restart {
                crate::power::reboot();
            } else {
                crate::power::shutdown();
            }
        }
    }

    // ---- keyboard ----------------------------------------------------------

    fn poll_keys(&mut self) {
        // releasing the mouse from a program that captured it
        if input::held(KeyCode::LWin) || input::held(KeyCode::RWin) {
            let mut released = false;
            for w in self.wins.iter_mut() {
                if w.captured {
                    w.captured = false;
                    released = true;
                }
            }
            if released {
                let p = self.mouse;
                self.damage_rect((p.0, p.1, p.0 + 40, p.1 + 40));
            }
        }
        while let Some(key) = input::pop_raw_key() {
            crate::sched::note_progress();
            let sup = input::held(KeyCode::LWin) || input::held(KeyCode::RWin);
            let alt = input::held(KeyCode::LAlt);
            let shift = input::held(KeyCode::LShift) || input::held(KeyCode::RShift);
            if self.launcher.is_some() {
                self.launcher_key(key);
                continue;
            }
            if sup || alt {
                if self.shortcut(key, sup, shift) {
                    continue;
                }
            }
            if let Some(id) = self.focused_id() {
                self.send(id, Event::Key(key));
            }
        }
    }

    /// Desktop shortcuts. Returns true if the key was one.
    fn shortcut(&mut self, key: Key, sup: bool, shift: bool) -> bool {
        let focused = self.focused_id();
        let work = self.work_area();
        match key {
            Key::Char('\t') if !shift => {
                // cycle windows: the one at the back comes to the front
                if let Some(id) = self.wins.iter().find(|w| !w.minimized).map(|w| w.id) {
                    self.focus(id);
                }
                true
            }
            Key::Char(' ') if sup => {
                self.launcher = Some(Launcher { query: String::new(), selected: 0 });
                self.damage_all();
                true
            }
            Key::Char('\n') if sup => {
                self.open(App::Terminal);
                true
            }
            Key::Char('q') | Key::Char('Q') if sup => {
                if let Some(id) = focused {
                    self.close(id);
                }
                true
            }
            Key::Char('f') | Key::Char('F') if sup => {
                if let Some(id) = focused {
                    self.toggle_maximize(id);
                }
                true
            }
            Key::Char('m') | Key::Char('M') if sup => {
                if let Some(id) = focused {
                    self.minimize(id);
                }
                true
            }
            Key::Char('t') | Key::Char('T') if sup => {
                let on = !TILING.load(Ordering::Relaxed);
                TILING.store(on, Ordering::Relaxed);
                crate::settings::set("tiling", if on { "1" } else { "0" });
                true
            }
            Key::Char(c) if sup && c.is_ascii_digit() && c != '0' => {
                let n = c as usize - '1' as usize;
                let infos: Vec<&'static AppInfo> = apps::DOCK.iter().flatten().collect();
                if let Some(info) = infos.get(n) {
                    self.launch_or_toggle(info.app);
                }
                true
            }
            Key::Left if sup => {
                if let Some(id) = focused {
                    self.snap(id, (work.0, work.1, work.2 / 2, work.3));
                }
                true
            }
            Key::Right if sup => {
                if let Some(id) = focused {
                    self.snap(id, (work.0 + work.2 / 2, work.1, work.2 - work.2 / 2, work.3));
                }
                true
            }
            Key::Up if sup => {
                if let Some(id) = focused {
                    if let Some(i) = self.win_index(id) {
                        if !self.wins[i].maximized {
                            self.toggle_maximize(id);
                        }
                    }
                }
                true
            }
            Key::Down if sup => {
                if let Some(id) = focused {
                    if let Some(i) = self.win_index(id) {
                        if self.wins[i].maximized || self.wins[i].snapped {
                            self.toggle_maximize(id);
                        } else {
                            self.minimize(id);
                        }
                    }
                }
                true
            }
            _ => false,
        }
    }

    fn launcher_matches(&self) -> Vec<&'static AppInfo> {
        let q = self.launcher.as_ref().map(|l| l.query.to_ascii_lowercase()).unwrap_or_default();
        apps::DOCK.iter().flatten().filter(|i| q.is_empty() || i.name.to_ascii_lowercase().contains(&q)).collect()
    }

    fn launcher_key(&mut self, key: Key) {
        let matches = self.launcher_matches();
        let Some(l) = self.launcher.as_mut() else { return };
        match key {
            Key::Char('\u{1b}') => self.launcher = None,
            Key::Char('\n') => {
                let pick = matches.get(l.selected).map(|i| i.app);
                self.launcher = None;
                if let Some(app) = pick {
                    self.launch_or_toggle(app);
                }
            }
            Key::Char('\u{8}') => {
                l.query.pop();
                l.selected = 0;
            }
            Key::Up => l.selected = l.selected.saturating_sub(1),
            Key::Down => l.selected = (l.selected + 1).min(matches.len().saturating_sub(1)),
            Key::Char(c) if !c.is_control() => {
                l.query.push(c);
                l.selected = 0;
            }
            _ => {}
        }
        self.damage_all();
    }

    // ---- painting ----------------------------------------------------------

    fn compose(&mut self) {
        let rects = core::mem::take(&mut self.damage);
        let started = crate::time::uptime_ms();
        let this = &*self;
        framebuffer::with_screen(|fb| {
            for d in &rects {
                let clip = (d.0.max(0), d.1.max(0), d.2.min(this.sw), d.3.min(this.sh));
                if clip.2 > clip.0 && clip.3 > clip.1 {
                    this.paint(fb, clip);
                }
            }
            fb.flush();
        });
        let took = crate::time::uptime_ms() - started;
        self.compose_ms = (self.compose_ms * 7 + took as u32 * 100) / 8;
    }

    fn paint(&self, fb: &mut Framebuffer, clip: Rect) {
        let pitch = fb.pitch();
        fb.blit_bytes(&self.wallpaper, pitch, clip.0 as usize, clip.1 as usize, (clip.2 - clip.0) as usize, (clip.3 - clip.1) as usize, clip.0, clip.1, clip);
        let focused = self.focused_id();
        for w in self.wins.iter() {
            if w.minimized {
                continue;
            }
            if intersects(self.win_damage(w), clip) {
                self.paint_window(fb, w, focused == Some(w.id), clip);
            }
        }
        if let Some(t) = self.snap_preview {
            if intersects(to_rect(t), clip) {
                let g = 6 * self.s;
                let r = (t.0 + g, t.1 + g, t.2 - 2 * g, t.3 - 2 * g);
                fb.tint_rect(r.0, r.1, r.2, r.3, accent(), 60, clip);
                fb.tint_rect(r.0, r.1, r.2, 2, accent(), 200, clip);
                fb.tint_rect(r.0, r.1 + r.3 - 2, r.2, 2, accent(), 200, clip);
                fb.tint_rect(r.0, r.1, 2, r.3, accent(), 200, clip);
                fb.tint_rect(r.0 + r.2 - 2, r.1, 2, r.3, accent(), 200, clip);
            }
        }
        if intersects(self.dock_zone(), clip) {
            self.paint_dock(fb, clip);
        }
        if clip.1 < self.menu_h() || self.menu_open {
            self.paint_menubar(fb, clip);
        }
        if self.launcher.is_some() {
            self.paint_launcher(fb, clip);
        }
        let captured = self.wins.iter().any(|w| w.captured && !w.minimized);
        if !captured {
            draw::draw_pointer(fb, self.mouse.0, self.mouse.1, self.s);
        }
    }

    fn paint_window(&self, fb: &mut Framebuffer, w: &Win, focused: bool, clip: Rect) {
        let s = self.s;
        let r = self.radius();
        // shadow: a few translucent layers, offset downwards. Only the part outside
        // the window is drawn (the window covers the rest).
        for (grow, alpha) in [(14, 10u32), (9, 14), (5, 20), (2, 26)] {
            let g = grow * s;
            let (rx, ry, rw, rh) = (w.x - g + 2, w.y - g / 2 + 6 * s, w.w + 2 * g - 4, w.h + g + g / 2);
            let (wx0, wy0, wx1, wy1) = (w.x, w.y + r, w.x + w.w, w.y + w.h - r);
            // above, below, left and right of the window (extended under its rounded corners)
            fb.tint_rect(rx, ry, rw, (wy0 - ry).max(0), 0x000000, alpha, clip);
            fb.tint_rect(rx, wy1, rw, (ry + rh - wy1).max(0), 0x000000, alpha, clip);
            fb.tint_rect(rx, wy0, (wx0 - rx).max(0), wy1 - wy0, 0x000000, alpha, clip);
            fb.tint_rect(wx1, wy0, (rx + rw - wx1).max(0), wy1 - wy0, 0x000000, alpha, clip);
        }
        // the title bar (rounded at the top)
        let bar = if focused { 0x353B46 } else { 0x2B3039 };
        fb.fill_rounded(w.x, w.y, w.w, self.title_h() + r, r, bar, clip);
        // the program's own picture
        {
            let surface = w.ctx.surface.lock();
            fb.blit_rounded(surface.pixels(), surface.pitch(), surface.width(), surface.height(), w.x, w.y + self.title_h(), r as usize, false, clip);
        }
        // a hairline frame, in the accent color for the window in front
        let line = if focused { accent() } else { BORDER };
        let a = if focused { 140 } else { 255 };
        fb.tint_rect(w.x + r, w.y, w.w - 2 * r, 1, line, a, clip);
        fb.tint_rect(w.x, w.y + r, 1, w.h - 2 * r, line, a, clip);
        fb.tint_rect(w.x + w.w - 1, w.y + r, 1, w.h - 2 * r, line, a, clip);
        fb.tint_rect(w.x + r, w.y + w.h - 1, w.w - 2 * r, 1, line, a, clip);
        // traffic lights
        let cy = w.y + self.title_h() / 2;
        let colors = if focused { [0xFF5F57, 0xFEBC2E, 0x28C840] } else { [0x4B5160, 0x4B5160, 0x4B5160] };
        for (k, c) in colors.iter().enumerate() {
            let cx = w.x + 16 * s + k as i32 * 20 * s;
            draw_disc_clipped(fb, cx, cy, 6 * s, *c, clip);
        }
        // the title
        let font = &self.f_menu_bold;
        let tw = font.text_width(&w.title) as i32;
        let tx = w.x + (w.w - tw) / 2;
        let ty = w.y + (self.title_h() - font.height() as i32) / 2;
        if intersects((tx, ty, tx + tw, ty + font.height() as i32), clip) {
            let color = if focused { TEXT } else { TEXT_FAINT };
            fb.set_clip_rect(clip);
            draw::text(fb, font, tx, ty, &w.title, color);
            fb.clear_clip();
        }
        if w.captured {
            let msg = "mouse captured: press the Super key to release";
            let f = &self.f_small;
            let mw = f.text_width(msg) as i32 + 16 * s;
            let mx = w.x + (w.w - mw) / 2;
            let my = w.y + self.title_h() + 8 * s;
            fb.fill_rounded(mx, my, mw, f.height() as i32 + 8 * s, 6 * s, 0x14171C, clip);
            fb.set_clip_rect(clip);
            draw::text(fb, f, mx + 8 * s, my + 4 * s, msg, TEXT);
            fb.clear_clip();
        }
    }

    fn paint_dock(&self, fb: &mut Framebuffer, clip: Rect) {
        let s = self.s;
        let (bar, rects) = self.dock_layout(self.dock_mx());
        // the base bar keeps its normal height; icons grow upwards out of it
        fb.fill_rounded(bar.0 - 1, bar.1 - 1, bar.2 + 2, bar.3 + 2, 18 * s, 0x4A5060, clip);
        fb.fill_rounded(bar.0, bar.1, bar.2, bar.3, 18 * s, 0x1F232B, clip);
        let items = Self::dock_items();
        for (k, r) in rects.iter().enumerate() {
            match (r, items[k]) {
                (Some(r), Some(info)) => {
                    draw::icon(fb, info.icon, r.0, r.1, r.2, clip);
                    // a dot under apps that are running
                    let running = self.wins.iter().any(|w| w.app == info.app && !w.ctx.is_closed());
                    if running {
                        let cx = r.0 + r.2 / 2;
                        let cy = bar.1 + bar.3 - 5 * s;
                        draw_disc_clipped(fb, cx, cy, 2 * s, TEXT_MUTED, clip);
                    }
                }
                (None, None) => {
                    let sx = self.sep_x(&rects, k);
                    fb.tint_rect(sx, bar.1 + 12 * s, 1, bar.3 - 24 * s, 0x5A6070, 255, clip);
                }
                _ => {}
            }
        }
        // the name of the app under the pointer
        if let Some(k) = self.hover_dock {
            if let (Some(Some(r)), Some(info)) = (rects.get(k), items.get(k).copied().flatten()) {
                let f = &self.f_menu;
                let tw = f.text_width(info.name) as i32 + 16 * s;
                let th = f.height() as i32 + 8 * s;
                let tx = (r.0 + r.2 / 2 - tw / 2).clamp(4, self.sw - tw - 4);
                let ty = r.1 - th - 8 * s;
                fb.fill_rounded(tx, ty, tw, th, 6 * s, 0x14171C, clip);
                fb.set_clip_rect(clip);
                draw::text(fb, f, tx + 8 * s, ty + 4 * s, info.name, TEXT);
                fb.clear_clip();
            }
        }
    }

    fn sep_x(&self, rects: &[Option<Xywh>], k: usize) -> i32 {
        let prev = rects[..k].iter().rev().flatten().next();
        match prev {
            Some(p) => p.0 + p.2 + 4 * self.s + 4 * self.s,
            None => 0,
        }
    }

    fn paint_menubar(&self, fb: &mut Framebuffer, clip: Rect) {
        let s = self.s;
        let h = self.menu_h();
        let area = (0, 0, self.sw, h);
        if intersects(area, clip) {
            fb.tint_rect(0, 0, self.sw, h, 0x14171C, 215, clip);
            fb.tint_rect(0, h - 1, self.sw, 1, 0x000000, 80, clip);
            fb.set_clip_rect(clip);
            let f = &self.f_menu_bold;
            let ty = (h - f.height() as i32) / 2;
            // the logo and the name of the program in front
            draw::text(fb, f, 12 * s, ty, "zero", accent());
            let name = self
                .wins
                .iter()
                .rev()
                .find(|w| !w.minimized)
                .map(|w| w.title.as_str())
                .unwrap_or("Desktop");
            draw::text(fb, f, 70 * s, ty, name, TEXT);
            // right side: clock
            let (hh, mm, _ss, day, month) = self.clock;
            let months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
            let mname = months.get((month as usize).wrapping_sub(1)).copied().unwrap_or("");
            let clock = alloc::format!("{} {}   {:02}:{:02}", mname, day, hh, mm);
            let cw = self.f_menu.text_width(&clock) as i32;
            draw::text(fb, &self.f_menu, self.sw - cw - 14 * s, ty, &clock, TEXT);
            fb.clear_clip();
        }
        if self.menu_open {
            let r = self.menu_rect();
            fb.fill_rounded(r.0 - 1, r.1 - 1, r.2 + 2, r.3 + 2, 8 * s, 0x4A5060, clip);
            fb.fill_rounded(r.0, r.1, r.2, r.3, 8 * s, 0x242830, clip);
            fb.set_clip_rect(clip);
            let item_h = 28 * s;
            for (k, label) in MENU_ITEMS.iter().enumerate() {
                let y = r.1 + 5 * s + k as i32 * item_h;
                if self.menu_hover == Some(k) {
                    fb.fill_rounded(r.0 + 4 * s, y, r.2 - 8 * s, item_h, 5 * s, accent(), clip);
                    draw::text(fb, &self.f_menu, r.0 + 14 * s, y + (item_h - self.f_menu.height() as i32) / 2, label, 0x14171C);
                } else {
                    draw::text(fb, &self.f_menu, r.0 + 14 * s, y + (item_h - self.f_menu.height() as i32) / 2, label, TEXT);
                }
            }
            fb.clear_clip();
        }
    }

    fn paint_launcher(&self, fb: &mut Framebuffer, clip: Rect) {
        let s = self.s;
        let Some(l) = self.launcher.as_ref() else { return };
        let matches = self.launcher_matches();
        let w = 460 * s;
        let row = 34 * s;
        let h = 56 * s + matches.len().min(8) as i32 * row + 10 * s;
        let x = (self.sw - w) / 2;
        let y = self.sh / 5;
        fb.tint_rect(0, 0, self.sw, self.sh, 0x000000, 70, clip);
        fb.fill_rounded(x - 1, y - 1, w + 2, h + 2, 12 * s, 0x4A5060, clip);
        fb.fill_rounded(x, y, w, h, 12 * s, 0x242830, clip);
        fb.set_clip_rect(clip);
        let f = draw::font(18 * s as usize, false);
        let query = if l.query.is_empty() { "Search apps..." } else { l.query.as_str() };
        let color = if l.query.is_empty() { TEXT_FAINT } else { TEXT };
        draw::text(fb, &f, x + 18 * s, y + 14 * s, query, color);
        if !l.query.is_empty() {
            let cx = x + 18 * s + f.text_width(&l.query) as i32;
            draw::rect(fb, cx, y + 14 * s, 2 * s, f.height() as i32, accent());
        }
        for (k, info) in matches.iter().take(8).enumerate() {
            let ry = y + 56 * s + k as i32 * row;
            if k == l.selected {
                fb.fill_rounded(x + 8 * s, ry, w - 16 * s, row - 2 * s, 8 * s, accent(), clip);
            }
            draw::icon(fb, info.icon, x + 16 * s, ry + 3 * s, row - 8 * s, clip);
            let c = if k == l.selected { 0x14171C } else { TEXT };
            draw::text(fb, &self.f_menu, x + 16 * s + row, ry + (row - self.f_menu.height() as i32) / 2 - s, info.name, c);
        }
        fb.clear_clip();
    }
}

fn draw_disc_clipped(fb: &mut Framebuffer, cx: i32, cy: i32, r: i32, color: u32, clip: Rect) {
    for dy in -r..=r {
        let mut span = 0;
        while (span + 1) * (span + 1) + dy * dy <= r * r {
            span += 1;
        }
        fb.tint_rect(cx - span, cy + dy, span * 2 + 1, 1, color, 255, clip);
    }
}

/// Channel order the shadow buffer expects for a 0xRRGGBB color, assuming BGR.
fn pack_color(c: u32, bpp: usize) -> [u8; 4] {
    let _ = bpp;
    [c as u8, (c >> 8) as u8, (c >> 16) as u8, 0]
}

/// (hour, minute, second, day, month) from the CMOS clock.
fn read_rtc() -> (u8, u8, u8, u8, u8) {
    use x86_64::instructions::port::Port;
    let mut index: Port<u8> = Port::new(0x70);
    let mut data: Port<u8> = Port::new(0x71);
    let mut read = |reg: u8| unsafe {
        index.write(reg);
        data.read()
    };
    // wait out an update in progress
    for _ in 0..1000 {
        if read(0x0A) & 0x80 == 0 {
            break;
        }
    }
    let (s, m, h, d, mo, b) = (read(0x00), read(0x02), read(0x04), read(0x07), read(0x08), read(0x0B));
    let bcd = b & 4 == 0;
    let conv = |v: u8| if bcd { (v & 0x0F) + (v >> 4) * 10 } else { v };
    let pm = h & 0x80 != 0;
    let mut hour = conv(h & 0x7F);
    if b & 2 == 0 && pm {
        hour = (hour + 12) % 24;
    }
    (hour, conv(m), conv(s), conv(d), conv(mo))
}
