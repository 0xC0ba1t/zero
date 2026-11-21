//! The window manager and compositor: the desktop thread.

use super::apps::{self, AppId};
use super::gfx::{self, ease_back, ease_in_out, ease_out, lerp, Canvas, Rect};
use super::icons::{self, Icon};
use super::theme::{self, Theme};
use super::ui::{self, MenuItem, ALT, CTRL, SHIFT, SUPER};
use super::{Cursor, Event, Request};
use crate::fs::FILESYSTEM;
use crate::graphics::display::Display;
use crate::graphics::font::Font;
use crate::graphics::framebuffer::{self, Framebuffer};
use crate::input::{self, Key};
use crate::sched::{self, Context};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use spin::Mutex;

// ---- settings other apps change ------------------------------------------------

pub static WALLPAPER: AtomicUsize = AtomicUsize::new(0);
pub static WALL_IMAGE: Mutex<Option<String>> = Mutex::new(None);
pub static WALL_GEN: AtomicU32 = AtomicU32::new(0);
pub static TILING: AtomicBool = AtomicBool::new(false);
pub static MAGNIFY: AtomicBool = AtomicBool::new(true);
pub static DOCK_SIZE: AtomicU32 = AtomicU32::new(50);
pub static SHOW_BUDDY: AtomicBool = AtomicBool::new(true);
pub static CLOCK_24H: AtomicBool = AtomicBool::new(true);
pub static DND: AtomicBool = AtomicBool::new(false);
pub static FPS: AtomicU32 = AtomicU32::new(0);
pub static FRAME_US: AtomicU32 = AtomicU32::new(0);

pub const WALLPAPERS: [&str; 6] = ["Sequoia", "Ocean", "Forest", "Graphite", "Sunrise", "Aurora"];

pub fn load_settings() {
    let get = |k: &str| crate::settings::get(k);
    if let Some(v) = get("wallpaper") {
        WALLPAPER.store(v.parse().unwrap_or(0).min(WALLPAPERS.len() - 1), Ordering::Relaxed);
    }
    if let Some(v) = get("wall_image") {
        if !v.is_empty() {
            *WALL_IMAGE.lock() = Some(v);
        }
    }
    let flag = |k: &str, f: &AtomicBool| {
        if let Some(v) = get(k) {
            f.store(v == "1", Ordering::Relaxed);
        }
    };
    flag("tiling", &TILING);
    flag("magnify", &MAGNIFY);
    flag("show_buddy", &SHOW_BUDDY);
    flag("clock24", &CLOCK_24H);
    if let Some(v) = get("dock_size") {
        DOCK_SIZE.store(v.parse().unwrap_or(50).clamp(32, 80), Ordering::Relaxed);
    }
    if let Some(v) = get("mouse_speed") {
        crate::mouse::SPEED.store(v.parse().unwrap_or(35).clamp(10, 200), Ordering::Relaxed);
    }
    theme::load();
}

pub fn save_flag(key: &str, on: bool) {
    crate::settings::set(key, if on { "1" } else { "0" });
}

const MENU_H: i32 = 28;
const TITLE_H: i32 = 32;
const RADIUS: i32 = 11;

type Bx = (i32, i32, i32, i32); // x0 y0 x1 y1

fn bx(r: Rect) -> Bx {
    (r.0, r.1, r.0 + r.2, r.1 + r.3)
}

fn union(a: Bx, b: Bx) -> Bx {
    (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3))
}

fn meets(a: Bx, b: Bx) -> bool {
    a.0 < b.2 && b.0 < a.2 && a.1 < b.3 && b.1 < a.3
}

fn within(p: (i32, i32), r: Rect) -> bool {
    ui::inside(p, r)
}

fn lerp_rect(a: Rect, b: Rect, t: i32) -> Rect {
    (lerp(a.0, b.0, t), lerp(a.1, b.1, t), lerp(a.2, b.2, t), lerp(a.3, b.3, t))
}

// ---- windows ----------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum AnimKind {
    Open,
    Close,
    Minimize,
    Restore,
    Zoom,
}

#[derive(Clone, Copy)]
struct Anim {
    kind: AnimKind,
    start: u64,
    dur: u64,
    from: Rect,
    to: Rect,
}

struct Win {
    id: u32,
    app: AppId,
    ctx: Arc<Context>,
    done: Arc<AtomicBool>,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    minimized: bool,
    maximized: bool,
    snapped: bool,
    restore: Rect,
    last_version: u64,
    keep_alive: bool,
    captured: bool,
    anim: Option<Anim>,
    /// A picture of the window, for animations that change its size.
    snap: Option<(Vec<u32>, usize, usize)>,
    dying: bool,
    title: String,
}

impl Win {
    fn rect(&self) -> Rect {
        (self.x, self.y, self.w, self.h)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Part {
    Close,
    Minimize,
    Zoom,
    Title,
    Edge(u8),
    Content,
}

#[derive(Clone, Copy)]
enum Drag {
    None,
    Move { id: u32, ox: i32, oy: i32, moved: bool },
    Resize { id: u32, edge: u8, mouse: (i32, i32), start: Rect },
    Content { id: u32, button: u8 },
    Icon { index: usize, start: (i32, i32), moved: bool },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MenuKind {
    Logo,
    App,
    AppDefined(usize),
    Window,
    Desktop,
    Icon(usize),
    Dock(AppId),
    Trash,
}

struct PopMenu {
    kind: MenuKind,
    x: i32,
    y: i32,
    items: Vec<MenuItem>,
    hover: Option<usize>,
    opened: u64,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum Panel {
    None,
    Control,
    Notifications,
}

struct Spotlight {
    query: String,
    selected: usize,
    opened: u64,
}

#[derive(Clone)]
struct Hit {
    title: String,
    sub: String,
    icon: Icon,
    action: Action,
}

#[derive(Clone)]
enum Action {
    App(AppId),
    File(String),
    Settings,
    AskBuddy(String),
    Copy(String),
}

struct Notif {
    title: String,
    body: String,
    at: u64,
}

struct DeskIcon {
    name: String,
    is_dir: bool,
    pos: (i32, i32),
}

pub struct Desktop {
    sw: i32,
    sh: i32,
    scr: Vec<u32>,
    wall: Vec<u32>,
    wall_key: (usize, u32, bool),
    wins: Vec<Win>,
    next_id: u32,
    mouse: (i32, i32),
    buttons: u8,
    drag: Drag,
    damage: Vec<Bx>,
    dock_scale: Vec<i32>,
    dock_hover: Option<usize>,
    dock_bounce: Vec<(AppId, u64)>,
    dock_track: bool,
    menu: Option<PopMenu>,
    panel: Panel,
    panel_at: u64,
    spot: Option<Spotlight>,
    launchpad: Option<(u64, String)>,
    lp_blur: Vec<u32>,
    mission: Option<(u64, bool)>,
    switcher: Option<(usize, u64)>,
    notifs: Vec<Notif>,
    history: Vec<Notif>,
    snap_preview: Option<Rect>,
    last_click: (u64, i32, i32, u8),
    cursor: Cursor,
    tiling: bool,
    icons: Vec<DeskIcon>,
    icon_sel: Option<usize>,
    icons_checked: u64,
    clock: (u8, u8, u8, u8, u8, u16),
    last_second: u64,
    last_frame: u64,
    frames: u32,
    fps_at: u64,
    pub buddy: super::buddy_desk::DeskBuddy,
    hover_win: Option<u32>,
    volume_drag: bool,
    /// The time this frame is drawn at (the same for every processor).
    frame_now: u64,
    /// Whether the trash has something in it (checked once a second, not on every paint).
    trash_full: bool,
    th: Theme,
    theme_gen: u32,
    f: Font,
    fb: Font,
    fbig: Font,
}

pub fn run() {
    // very large screens: draw at half size (crisp 2x scaling by the display layer)
    let wanted = crate::settings::get_or("desk_res", "");
    framebuffer::with_screen(|fb| {
        let (pw, ph) = fb.physical();
        let parsed = wanted.split_once('x').and_then(|(a, b)| Some((a.parse::<usize>().ok()?, b.parse::<usize>().ok()?)));
        match parsed {
            Some((w, h)) if w <= pw && h <= ph => {
                fb.set_logical(w, h);
            }
            _ => {
                if pw >= 2000 && fb.width() == pw {
                    fb.set_logical(pw / 2, ph / 2);
                }
            }
        }
    });
    let Some((sw, sh)) = framebuffer::with_screen(|fb| (fb.width() as i32, fb.height() as i32)) else { return };
    load_settings();
    crate::graphics::ui::GUI.store(true, Ordering::SeqCst);
    let mouse = crate::mouse::enable();
    crate::mouse::keep_enabled(true);
    crate::serial_println!("[gui] mouse: {}", if mouse { "yes" } else { "no" });
    let cpus = crate::smp::start();
    crate::serial_println!("[smp] {} processor(s)", cpus);
    crate::net::start();
    crate::audio::start();
    super::buddy_desk::init();
    input::clear_gui_keys();
    let mut d = Desktop::new(sw, sh);
    sched::enable_preemption(true);
    // first time: the welcome setup
    let first_run = crate::settings::get("desk_setup").map(|v| v.is_empty()).unwrap_or(true);
    if first_run {
        super::onboard::run(sw, sh);
        d.th = theme::current();
        d.theme_gen = theme::GENERATION.load(Ordering::Relaxed);
        d.make_wallpaper();
        d.damage_all();
    }
    if !first_run && crate::settings::has_password() {
        super::onboard::lock_screen(sw, sh);
        d.damage_all();
    }
    d.ensure_folders();
    super::buddy_desk::greet();
    d.main_loop();
}

impl Desktop {
    fn new(sw: i32, sh: i32) -> Desktop {
        let mut d = Desktop {
            sw,
            sh,
            scr: alloc::vec![0; (sw * sh) as usize],
            wall: Vec::new(),
            wall_key: (usize::MAX, u32::MAX, false),
            wins: Vec::new(),
            next_id: 1,
            mouse: (sw / 2, sh / 2),
            buttons: 0,
            drag: Drag::None,
            damage: Vec::new(),
            dock_scale: Vec::new(),
            dock_hover: None,
            dock_bounce: Vec::new(),
            dock_track: false,
            menu: None,
            panel: Panel::None,
            panel_at: 0,
            spot: None,
            launchpad: None,
            lp_blur: Vec::new(),
            mission: None,
            switcher: None,
            notifs: Vec::new(),
            history: Vec::new(),
            snap_preview: None,
            last_click: (0, 0, 0, 0),
            cursor: Cursor::Arrow,
            tiling: TILING.load(Ordering::Relaxed),
            icons: Vec::new(),
            icon_sel: None,
            icons_checked: 0,
            clock: (0, 0, 0, 0, 0, 0),
            last_second: u64::MAX,
            last_frame: 0,
            frames: 0,
            fps_at: 0,
            buddy: super::buddy_desk::DeskBuddy::new(),
            hover_win: None,
            volume_drag: false,
            frame_now: 0,
            trash_full: false,
            th: theme::current(),
            theme_gen: theme::GENERATION.load(Ordering::Relaxed),
            f: gfx::font(16, false),
            fb: gfx::font(16, true),
            fbig: gfx::font(24, true),
        };
        d.make_wallpaper();
        d.damage_all();
        d
    }

    fn ensure_folders(&mut self) {
        {
            let mut fs = FILESYSTEM.lock();
            if let Some(fs) = fs.as_mut() {
                for dir in ["/Desktop", "/Documents", "/Pictures", "/Music", "/Movies", "/Downloads", "/Trash", "/Apps"] {
                    if !fs.is_dir(dir) {
                        let _ = fs.create_dir(dir);
                    }
                }
            }
        }
        super::media::install_samples();
        crate::pfs::sync_if_dirty();
        self.reload_icons();
    }

    // ---- damage --------------------------------------------------------------

    fn damage_rect(&mut self, r: Bx) {
        let mut r = (r.0.max(0), r.1.max(0), r.2.min(self.sw), r.3.min(self.sh));
        if r.2 <= r.0 || r.3 <= r.1 {
            return;
        }
        let area = |q: Bx| ((q.2 - q.0) as i64) * ((q.3 - q.1) as i64);
        let mut i = 0;
        while i < self.damage.len() {
            let d = self.damage[i];
            let u = union(d, r);
            if meets(d, r) || area(u) <= (area(d) + area(r)) * 5 / 4 {
                r = u;
                self.damage.swap_remove(i);
                i = 0;
            } else {
                i += 1;
            }
        }
        self.damage.push(r);
        if self.damage.len() > 8 {
            let all = self.damage.iter().fold(self.damage[0], |a, q| union(a, *q));
            self.damage.clear();
            self.damage.push(all);
        }
    }

    fn damage_all(&mut self) {
        self.damage.clear();
        self.damage.push((0, 0, self.sw, self.sh));
    }

    fn damage_win(&mut self, r: Rect) {
        let m = 40;
        self.damage_rect((r.0 - m, r.1 - m / 2, r.0 + r.2 + m, r.1 + r.3 + m + 10));
    }

    fn damage_dock(&mut self) {
        let z = self.dock_zone();
        self.damage_rect(z);
    }

    // ---- wallpaper -------------------------------------------------------------

    fn make_wallpaper(&mut self) {
        let index = WALLPAPER.load(Ordering::Relaxed);
        let gen = WALL_GEN.load(Ordering::Relaxed);
        let dark = theme::is_dark();
        if self.wall_key == (index, gen, dark) && !self.wall.is_empty() {
            return;
        }
        self.wall_key = (index, gen, dark);
        let (sw, sh) = (self.sw, self.sh);
        let mut data = alloc::vec![0u32; (sw * sh) as usize];
        let image = WALL_IMAGE.lock().clone();
        if let Some(path) = image {
            if let Some(img) = super::media::load_image(&path) {
                let mut cv = Canvas::new(&mut data, sw as usize, sh as usize);
                let (iw, ih) = (img.w as i64, img.h as i64);
                let scale = ((sw as i64 * 1000 / iw.max(1)).max(sh as i64 * 1000 / ih.max(1))) as i32;
                let (dw, dh) = ((iw * scale as i64 / 1000) as i32, (ih * scale as i64 / 1000) as i32);
                cv.blit_scaled(((sw - dw) / 2, (sh - dh) / 2, dw, dh), &img.px, img.w, img.h, true, false);
                self.wall = data;
                self.damage_all();
                return;
            }
        }
        let (top, bottom, glows): (u32, u32, [(i32, i32, i32, u32); 3]) = match index {
            1 => (0x08223F, 0x0C5468, [(20, 25, 60, 0x1E90FF), (85, 80, 50, 0x00D1B2), (60, 10, 35, 0x7B61FF)]),
            2 => (0x0C1F16, 0x1F4A2E, [(15, 20, 55, 0x3FA65D), (85, 75, 45, 0xB8D94A), (50, 100, 40, 0x0E5C3A)]),
            3 => (0x141519, 0x2C2F37, [(30, 20, 55, 0x50555F), (80, 90, 45, 0x2E323A), (90, 10, 30, 0x5D6470)]),
            4 => (0x2A1736, 0xC25B5B, [(25, 85, 60, 0xFFB45E), (80, 20, 45, 0xF7768E), (50, 50, 30, 0xFFD39A)]),
            5 => (0x040B1A, 0x0B1E33, [(30, 30, 50, 0x23D5AB), (70, 20, 45, 0x7B61FF), (50, 80, 50, 0x1460A0)]),
            _ => (0x1B1440, 0x7A2E6E, [(20, 20, 60, 0x4C6EF5), (85, 85, 55, 0xFF7A59), (65, 35, 40, 0xC056A0)]),
        };
        let (top, bottom) = if dark { (top, bottom) } else { (gfx::lighten(top, 50), gfx::lighten(bottom, 50)) };
        struct Out(*mut u32);
        unsafe impl Sync for Out {}
        impl Out {
            fn ptr(&self) -> *mut u32 {
                self.0
            }
        }
        let out = Out(data.as_mut_ptr());
        const BAND: i32 = 8;
        let paint = |band: usize| {
            let y0 = band as i32 * BAND;
            for y in y0..(y0 + BAND).min(sh) {
                let base = gfx::mix(top, bottom, (y * 255 / sh) as u32);
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
                            c = gfx::mix(c, color, (t * t / 255 * 150 / 255) as u32);
                        }
                    }
                    let n = ((x as u32).wrapping_mul(73856093) ^ (y as u32).wrapping_mul(19349663)) % 5;
                    let c = if n == 0 { gfx::lighten(c, 3) } else { c };
                    unsafe { *out.ptr().add((y * sw + x) as usize) = c };
                }
            }
        };
        crate::smp::parallel_for(((sh + BAND - 1) / BAND) as usize, &paint);
        self.wall = data;
        self.damage_all();
    }

    // ---- desktop icons ---------------------------------------------------------

    fn reload_icons(&mut self) {
        let entries = FILESYSTEM.lock().as_ref().and_then(|fs| fs.entries("/Desktop").ok()).unwrap_or_default();
        let old: Vec<(String, (i32, i32))> = self.icons.iter().map(|i| (i.name.clone(), i.pos)).collect();
        self.icons.clear();
        let (cell_w, cell_h) = (96, 92);
        let rows = ((self.sh - MENU_H - 130) / cell_h).max(1);
        for (slot, e) in entries.into_iter().enumerate() {
            let slot = slot as i32;
            let pos = old.iter().find(|(n, p)| *n == e.name && p.0 >= 0).map(|(_, p)| *p).unwrap_or_else(|| {
                let (col, row) = (slot / rows, slot % rows);
                (self.sw - 16 - cell_w - col * cell_w, MENU_H + 14 + row * cell_h)
            });
            self.icons.push(DeskIcon { name: e.name, is_dir: e.is_dir, pos });
        }
        self.damage_all();
    }

    fn icon_rect(&self, i: usize) -> Rect {
        let p = self.icons[i].pos;
        (p.0, p.1, 96, 88)
    }

    // ---- geometry ---------------------------------------------------------------

    fn work_area(&self) -> Rect {
        let dock = DOCK_SIZE.load(Ordering::Relaxed) as i32;
        (0, MENU_H, self.sw, self.sh - MENU_H - dock - 30)
    }

    fn win_index(&self, id: u32) -> Option<usize> {
        self.wins.iter().position(|w| w.id == id)
    }

    fn focused(&self) -> Option<u32> {
        self.wins.iter().rev().find(|w| !w.minimized && !w.dying && !matches!(w.anim, Some(Anim { kind: AnimKind::Minimize, .. }))).map(|w| w.id)
    }

    fn update_focus(&mut self) {
        let f = self.focused();
        for w in self.wins.iter() {
            let now = Some(w.id) == f;
            let was = w.ctx.focused.swap(now, Ordering::Relaxed);
            if was != now {
                w.ctx.events.lock().push_back(Event::Focus(now));
            }
        }
        self.damage_rect((0, 0, self.sw, MENU_H));
    }

    fn surface_for(&self, w: i32, h: i32) -> Option<Framebuffer> {
        framebuffer::with_screen(|fb| Framebuffer::offscreen32(fb, w.max(1) as usize, (h - TITLE_H).max(1) as usize))
    }

    // ---- opening and closing ---------------------------------------------------

    pub fn open(&mut self, app: AppId, arg: Option<String>) {
        let info = apps::info(app);
        if info.single {
            if let Some(i) = self.wins.iter().position(|w| w.app == app && !w.dying) {
                let id = self.wins[i].id;
                if arg.is_some() {
                    *self.wins[i].ctx.arg.lock() = arg;
                    self.wins[i].ctx.events.lock().push_back(Event::Focus(true));
                }
                self.unminimize(id);
                self.raise(id);
                return;
            }
        }
        let wa = self.work_area();
        let w = info.size.0.min(wa.2 - 40);
        let h = (info.size.1 + TITLE_H).min(wa.3 - 20);
        let n = self.wins.iter().filter(|w| !w.minimized && !w.dying).count() as i32;
        let x = (wa.0 + (wa.2 - w) / 2 + (n % 5) * 28 - 56).clamp(wa.0 + 4, (wa.0 + wa.2 - w - 4).max(wa.0 + 4));
        let y = (wa.1 + (wa.3 - h) / 3 + (n % 5) * 24).clamp(wa.1 + 4, (wa.1 + wa.3 - h).max(wa.1 + 4));
        let Some(mut surface) = self.surface_for(w, h) else { return };
        let bg = self.th.window;
        surface.pixels32().fill(bg);
        let ctx = Context::new(surface);
        *ctx.title.lock() = String::from(info.name);
        let done = apps::start(app, ctx.clone(), arg);
        let id = self.next_id;
        self.next_id += 1;
        let now = crate::time::uptime_ms();
        self.wins.push(Win {
            id,
            app,
            ctx,
            done,
            x,
            y,
            w,
            h,
            minimized: false,
            maximized: false,
            snapped: false,
            restore: (x, y, w, h),
            last_version: u64::MAX,
            keep_alive: info.keep_alive,
            captured: false,
            anim: Some(Anim { kind: AnimKind::Open, start: now, dur: 240, from: (x, y, w, h), to: (x, y, w, h) }),
            snap: None,
            dying: false,
            title: String::from(info.name),
        });
        self.dock_bounce.retain(|(a, _)| *a != app);
        self.dock_bounce.push((app, now));
        self.update_focus();
        crate::audio::effect(crate::audio::Effect::Open);
        if self.tiling {
            self.retile();
        }
        self.damage_win((x, y, w, h));
        self.damage_dock();
        super::buddy_desk::on_app_opened(info.name);
    }

    fn raise(&mut self, id: u32) {
        let Some(i) = self.win_index(id) else { return };
        let prev = self.focused().and_then(|f| self.win_index(f)).map(|k| self.wins[k].rect());
        let w = self.wins.remove(i);
        let r = w.rect();
        self.wins.push(w);
        self.update_focus();
        self.damage_win(r);
        if let Some(p) = prev {
            self.damage_win(p);
        }
    }

    fn snapshot(&mut self, id: u32) {
        let Some(i) = self.win_index(id) else { return };
        let r = self.wins[i].rect();
        let (w, h) = (r.2 as usize, r.3 as usize);
        let mut buf = alloc::vec![0u32; w * h];
        {
            let mut cv = Canvas::new(&mut buf, w, h);
            let th = self.th;
            cv.fill(th.window);
            let surf = self.wins[i].ctx.surface.lock();
            let src = surf.pixels32_ref();
            draw_frame(&mut cv, &th, &self.fb, (0, 0, r.2, r.3), &self.wins[i].title, true, None, Some((src, surf.width(), surf.height())), self.wins[i].maximized);
        }
        self.wins[i].snap = Some((buf, w, h));
    }

    fn start_anim(&mut self, id: u32, kind: AnimKind, from: Rect, to: Rect, dur: u64) {
        if let Some(i) = self.win_index(id) {
            self.wins[i].anim = Some(Anim { kind, start: crate::time::uptime_ms(), dur, from, to });
            self.damage_win(from);
            self.damage_win(to);
        }
    }

    fn dock_target(&self, app: AppId) -> Rect {
        let (_, items) = self.dock_layout();
        for it in items {
            if let DockItem::App(a, r, _) = it {
                if a == app {
                    return r;
                }
            }
        }
        (self.sw / 2, self.sh - 30, 10, 10)
    }

    fn minimize(&mut self, id: u32) {
        let Some(i) = self.win_index(id) else { return };
        if self.wins[i].minimized || self.wins[i].dying {
            return;
        }
        self.snapshot(id);
        let Some(i) = self.win_index(id) else { return };
        let from = self.wins[i].rect();
        let to = self.dock_target(self.wins[i].app);
        self.wins[i].captured = false;
        self.wins[i].ctx.visible.store(false, Ordering::Relaxed);
        self.start_anim(id, AnimKind::Minimize, from, to, 340);
        self.update_focus();
        if self.tiling {
            self.retile();
        }
        self.damage_dock();
    }

    fn unminimize(&mut self, id: u32) {
        let Some(i) = self.win_index(id) else { return };
        if !self.wins[i].minimized {
            return;
        }
        self.wins[i].minimized = false;
        self.wins[i].ctx.visible.store(true, Ordering::Relaxed);
        let to = self.wins[i].rect();
        let from = self.dock_target(self.wins[i].app);
        self.snapshot(id);
        self.start_anim(id, AnimKind::Restore, from, to, 300);
        if self.tiling {
            self.retile();
        }
    }

    fn close(&mut self, id: u32) {
        let Some(i) = self.win_index(id) else { return };
        if self.wins[i].keep_alive && !self.wins[i].done.load(Ordering::SeqCst) {
            self.minimize(id);
            return;
        }
        if self.wins[i].dying {
            return;
        }
        self.snapshot(id);
        let Some(i) = self.win_index(id) else { return };
        let r = self.wins[i].rect();
        self.wins[i].dying = true;
        self.wins[i].ctx.closed.store(true, Ordering::SeqCst);
        self.start_anim(id, AnimKind::Close, r, r, 180);
        crate::audio::effect(crate::audio::Effect::Close);
        self.update_focus();
        if self.tiling {
            self.retile();
        }
        self.damage_dock();
    }

    /// Moves and resizes a window (recreating its surface if the size changed).
    fn set_rect(&mut self, id: u32, r: Rect) {
        let Some(i) = self.win_index(id) else { return };
        let old = self.wins[i].rect();
        let (w, h) = (r.2.max(240), r.3.max(TITLE_H + 120));
        let size_changed = w != self.wins[i].w || h != self.wins[i].h;
        let bg = self.th.window;
        let win = &mut self.wins[i];
        win.x = r.0;
        win.y = r.1;
        win.w = w;
        win.h = h;
        if size_changed {
            let ctx = win.ctx.clone();
            let (cw, ch) = (w as usize, (h - TITLE_H) as usize);
            {
                let mut surface = ctx.surface.lock();
                // keep what was drawn, so the window does not flash while the app catches up
                let mut fresh = Framebuffer::offscreen32(&surface, cw, ch);
                {
                    let (ow, oh) = (surface.width(), surface.height());
                    let src = surface.pixels32_ref();
                    let dst = fresh.pixels32();
                    dst.fill(bg);
                    for y in 0..oh.min(ch) {
                        let n = ow.min(cw);
                        dst[y * cw..y * cw + n].copy_from_slice(&src[y * ow..y * ow + n]);
                    }
                }
                fresh.damage_rows(0, ch);
                *surface = fresh;
            }
            let mut q = ctx.events.lock();
            q.retain(|e| !matches!(e, Event::Resize { .. }));
            q.push_back(Event::Resize { w: cw, h: ch });
        }
        self.wins[i].last_version = u64::MAX;
        self.damage_win(old);
        self.damage_win((r.0, r.1, w, h));
    }

    fn zoom(&mut self, id: u32) {
        let Some(i) = self.win_index(id) else { return };
        let from = self.wins[i].rect();
        let to = if self.wins[i].maximized || self.wins[i].snapped {
            self.wins[i].maximized = false;
            self.wins[i].snapped = false;
            self.wins[i].restore
        } else {
            self.wins[i].restore = from;
            self.wins[i].maximized = true;
            let wa = self.work_area();
            (wa.0 + 6, wa.1 + 6, wa.2 - 12, wa.3 - 12)
        };
        self.animate_to(id, to);
    }

    fn animate_to(&mut self, id: u32, to: Rect) {
        let Some(i) = self.win_index(id) else { return };
        let from = self.wins[i].rect();
        if from == to {
            return;
        }
        self.snapshot(id);
        self.set_rect(id, to);
        self.start_anim(id, AnimKind::Zoom, from, to, 230);
    }

    fn snap(&mut self, id: u32, r: Rect) {
        let Some(i) = self.win_index(id) else { return };
        if !self.wins[i].maximized && !self.wins[i].snapped {
            self.wins[i].restore = self.wins[i].rect();
        }
        self.wins[i].snapped = true;
        self.wins[i].maximized = false;
        self.animate_to(id, r);
    }

    fn snap_target(&self, x: i32, y: i32) -> Option<Rect> {
        let wa = self.work_area();
        let (l, t, w, h) = (wa.0 + 6, wa.1 + 6, wa.2 - 12, wa.3 - 12);
        let (hw, hh) = (w / 2, h / 2);
        let corner = 90;
        let left = x <= 3;
        let right = x >= self.sw - 4;
        if left && y <= t + corner {
            Some((l, t, hw - 3, hh - 3))
        } else if right && y <= t + corner {
            Some((l + w - hw + 3, t, hw - 3, hh - 3))
        } else if left && y >= t + h - corner {
            Some((l, t + hh + 3, hw - 3, h - hh - 3))
        } else if right && y >= t + h - corner {
            Some((l + w - hw + 3, t + hh + 3, hw - 3, h - hh - 3))
        } else if left {
            Some((l, t, hw - 3, h))
        } else if right {
            Some((l + w - hw + 3, t, hw - 3, h))
        } else if y <= MENU_H + 2 {
            Some((l, t, w, h))
        } else {
            None
        }
    }

    fn retile(&mut self) {
        let wa = self.work_area();
        let gap = 8;
        let mut ids: Vec<u32> = self.wins.iter().filter(|w| !w.minimized && !w.dying && !matches!(w.anim, Some(Anim { kind: AnimKind::Minimize, .. }))).map(|w| w.id).collect();
        ids.sort();
        let n = ids.len() as i32;
        if n == 0 {
            return;
        }
        let (x, y, w, h) = (wa.0 + gap, wa.1 + gap, wa.2 - 2 * gap, wa.3 - 2 * gap);
        for (k, id) in ids.iter().enumerate() {
            let r = if n == 1 {
                (x, y, w, h)
            } else if k == 0 {
                (x, y, w * 55 / 100 - gap / 2, h)
            } else {
                let rows = n - 1;
                let rh = (h - (rows - 1) * gap) / rows;
                let mw = w * 55 / 100;
                (x + mw + gap / 2, y + (k as i32 - 1) * (rh + gap), w - mw - gap / 2, rh)
            };
            if let Some(i) = self.win_index(*id) {
                if !self.wins[i].snapped && !self.wins[i].maximized {
                    self.wins[i].restore = self.wins[i].rect();
                }
                self.wins[i].snapped = true;
                self.animate_to(*id, r);
            }
        }
    }

    // ---- the dock ------------------------------------------------------------

    fn dock_apps(&self) -> Vec<AppId> {
        let mut v: Vec<AppId> = apps::DOCK.to_vec();
        for w in &self.wins {
            if !w.dying && !v.contains(&w.app) {
                v.push(w.app);
            }
        }
        v
    }

    fn running(&self, app: AppId) -> bool {
        self.wins.iter().any(|w| w.app == app && !w.dying)
    }

    fn dock_zone(&self) -> Bx {
        let base = DOCK_SIZE.load(Ordering::Relaxed) as i32;
        let n = self.dock_apps().len() as i32 + 2;
        let wmax = n * (base * 17 / 10 + 10) + 140;
        ((self.sw - wmax) / 2, self.sh - base * 2 - 80, (self.sw + wmax) / 2, self.sh)
    }

    fn dock_layout(&self) -> (Rect, Vec<DockItem>) {
        let base = DOCK_SIZE.load(Ordering::Relaxed) as i32;
        let (pad, gap, sep) = (8, 6, 14);
        let list = self.dock_apps();
        let count = list.len() + 2;
        let mut sizes = Vec::with_capacity(count);
        for k in 0..count {
            let s = self.dock_scale.get(k).copied().unwrap_or(1000);
            sizes.push(if k == list.len() { sep } else { base * s / 1000 });
        }
        let total: i32 = sizes.iter().sum::<i32>() + gap * (count as i32 - 1) + 2 * pad;
        let bar_h = base + 2 * pad;
        let bar_y = self.sh - 8 - bar_h;
        let bar = ((self.sw - total) / 2, bar_y, total, bar_h);
        let mut items = Vec::with_capacity(count);
        let mut x = bar.0 + pad;
        for (k, sz) in sizes.iter().enumerate() {
            let r = (x, bar_y + pad + base - sz, *sz, *sz);
            if k < list.len() {
                items.push(DockItem::App(list[k], r, k));
            } else if k == list.len() {
                items.push(DockItem::Sep(x + sz / 2));
            } else {
                items.push(DockItem::Trash(r, k));
            }
            x += sz + gap;
        }
        (bar, items)
    }

    fn dock_centres(&self) -> Vec<i32> {
        let base = DOCK_SIZE.load(Ordering::Relaxed) as i32;
        let (pad, gap, sep) = (8, 6, 14);
        let n = self.dock_apps().len() + 2;
        let mut total = 2 * pad + gap * (n as i32 - 1);
        for k in 0..n {
            total += if k == n - 2 { sep } else { base };
        }
        let mut x = (self.sw - total) / 2 + pad;
        let mut out = Vec::with_capacity(n);
        for k in 0..n {
            let s = if k == n - 2 { sep } else { base };
            out.push(x + s / 2);
            x += s + gap;
        }
        out
    }

    fn update_dock(&mut self) -> bool {
        let n = self.dock_apps().len() + 2;
        self.dock_scale.resize(n, 1000);
        let base = DOCK_SIZE.load(Ordering::Relaxed) as i32;
        let bar_y = self.sh - 8 - (base + 16);
        let near = self.mouse.1 >= bar_y - 14 && self.menu.is_none() && !self.blocked() && matches!(self.drag, Drag::None | Drag::Icon { .. });
        let centres = self.dock_centres();
        let near = near && self.mouse.0 >= centres[0] - base * 2 && self.mouse.0 <= centres[n - 1] + base * 2;
        self.dock_track = near;
        let mut moving = false;
        for k in 0..n {
            let target = if near && MAGNIFY.load(Ordering::Relaxed) && k != n - 2 {
                let d = (self.mouse.0 - centres[k]).abs();
                let reach = base * 3;
                if d < reach {
                    1000 + 600 * ease_in_out((reach - d) * 1000 / reach) / 1000
                } else {
                    1000
                }
            } else {
                1000
            };
            let cur = self.dock_scale[k];
            let mut next = cur + (target - cur) * 40 / 100;
            if (target - next).abs() < 10 {
                next = target;
            }
            if next != cur {
                moving = true;
            }
            self.dock_scale[k] = next;
        }
        let now = crate::time::uptime_ms();
        self.dock_bounce.retain(|(_, t)| now < t + 900);
        moving || !self.dock_bounce.is_empty()
    }

    // ---- main loop -----------------------------------------------------------

    fn main_loop(&mut self) {
        loop {
            let now = crate::time::uptime_ms();
            self.poll_mouse();
            self.poll_keys();
            self.handle_requests();
            self.housekeeping(now);
            let mut animating = false;
            for i in 0..self.wins.len() {
                let v = self.wins[i].ctx.surface.lock().version();
                if v != self.wins[i].last_version {
                    self.wins[i].last_version = v;
                    if !self.wins[i].minimized {
                        let w = &self.wins[i];
                        let r = (w.x, w.y + TITLE_H, w.x + w.w, w.y + w.h);
                        self.damage_rect(r);
                    }
                }
                if let Some(a) = self.wins[i].anim {
                    animating = true;
                    let cur = self.anim_rect(&self.wins[i], &a, now);
                    self.damage_win(a.from);
                    self.damage_win(a.to);
                    self.damage_win(cur);
                    if now >= a.start + a.dur {
                        self.finish_anim(i);
                    }
                }
            }
            self.wins.retain(|w| !(w.dying && w.anim.is_none() && w.done.load(Ordering::SeqCst)));
            if self.update_dock() {
                animating = true;
                self.damage_dock();
            }
            if SHOW_BUDDY.load(Ordering::Relaxed) && self.buddy.tick(now) {
                let r = self.buddy_area();
                self.damage_rect(r);
            }
            if self.overlay_animating(now) {
                animating = true;
            }
            if !self.damage.is_empty() && now >= self.last_frame + 14 {
                self.last_frame = now;
                self.compose();
                sched::note_progress();
            }
            if animating {
                sched::yield_now();
            } else {
                sched::yield_idle();
            }
        }
    }

    fn overlay_animating(&mut self, now: u64) -> bool {
        let mut any = false;
        let mut full = false;
        if let Some(s) = &self.spot {
            if now < s.opened + 220 || (now / 530) % 2 != ((now.saturating_sub(16)) / 530) % 2 {
                any = true;
                full = true;
            }
        }
        if let Some((t, _)) = self.launchpad {
            if now < t + 280 {
                any = true;
                full = true;
            }
        }
        if let Some((t, _)) = self.mission {
            if now < t + 300 {
                any = true;
                full = true;
            }
        }
        if self.panel != Panel::None && now < self.panel_at + 220 {
            any = true;
            let r = (self.sw - 420, 0, self.sw, self.sh);
            self.damage_rect(r);
        }
        if let Some(m) = &self.menu {
            if now < m.opened + 140 {
                any = true;
                let (frame, _) = ui::menu_layout(&self.f, m.x, m.y, &m.items, self.sw, self.sh);
                self.damage_rect(bx((frame.0 - 30, frame.1 - 30, frame.2 + 60, frame.3 + 60)));
            }
        }
        let before = self.notifs.len();
        self.notifs.retain(|n| now < n.at + 5200);
        if before != self.notifs.len() || self.notifs.iter().any(|n| now < n.at + 420 || now > n.at + 4780) {
            any = true;
            let r = (self.sw - 400, MENU_H, self.sw, MENU_H + 20 + 82 * 4);
            self.damage_rect(r);
        }
        if full {
            self.damage_all();
        }
        any
    }

    fn anim_rect(&self, w: &Win, a: &Anim, now: u64) -> Rect {
        let t = (((now.saturating_sub(a.start)) * 1000) / a.dur.max(1)).min(1000) as i32;
        match a.kind {
            AnimKind::Open => scale_about(w.rect(), 880 + 120 * ease_out(t) / 1000),
            AnimKind::Close => scale_about(w.rect(), 1000 - 100 * ease_out(t) / 1000),
            AnimKind::Minimize => lerp_rect(a.from, a.to, ease_in_out(t)),
            AnimKind::Restore => lerp_rect(a.from, a.to, ease_out(t)),
            AnimKind::Zoom => lerp_rect(a.from, a.to, ease_out(t)),
        }
    }

    fn finish_anim(&mut self, i: usize) {
        let Some(a) = self.wins[i].anim.take() else { return };
        self.wins[i].snap = None;
        if a.kind == AnimKind::Minimize {
            self.wins[i].minimized = true;
        }
        let r = self.wins[i].rect();
        self.damage_win(r);
        self.damage_win(a.from);
        self.damage_win(a.to);
    }

    fn housekeeping(&mut self, now: u64) {
        let secs = crate::time::uptime_secs();
        if secs != self.last_second {
            self.last_second = secs;
            let c = read_rtc();
            if (c.0, c.1) != (self.clock.0, self.clock.1) {
                self.clock = c;
                self.damage_rect((self.sw - 320, 0, self.sw, MENU_H));
            }
            sched::reap();
            let full = FILESYSTEM.lock().as_ref().and_then(|fs| fs.list("/Trash").ok()).map(|l| !l.is_empty()).unwrap_or(false);
            if full != self.trash_full {
                self.trash_full = full;
                self.damage_dock();
            }
            if now >= self.icons_checked + 2000 {
                self.icons_checked = now;
                let names: Vec<String> = FILESYSTEM.lock().as_ref().and_then(|fs| fs.list("/Desktop").ok()).unwrap_or_default();
                let mine: Vec<String> = self.icons.iter().map(|i| i.name.clone()).collect();
                if names != mine {
                    self.reload_icons();
                }
                self.damage_dock();
            }
            if now >= self.fps_at + 1000 {
                FPS.store(self.frames, Ordering::Relaxed);
                self.frames = 0;
                self.fps_at = now;
            }
        }
        for i in 0..self.wins.len() {
            if self.wins[i].done.load(Ordering::SeqCst) && !self.wins[i].dying {
                let id = self.wins[i].id;
                self.close(id);
            }
        }
        for i in 0..self.wins.len() {
            let t = self.wins[i].ctx.title.lock().clone();
            if !t.is_empty() && t != self.wins[i].title {
                self.wins[i].title = t;
                let r = self.wins[i].rect();
                self.damage_rect((r.0, r.1, r.0 + r.2, r.1 + TITLE_H));
                self.damage_rect((0, 0, self.sw, MENU_H));
            }
            let want = self.wins[i].ctx.want_size.lock().take();
            if let Some((w, h)) = want {
                let id = self.wins[i].id;
                let r = self.wins[i].rect();
                let wa = self.work_area();
                let (nw, nh) = ((w as i32).min(wa.2 - 20), (h as i32 + TITLE_H).min(wa.3 - 10));
                let nx = (r.0 + (r.2 - nw) / 2).clamp(wa.0, (wa.0 + wa.2 - nw).max(wa.0));
                let ny = r.1.clamp(wa.1, (wa.1 + wa.3 - nh).max(wa.1));
                self.set_rect(id, (nx, ny, nw, nh));
            }
        }
        let g = theme::GENERATION.load(Ordering::Relaxed);
        if g != self.theme_gen {
            self.theme_gen = g;
            self.th = theme::current();
            self.make_wallpaper();
            self.damage_all();
        }
        if (WALLPAPER.load(Ordering::Relaxed), WALL_GEN.load(Ordering::Relaxed), theme::is_dark()) != self.wall_key {
            self.make_wallpaper();
        }
        let tiling = TILING.load(Ordering::Relaxed);
        if tiling != self.tiling {
            self.tiling = tiling;
            if tiling {
                self.retile();
            }
        }
    }

    fn handle_requests(&mut self) {
        while let Some(r) = super::take_request() {
            match r {
                Request::Open(app, arg) => self.open(app, arg),
                Request::OpenFile(path) => {
                    let is_dir = FILESYSTEM.lock().as_ref().map(|fs| fs.is_dir(&path)).unwrap_or(false);
                    if is_dir {
                        self.open(AppId::Files, Some(path));
                    } else {
                        self.open(apps::for_file(&path), Some(path));
                    }
                }
                Request::Notify(title, body) => self.push_notif(&title, &body),
                Request::BuddySay(t) => self.buddy.say(&t),
                Request::ShowDesktop => self.show_desktop(),
            }
        }
    }

    fn push_notif(&mut self, title: &str, body: &str) {
        let now = crate::time::uptime_ms();
        self.history.insert(0, Notif { title: String::from(title), body: String::from(body), at: now });
        self.history.truncate(30);
        if !DND.load(Ordering::Relaxed) {
            self.notifs.push(Notif { title: String::from(title), body: String::from(body), at: now });
            if self.notifs.len() > 3 {
                self.notifs.remove(0);
            }
            crate::audio::effect(crate::audio::Effect::Click);
        }
        self.damage_rect((self.sw - 400, MENU_H, self.sw, MENU_H + 400));
    }

    fn show_desktop(&mut self) {
        let ids: Vec<u32> = self.wins.iter().filter(|w| !w.minimized && !w.dying).map(|w| w.id).collect();
        for id in ids {
            self.minimize(id);
        }
    }

    // ---- mouse -----------------------------------------------------------------

    fn poll_mouse(&mut self) {
        let wheel = crate::mouse::take_wheel();
        if wheel != 0 {
            self.on_wheel(wheel);
        }
        let Some((dx, dy, b)) = crate::mouse::take() else { return };
        if let Some(w) = self.wins.iter().find(|w| w.captured && !w.minimized) {
            let mut q = w.ctx.events.lock();
            if q.len() < 256 {
                q.push_back(Event::MouseRaw { dx, dy, buttons: b });
            }
            self.buttons = b;
            return;
        }
        let (sx, sy) = crate::mouse::scale(dx, dy);
        let old = self.mouse;
        let nx = (old.0 + sx).clamp(0, self.sw - 1);
        let ny = (old.1 - sy).clamp(0, self.sh - 1);
        self.mouse = (nx, ny);
        let prev = self.buttons;
        self.buttons = b;
        if (nx, ny) != old {
            self.damage_pointer(old);
            self.damage_pointer((nx, ny));
            self.on_move();
        }
        let (pe, re) = crate::mouse::last_edges();
        for (bit, button) in [(1u8, 0u8), (2, 1), (4, 2)] {
            let was = prev & bit != 0;
            let press = !was && (b & bit != 0 || pe & bit != 0);
            if press {
                self.on_press(button);
            }
            if (was || press) && (b & bit == 0 || re & bit != 0) && !(b & bit != 0 && !was) {
                self.on_release(button);
            }
        }
    }

    fn damage_pointer(&mut self, p: (i32, i32)) {
        self.damage_rect((p.0 - 12, p.1 - 12, p.0 + 26, p.1 + 30));
    }

    fn send(&self, id: u32, ev: Event) {
        if let Some(i) = self.win_index(id) {
            let mut q = self.wins[i].ctx.events.lock();
            if q.len() >= 300 {
                return;
            }
            if let (Event::MouseMove { .. }, Some(Event::MouseMove { .. })) = (&ev, q.back()) {
                q.pop_back();
            }
            q.push_back(ev);
        }
    }

    fn local(&self, id: u32) -> (i32, i32) {
        match self.win_index(id) {
            Some(i) => (self.mouse.0 - self.wins[i].x, self.mouse.1 - self.wins[i].y - TITLE_H),
            None => (0, 0),
        }
    }

    fn hit_window(&self, p: (i32, i32)) -> Option<(u32, Part)> {
        for w in self.wins.iter().rev() {
            if w.minimized || w.dying || matches!(w.anim, Some(Anim { kind: AnimKind::Minimize, .. })) {
                continue;
            }
            let b = 5;
            if !within(p, (w.x - b, w.y - b, w.w + 2 * b, w.h + 2 * b)) {
                continue;
            }
            let (rx, ry) = (p.0 - w.x, p.1 - w.y);
            if !w.maximized && !self.tiling {
                let mut e = 0;
                if rx < 4 {
                    e |= 1;
                }
                if rx >= w.w - 4 {
                    e |= 2;
                }
                if ry < 3 {
                    e |= 4;
                }
                if ry >= w.h - 4 {
                    e |= 8;
                }
                if e != 0 {
                    return Some((w.id, Part::Edge(e)));
                }
            }
            if rx < 0 || ry < 0 || rx >= w.w || ry >= w.h {
                continue;
            }
            if ry < TITLE_H {
                for (k, part) in [Part::Close, Part::Minimize, Part::Zoom].into_iter().enumerate() {
                    let cx = 20 + k as i32 * 20;
                    if (rx - cx).abs() <= 8 && (ry - TITLE_H / 2).abs() <= 8 {
                        return Some((w.id, part));
                    }
                }
                return Some((w.id, Part::Title));
            }
            return Some((w.id, Part::Content));
        }
        None
    }

    fn on_wheel(&mut self, dz: i32) {
        if self.blocked() {
            return;
        }
        if let Some((id, Part::Content)) = self.hit_window(self.mouse) {
            self.send(id, Event::Wheel { dy: dz });
        }
    }

    fn on_move(&mut self) {
        let p = self.mouse;
        if self.menu.is_some() {
            let (frame, rows, items) = {
                let m = self.menu.as_ref().unwrap();
                let (f, r) = ui::menu_layout(&self.f, m.x, m.y, &m.items, self.sw, self.sh);
                (f, r, m.items.clone())
            };
            let h = rows.iter().position(|r| within(p, *r)).filter(|&i| items[i].enabled && !items[i].separator);
            if let Some(m) = &mut self.menu {
                if h != m.hover {
                    m.hover = h;
                    self.damage_rect(bx((frame.0 - 4, frame.1 - 4, frame.2 + 8, frame.3 + 8)));
                }
            }
            if p.1 < MENU_H {
                if let Some((kind, x)) = self.menubar_hit(p.0) {
                    let open_kind = self.menu.as_ref().map(|m| m.kind);
                    if Some(kind) != open_kind && matches!(open_kind, Some(MenuKind::Logo | MenuKind::App | MenuKind::AppDefined(_) | MenuKind::Window)) {
                        self.open_menu(kind, x, MENU_H);
                    }
                }
            }
            return;
        }
        if self.volume_drag {
            self.set_volume_from_mouse();
            return;
        }
        match self.drag {
            Drag::Move { id, ox, oy, .. } => {
                self.drag = Drag::Move { id, ox, oy, moved: true };
                if let Some(i) = self.win_index(id) {
                    if self.wins[i].maximized || self.wins[i].snapped {
                        let r = self.wins[i].restore;
                        self.wins[i].maximized = false;
                        self.wins[i].snapped = false;
                        let nox = (ox * r.2 / self.wins[i].w.max(1)).clamp(60, (r.2 - 60).max(60));
                        self.set_rect(id, (p.0 - nox, (p.1 - oy).max(MENU_H), r.2, r.3));
                        self.drag = Drag::Move { id, ox: nox, oy, moved: true };
                        return;
                    }
                    if self.tiling {
                        return;
                    }
                    let (w, h) = (self.wins[i].w, self.wins[i].h);
                    self.set_rect(id, (p.0 - ox, (p.1 - oy).max(MENU_H), w, h));
                }
                let t = self.snap_target(p.0, p.1);
                if t != self.snap_preview {
                    if let Some(o) = self.snap_preview {
                        self.damage_rect(bx(o));
                    }
                    if let Some(n) = t {
                        self.damage_rect(bx(n));
                    }
                    self.snap_preview = t;
                }
            }
            Drag::Resize { id, edge, mouse, start } => {
                let (dx, dy) = (p.0 - mouse.0, p.1 - mouse.1);
                let (mut x, mut y, mut w, mut h) = start;
                if edge & 2 != 0 {
                    w = start.2 + dx;
                }
                if edge & 8 != 0 {
                    h = start.3 + dy;
                }
                if edge & 1 != 0 {
                    w = start.2 - dx;
                    x = start.0 + dx;
                }
                if edge & 4 != 0 {
                    h = start.3 - dy;
                    y = start.1 + dy;
                }
                let (mw, mh) = (240, TITLE_H + 120);
                if w < mw {
                    if edge & 1 != 0 {
                        x = start.0 + start.2 - mw;
                    }
                    w = mw;
                }
                if h < mh {
                    if edge & 4 != 0 {
                        y = start.1 + start.3 - mh;
                    }
                    h = mh;
                }
                if let Some(i) = self.win_index(id) {
                    self.wins[i].maximized = false;
                    self.wins[i].snapped = false;
                }
                self.set_rect(id, (x, y.max(MENU_H), w, h));
            }
            Drag::Content { id, .. } => {
                let (lx, ly) = self.local(id);
                self.send(id, Event::MouseMove { x: lx, y: ly });
            }
            Drag::Icon { index, start, .. } => {
                if index < self.icons.len() {
                    let old = self.icon_rect(index);
                    let pos = (self.icons[index].pos.0 + p.0 - start.0, self.icons[index].pos.1 + p.1 - start.1);
                    self.icons[index].pos = (pos.0.clamp(0, self.sw - 96), pos.1.clamp(MENU_H, self.sh - 120));
                    self.drag = Drag::Icon { index, start: p, moved: true };
                    self.damage_rect(bx(old));
                    let r = self.icon_rect(index);
                    self.damage_rect(bx(r));
                }
            }
            Drag::None => {
                let hit = self.hit_window(p);
                let over = match hit {
                    Some((id, Part::Content)) if !self.blocked() => Some(id),
                    _ => None,
                };
                if over != self.hover_win {
                    if let Some(old) = self.hover_win {
                        self.send(old, Event::MouseLeave);
                    }
                    self.hover_win = over;
                }
                if let Some(id) = over {
                    let (lx, ly) = self.local(id);
                    self.send(id, Event::MouseMove { x: lx, y: ly });
                }
                self.cursor = match hit {
                    Some((_, Part::Edge(e))) => match e {
                        1 | 2 => Cursor::ResizeH,
                        4 | 8 => Cursor::ResizeV,
                        5 | 10 => Cursor::ResizeD1,
                        _ => Cursor::ResizeD2,
                    },
                    Some((id, Part::Content)) if !self.blocked() => self.win_index(id).map(|i| Cursor::from_u8(self.wins[i].ctx.cursor.load(Ordering::Relaxed))).unwrap_or(Cursor::Arrow),
                    _ => Cursor::Arrow,
                };
                if let Some((id, Part::Close | Part::Minimize | Part::Zoom | Part::Title)) = hit {
                    if let Some(i) = self.win_index(id) {
                        let w = &self.wins[i];
                        self.damage_rect((w.x, w.y, w.x + 80, w.y + TITLE_H));
                    }
                }
                let dh = self.dock_hit(p);
                if dh != self.dock_hover {
                    self.dock_hover = dh;
                    self.damage_dock();
                }
                if self.blocked() || self.panel != Panel::None {
                    self.damage_all();
                }
                if p == (0, 0) && self.mission.is_none() {
                    self.toggle_mission();
                }
            }
        }
    }

    fn blocked(&self) -> bool {
        self.launchpad.is_some() || self.mission.is_some() || self.spot.is_some()
    }

    fn dock_hit(&self, p: (i32, i32)) -> Option<usize> {
        let (bar, items) = self.dock_layout();
        if p.1 < bar.1 - 80 {
            return None;
        }
        for it in items {
            match it {
                DockItem::App(_, r, k) | DockItem::Trash(r, k) => {
                    if p.0 >= r.0 - 3 && p.0 < r.0 + r.2 + 3 && p.1 >= r.1 && p.1 < bar.1 + bar.3 {
                        return Some(k);
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn on_press(&mut self, button: u8) {
        let p = self.mouse;
        let now = crate::time::uptime_ms();
        let (t, x, y, n) = self.last_click;
        let clicks = if button == 0 && now < t + 450 && (p.0 - x).abs() < 6 && (p.1 - y).abs() < 6 { n + 1 } else { 1 };
        if button == 0 {
            self.last_click = (now, p.0, p.1, clicks);
        }
        // an open menu takes the click
        if let Some(m) = self.menu.take() {
            let (frame, rows) = ui::menu_layout(&self.f, m.x, m.y, &m.items, self.sw, self.sh);
            self.damage_rect(bx((frame.0 - 30, frame.1 - 30, frame.2 + 60, frame.3 + 60)));
            self.damage_rect((0, 0, self.sw, MENU_H));
            if within(p, frame) {
                if let Some(i) = rows.iter().position(|r| within(p, *r)) {
                    if m.items[i].enabled && !m.items[i].separator {
                        self.menu_action(m.kind, i);
                    }
                }
                return;
            }
            if p.1 >= MENU_H {
                return;
            }
            if let Some((kind, _)) = self.menubar_hit(p.0) {
                if kind == m.kind {
                    return;
                }
            }
        }
        if self.handle_overlay_click(p) {
            return;
        }
        if p.1 < MENU_H {
            self.menubar_click(p.0);
            return;
        }
        if self.panel != Panel::None {
            if self.panel_click(p) {
                return;
            }
            self.panel = Panel::None;
            self.damage_rect((self.sw - 420, 0, self.sw, self.sh));
        }
        for k in 0..self.notifs.len() {
            if within(p, self.notif_rect(k, now)) {
                self.notifs.remove(k);
                self.damage_rect((self.sw - 400, MENU_H, self.sw, MENU_H + 400));
                return;
            }
        }
        if self.buddy_hit(p) {
            if button == 0 {
                if clicks >= 2 {
                    self.open(AppId::Buddy, None);
                } else {
                    self.buddy.pet();
                }
            }
            return;
        }
        if let Some(k) = self.dock_hit(p) {
            self.dock_click(k, button);
            return;
        }
        match self.hit_window(p) {
            Some((id, part)) => {
                if self.focused() != Some(id) {
                    self.raise(id);
                }
                match part {
                    Part::Close if button == 0 => self.close(id),
                    Part::Minimize if button == 0 => self.minimize(id),
                    Part::Zoom if button == 0 => self.zoom(id),
                    Part::Title | Part::Close | Part::Minimize | Part::Zoom => {
                        if button == 0 {
                            if clicks == 2 {
                                self.zoom(id);
                            } else if let Some(i) = self.win_index(id) {
                                self.drag = Drag::Move { id, ox: p.0 - self.wins[i].x, oy: p.1 - self.wins[i].y, moved: false };
                            }
                        } else if button == 1 {
                            self.open_menu(MenuKind::Window, p.0, p.1);
                        }
                    }
                    Part::Edge(edge) => {
                        if let Some(i) = self.win_index(id) {
                            self.drag = Drag::Resize { id, edge, mouse: p, start: self.wins[i].rect() };
                        }
                    }
                    Part::Content => {
                        if let Some(i) = self.win_index(id) {
                            if self.wins[i].ctx.raw_mouse.load(Ordering::Relaxed) && !self.wins[i].captured && button == 0 {
                                self.wins[i].captured = true;
                                self.damage_pointer(p);
                                let r = self.wins[i].rect();
                                self.damage_win(r);
                                return;
                            }
                        }
                        let (lx, ly) = self.local(id);
                        self.send(id, Event::MouseDown { x: lx, y: ly, button, clicks });
                        self.drag = Drag::Content { id, button };
                    }
                }
            }
            None => self.desktop_click(p, button, clicks),
        }
    }

    fn desktop_click(&mut self, p: (i32, i32), button: u8, clicks: u8) {
        let hit = (0..self.icons.len()).rev().find(|&i| within(p, self.icon_rect(i)));
        if self.icon_sel != hit {
            self.icon_sel = hit;
            self.damage_all();
        }
        match (hit, button) {
            (Some(i), 0) => {
                if clicks >= 2 {
                    super::open_file(&alloc::format!("/Desktop/{}", self.icons[i].name));
                } else {
                    self.drag = Drag::Icon { index: i, start: p, moved: false };
                }
            }
            (Some(i), 1) => self.open_menu(MenuKind::Icon(i), p.0, p.1),
            (None, 1) => self.open_menu(MenuKind::Desktop, p.0, p.1),
            _ => {}
        }
    }

    fn on_release(&mut self, button: u8) {
        self.volume_drag = false;
        match self.drag {
            Drag::Move { id, moved, .. } => {
                if moved && button == 0 {
                    if let Some(t) = self.snap_preview.take() {
                        self.damage_rect(bx(t));
                        let wa = self.work_area();
                        if t == (wa.0 + 6, wa.1 + 6, wa.2 - 12, wa.3 - 12) {
                            if let Some(i) = self.win_index(id) {
                                let cur = self.wins[i].rect();
                                self.wins[i].restore = cur;
                                self.wins[i].maximized = true;
                                self.animate_to(id, t);
                            }
                        } else {
                            self.snap(id, t);
                        }
                    }
                }
                self.drag = Drag::None;
            }
            Drag::Resize { .. } => self.drag = Drag::None,
            Drag::Content { id, button: b } => {
                let (lx, ly) = self.local(id);
                self.send(id, Event::MouseUp { x: lx, y: ly, button });
                if button == b {
                    self.drag = Drag::None;
                }
            }
            Drag::Icon { index, moved, .. } => {
                self.drag = Drag::None;
                if moved && index < self.icons.len() {
                    let path = alloc::format!("/Desktop/{}", self.icons[index].name);
                    if let Some((id, Part::Content)) = self.hit_window(self.mouse) {
                        self.send(id, Event::Drop(path));
                    } else if let Some(k) = self.dock_hit(self.mouse) {
                        let (_, items) = self.dock_layout();
                        if matches!(items.get(k), Some(DockItem::Trash(..))) {
                            super::files::move_to_trash(&path);
                            self.reload_icons();
                        }
                    }
                }
            }
            Drag::None => {
                if let Some((id, Part::Content)) = self.hit_window(self.mouse) {
                    let (lx, ly) = self.local(id);
                    self.send(id, Event::MouseUp { x: lx, y: ly, button });
                }
            }
        }
        if let Some(t) = self.snap_preview.take() {
            self.damage_rect(bx(t));
        }
    }

    fn dock_click(&mut self, k: usize, button: u8) {
        let (_, items) = self.dock_layout();
        match items.get(k).copied() {
            Some(DockItem::App(app, r, _)) => {
                if button == 1 {
                    self.open_menu(MenuKind::Dock(app), r.0, r.1 - 8);
                    return;
                }
                let wins: Vec<u32> = self.wins.iter().filter(|w| w.app == app && !w.dying).map(|w| w.id).collect();
                if wins.is_empty() {
                    self.open(app, None);
                    return;
                }
                let visible = wins.iter().any(|id| self.win_index(*id).map(|i| !self.wins[i].minimized).unwrap_or(false));
                let front = self.focused().map(|f| wins.contains(&f)).unwrap_or(false);
                if front && visible {
                    for id in wins {
                        self.minimize(id);
                    }
                } else {
                    for id in wins {
                        self.unminimize(id);
                        self.raise(id);
                    }
                }
            }
            Some(DockItem::Trash(r, _)) => {
                if button == 1 {
                    self.open_menu(MenuKind::Trash, r.0, r.1 - 8);
                } else {
                    self.open(AppId::Files, Some(String::from("/Trash")));
                }
            }
            _ => {}
        }
    }

    // ---- menus -----------------------------------------------------------------

    fn front_app(&self) -> Option<(u32, AppId)> {
        self.focused().and_then(|id| self.win_index(id)).map(|i| (self.wins[i].id, self.wins[i].app))
    }

    fn menubar_items(&self) -> Vec<(MenuKind, String, i32, i32)> {
        let mut out = Vec::new();
        let mut x = 16;
        out.push((MenuKind::Logo, String::from("zero"), x, 36));
        x += 38;
        let name = self.front_app().map(|(_, a)| String::from(apps::info(a).name)).unwrap_or_else(|| String::from("Finder"));
        let w = gfx::text_width(&self.fb, &name) + 24;
        out.push((MenuKind::App, name, x, w));
        x += w;
        if let Some((id, _)) = self.front_app() {
            if let Some(i) = self.win_index(id) {
                let menus = self.wins[i].ctx.menus.lock().clone();
                for (k, (m, _)) in menus.iter().enumerate() {
                    let w = gfx::text_width(&self.f, m) + 24;
                    out.push((MenuKind::AppDefined(k), m.clone(), x, w));
                    x += w;
                }
            }
        }
        let w = gfx::text_width(&self.f, "Window") + 24;
        out.push((MenuKind::Window, String::from("Window"), x, w));
        out
    }

    fn menubar_hit(&self, x: i32) -> Option<(MenuKind, i32)> {
        self.menubar_items().into_iter().find(|(_, _, ix, w)| x >= *ix - 10 && x < *ix - 10 + *w).map(|(k, _, ix, _)| (k, ix - 10))
    }

    fn status_items(&self) -> [(i32, i32); 5] {
        let cw = 150;
        let c = self.sw - cw - 10;
        [(c, cw), (c - 34, 30), (c - 64, 30), (c - 94, 30), (c - 124, 30)]
    }

    fn menubar_click(&mut self, x: i32) {
        let st = self.status_items();
        let hit = |k: usize| x >= st[k].0 && x < st[k].0 + st[k].1;
        if hit(0) {
            self.toggle_panel(Panel::Notifications);
        } else if hit(1) || hit(3) || hit(4) {
            self.toggle_panel(Panel::Control);
        } else if hit(2) {
            self.open_spotlight();
        } else if let Some((kind, mx)) = self.menubar_hit(x) {
            self.open_menu(kind, mx, MENU_H);
        }
    }

    fn open_menu(&mut self, kind: MenuKind, x: i32, y: i32) {
        let items = self.menu_items(kind);
        if items.is_empty() {
            return;
        }
        if let Some(m) = &self.menu {
            let (frame, _) = ui::menu_layout(&self.f, m.x, m.y, &m.items, self.sw, self.sh);
            self.damage_rect(bx((frame.0 - 30, frame.1 - 30, frame.2 + 60, frame.3 + 60)));
        }
        let (frame, _) = ui::menu_layout(&self.f, x, y, &items, self.sw, self.sh);
        self.damage_rect(bx((frame.0 - 30, frame.1 - 30, frame.2 + 60, frame.3 + 60)));
        self.damage_rect((0, 0, self.sw, MENU_H));
        self.menu = Some(PopMenu { kind, x, y, items, hover: None, opened: crate::time::uptime_ms() });
    }

    fn menu_items(&self, kind: MenuKind) -> Vec<MenuItem> {
        let m = MenuItem::new;
        match kind {
            MenuKind::Logo => alloc::vec![
                m("About zero"),
                MenuItem::sep(),
                m("System Settings...").hint("Super ,"),
                m("Launchpad").hint("F4"),
                m("Mission Control").hint("F3"),
                MenuItem::sep(),
                m("Show Desktop").hint("Super D"),
                m("Lock Screen"),
                MenuItem::sep(),
                m("Restart..."),
                m("Shut Down..."),
            ],
            MenuKind::App => {
                let name = self.front_app().map(|(_, a)| apps::info(a).name).unwrap_or("Finder");
                alloc::vec![m(&alloc::format!("About {}", name)), MenuItem::sep(), m("Hide").hint("Super H"), m(&alloc::format!("Quit {}", name)).hint("Super Q")]
            }
            MenuKind::AppDefined(k) => {
                let Some((id, _)) = self.front_app() else { return Vec::new() };
                let Some(i) = self.win_index(id) else { return Vec::new() };
                let menus = self.wins[i].ctx.menus.lock();
                menus.get(k).map(|(_, items)| items.iter().map(|s| if s == "-" { MenuItem::sep() } else { m(s) }).collect()).unwrap_or_default()
            }
            MenuKind::Window => alloc::vec![
                m("Minimize").hint("Super M"),
                m("Zoom").hint("Super F"),
                MenuItem::sep(),
                m("Tile Left").hint("Super Left"),
                m("Tile Right").hint("Super Right"),
                m(if self.tiling { "Stop Auto Tiling" } else { "Auto Tile Windows" }).hint("Super T"),
                MenuItem::sep(),
                m("Close Window").hint("Super W"),
            ],
            MenuKind::Desktop => alloc::vec![
                m("New Folder"),
                m("New Text File"),
                MenuItem::sep(),
                m("Change Wallpaper..."),
                m("Clean Up Icons"),
                MenuItem::sep(),
                m("Open Terminal"),
                m("Show in Files"),
            ],
            MenuKind::Icon(_) => alloc::vec![m("Open"), m("Show in Files"), MenuItem::sep(), m("Move to Trash")],
            MenuKind::Dock(app) => {
                let running = self.running(app);
                let mut v = alloc::vec![m(if running { "Show" } else { "Open" })];
                if !apps::info(app).single {
                    v.push(m("New Window"));
                }
                if running {
                    v.push(MenuItem::sep());
                    v.push(m("Quit"));
                }
                v
            }
            MenuKind::Trash => {
                let empty = FILESYSTEM.lock().as_ref().and_then(|fs| fs.list("/Trash").ok()).map(|l| l.is_empty()).unwrap_or(true);
                alloc::vec![m("Open"), m("Empty Trash").disabled(empty)]
            }
        }
    }

    fn quit_app(&mut self, app: AppId) {
        let ids: Vec<u32> = self.wins.iter().filter(|w| w.app == app).map(|w| w.id).collect();
        for id in ids {
            if let Some(i) = self.win_index(id) {
                self.wins[i].keep_alive = false;
            }
            self.close(id);
        }
    }

    fn menu_action(&mut self, kind: MenuKind, i: usize) {
        let front = self.front_app();
        match kind {
            MenuKind::Logo => match i {
                0 => self.open(AppId::About, None),
                2 => self.open(AppId::Settings, None),
                3 => self.toggle_launchpad(),
                4 => self.toggle_mission(),
                6 => self.show_desktop(),
                7 => {
                    super::onboard::lock_screen(self.sw, self.sh);
                    self.damage_all();
                }
                9 => {
                    crate::pfs::sync_if_dirty();
                    crate::power::reboot()
                }
                10 => {
                    crate::pfs::sync_if_dirty();
                    crate::power::shutdown()
                }
                _ => {}
            },
            MenuKind::App => {
                if let Some((id, app)) = front {
                    match i {
                        0 => {
                            if app == AppId::About {
                                return;
                            }
                            let name = apps::info(app).name;
                            self.push_notif(name, "A zero app. Built with integer math and love.");
                        }
                        2 => self.minimize(id),
                        3 => self.quit_app(app),
                        _ => {}
                    }
                } else if i == 0 {
                    self.open(AppId::About, None);
                }
            }
            MenuKind::AppDefined(k) => {
                if let Some((id, _)) = front {
                    self.send(id, Event::Menu(k, i));
                }
            }
            MenuKind::Window => {
                let wa = self.work_area();
                match (i, front) {
                    (0, Some((id, _))) => self.minimize(id),
                    (1, Some((id, _))) => self.zoom(id),
                    (3, Some((id, _))) => self.snap(id, (wa.0 + 6, wa.1 + 6, wa.2 / 2 - 9, wa.3 - 12)),
                    (4, Some((id, _))) => self.snap(id, (wa.0 + wa.2 / 2 + 3, wa.1 + 6, wa.2 / 2 - 9, wa.3 - 12)),
                    (5, _) => self.set_tiling(!self.tiling),
                    (7, Some((id, _))) => self.close(id),
                    _ => {}
                }
            }
            MenuKind::Desktop => match i {
                0 | 1 => {
                    let folder = i == 0;
                    {
                        let mut fs = FILESYSTEM.lock();
                        if let Some(fs) = fs.as_mut() {
                            let mut n = 1;
                            let mut name = String::from(if folder { "untitled folder" } else { "untitled.txt" });
                            while fs.list("/Desktop").map(|l| l.contains(&name)).unwrap_or(false) {
                                n += 1;
                                name = if folder { alloc::format!("untitled folder {}", n) } else { alloc::format!("untitled {}.txt", n) };
                            }
                            let path = alloc::format!("/Desktop/{}", name);
                            let _ = if folder { fs.create_dir(&path) } else { fs.create_file(&path) };
                        }
                    }
                    crate::pfs::sync_if_dirty();
                    self.reload_icons();
                }
                3 => self.open(AppId::Settings, Some(String::from("wallpaper"))),
                4 => {
                    for icon in self.icons.iter_mut() {
                        icon.pos = (-1, -1);
                    }
                    self.reload_icons();
                }
                6 => self.open(AppId::Terminal, None),
                7 => self.open(AppId::Files, Some(String::from("/Desktop"))),
                _ => {}
            },
            MenuKind::Icon(k) => {
                if k < self.icons.len() {
                    let path = alloc::format!("/Desktop/{}", self.icons[k].name);
                    match i {
                        0 => super::open_file(&path),
                        1 => self.open(AppId::Files, Some(String::from("/Desktop"))),
                        3 => {
                            super::files::move_to_trash(&path);
                            self.reload_icons();
                        }
                        _ => {}
                    }
                }
            }
            MenuKind::Dock(app) => {
                let label = self.menu_items(kind).get(i).map(|m| m.label.clone()).unwrap_or_default();
                match label.as_str() {
                    "Open" => self.open(app, None),
                    "Show" => {
                        let ids: Vec<u32> = self.wins.iter().filter(|w| w.app == app).map(|w| w.id).collect();
                        for id in ids {
                            self.unminimize(id);
                            self.raise(id);
                        }
                    }
                    "New Window" => self.open(app, None),
                    "Quit" => self.quit_app(app),
                    _ => {}
                }
            }
            MenuKind::Trash => match i {
                0 => self.open(AppId::Files, Some(String::from("/Trash"))),
                1 => {
                    super::files::empty_trash();
                    self.damage_dock();
                    self.push_notif("Trash", "The trash is empty.");
                }
                _ => {}
            },
        }
    }

    fn set_tiling(&mut self, on: bool) {
        TILING.store(on, Ordering::Relaxed);
        save_flag("tiling", on);
        self.push_notif("Window tiling", if on { "Windows now tile automatically." } else { "Windows float freely again." });
    }

    // ---- panels ------------------------------------------------------------------

    fn toggle_panel(&mut self, p: Panel) {
        self.panel = if self.panel == p { Panel::None } else { p };
        self.panel_at = crate::time::uptime_ms();
        self.damage_rect((self.sw - 420, 0, self.sw, self.sh));
    }

    fn cc_rect(&self) -> Rect {
        (self.sw - 340, MENU_H + 6, 330, 330)
    }

    fn cc_tiles(&self) -> [(Rect, &'static str); 6] {
        let r = self.cc_rect();
        let (x, y) = (r.0 + 12, r.1 + 12);
        let w = (r.2 - 36) / 2;
        [
            ((x, y, w, 60), "Dark Mode"),
            ((x + w + 12, y, w, 60), "Tiling"),
            ((x, y + 72, w, 60), "Focus"),
            ((x + w + 12, y + 72, w, 60), "Bit"),
            ((x, y + 144, w, 60), "Mission Ctrl"),
            ((x + w + 12, y + 144, w, 60), "Desktop"),
        ]
    }

    fn cc_slider(&self) -> Rect {
        let r = self.cc_rect();
        (r.0 + 24, r.1 + 246, r.2 - 48, 24)
    }

    fn panel_click(&mut self, p: (i32, i32)) -> bool {
        match self.panel {
            Panel::Control => {
                if !within(p, self.cc_rect()) {
                    return false;
                }
                let tiles = self.cc_tiles();
                for (k, (t, _)) in tiles.iter().enumerate() {
                    if within(p, *t) {
                        match k {
                            0 => {
                                theme::set_dark(!theme::is_dark());
                                theme::save();
                            }
                            1 => self.set_tiling(!self.tiling),
                            2 => DND.store(!DND.load(Ordering::Relaxed), Ordering::Relaxed),
                            3 => {
                                let on = !SHOW_BUDDY.load(Ordering::Relaxed);
                                SHOW_BUDDY.store(on, Ordering::Relaxed);
                                save_flag("show_buddy", on);
                            }
                            4 => {
                                self.panel = Panel::None;
                                self.toggle_mission();
                            }
                            _ => {
                                self.panel = Panel::None;
                                self.show_desktop();
                            }
                        }
                        self.damage_all();
                        return true;
                    }
                }
                let s = self.cc_slider();
                if within(p, (s.0 - 10, s.1 - 6, s.2 + 20, s.3 + 12)) {
                    self.volume_drag = true;
                    self.set_volume_from_mouse();
                }
                true
            }
            Panel::Notifications => within(p, (self.sw - 370, MENU_H + 6, 360, self.sh - MENU_H - 120)),
            Panel::None => false,
        }
    }

    fn set_volume_from_mouse(&mut self) {
        let s = self.cc_slider();
        let v = ((self.mouse.0 - s.0) * 100 / s.2.max(1)).clamp(0, 100);
        crate::audio::VOLUME.store(v as u32, Ordering::Relaxed);
        self.damage_rect(bx((s.0 - 20, s.1 - 20, s.2 + 40, s.3 + 40)));
    }

    // ---- spotlight, launchpad, mission control --------------------------------------

    fn open_spotlight(&mut self) {
        self.spot = Some(Spotlight { query: String::new(), selected: 0, opened: crate::time::uptime_ms() });
        self.menu = None;
        self.damage_all();
    }

    fn spot_hits(&self) -> Vec<Hit> {
        let Some(s) = &self.spot else { return Vec::new() };
        let q = s.query.trim().to_ascii_lowercase();
        let mut out = Vec::new();
        if q.is_empty() {
            return out;
        }
        if let Some(v) = calc(&q) {
            out.push(Hit { title: alloc::format!("= {}", v), sub: String::from("Calculator"), icon: Icon::Calculator, action: Action::Copy(alloc::format!("{}", v)) });
        }
        for a in apps::APPS {
            if a.name.to_ascii_lowercase().contains(&q) && a.id != AppId::Script {
                out.push(Hit { title: String::from(a.name), sub: String::from(a.category), icon: a.icon, action: Action::App(a.id) });
            }
        }
        for key in ["settings", "wallpaper", "dark", "sound", "network", "display", "dock"] {
            if key.starts_with(&q) || q.starts_with(key) {
                out.push(Hit { title: String::from("System Settings"), sub: alloc::format!("{} settings", key), icon: Icon::Settings, action: Action::Settings });
                break;
            }
        }
        let mut found = Vec::new();
        if let Some(fs) = FILESYSTEM.lock().as_ref() {
            find_files(fs, "/", &q, &mut found, 0);
        }
        for path in found.into_iter().take(6) {
            let name: String = path.rsplit('/').next().unwrap_or("").into();
            let is_dir = FILESYSTEM.lock().as_ref().map(|fs| fs.is_dir(&path)).unwrap_or(false);
            out.push(Hit { title: name.clone(), sub: path.clone(), icon: apps::file_icon(&name, is_dir), action: Action::File(path) });
        }
        out.push(Hit { title: alloc::format!("Ask Bit: {}", s.query.trim()), sub: String::from("Your desktop buddy"), icon: Icon::Buddy, action: Action::AskBuddy(String::from(s.query.trim())) });
        out.truncate(8);
        out
    }

    fn run_hit(&mut self, h: Hit) {
        self.spot = None;
        self.damage_all();
        match h.action {
            Action::App(a) => self.open(a, None),
            Action::File(p) => super::open_file(&p),
            Action::Settings => self.open(AppId::Settings, None),
            Action::AskBuddy(q) => {
                super::buddy_desk::queue_question(&q);
                self.open(AppId::Buddy, None);
            }
            Action::Copy(v) => self.push_notif("Calculator", &alloc::format!("The answer is {}", v)),
        }
    }

    fn toggle_launchpad(&mut self) {
        if self.launchpad.is_some() {
            self.launchpad = None;
        } else {
            let mut copy = self.scr.clone();
            {
                let (w, h) = (self.sw, self.sh);
                let mut cv = Canvas::new(&mut copy, w as usize, h as usize);
                cv.blur(0, 0, w, h, 14);
                cv.rect_a(0, 0, w, h, 0x000000, 80);
            }
            self.lp_blur = copy;
            self.launchpad = Some((crate::time::uptime_ms(), String::new()));
        }
        self.menu = None;
        self.damage_all();
    }

    fn lp_items(&self) -> Vec<&'static apps::AppInfo> {
        let q = self.launchpad.as_ref().map(|(_, q)| q.to_ascii_lowercase()).unwrap_or_default();
        apps::APPS.iter().filter(|a| a.id != AppId::Script && a.id != AppId::About && (q.is_empty() || a.name.to_ascii_lowercase().contains(&q))).collect()
    }

    fn lp_cell(&self, k: usize) -> Rect {
        let cols = 7.min((self.sw - 80) / 128).max(3);
        let (cw, ch) = (128, 124);
        let gx = (self.sw - cols * cw) / 2;
        (gx + (k as i32 % cols) * cw, 130 + (k as i32 / cols) * ch, cw, ch)
    }

    fn toggle_mission(&mut self) {
        self.mission = if self.mission.is_some() { None } else { Some((crate::time::uptime_ms(), false)) };
        self.menu = None;
        self.damage_all();
    }

    fn mission_rects(&self) -> Vec<(u32, Rect, Rect)> {
        let ws: Vec<&Win> = self.wins.iter().filter(|w| !w.minimized && !w.dying).collect();
        let n = ws.len() as i32;
        if n == 0 {
            return Vec::new();
        }
        let cols = isqrt_i(n - 1) + 1;
        let rows = (n + cols - 1) / cols;
        let area = (40, MENU_H + 60, self.sw - 80, self.sh - MENU_H - 200);
        let (cw, ch) = (area.2 / cols, area.3 / rows);
        ws.iter()
            .enumerate()
            .map(|(k, w)| {
                let (c, r) = (k as i32 % cols, k as i32 / cols);
                let s = ((cw - 30) * 1000 / w.w.max(1)).min((ch - 44) * 1000 / w.h.max(1)).min(1000);
                let (tw, th) = (w.w * s / 1000, w.h * s / 1000);
                (w.id, w.rect(), (area.0 + c * cw + (cw - tw) / 2, area.1 + r * ch + (ch - th) / 2, tw, th))
            })
            .collect()
    }

    fn handle_overlay_click(&mut self, p: (i32, i32)) -> bool {
        if self.spot.is_some() {
            let hits = self.spot_hits();
            let (x, y, w) = self.spot_box();
            for (k, h) in hits.iter().enumerate() {
                if within(p, (x + 8, y + 62 + k as i32 * 44, w - 16, 42)) {
                    self.run_hit(h.clone());
                    return true;
                }
            }
            if !within(p, (x, y, w, 70 + hits.len() as i32 * 44)) {
                self.spot = None;
                self.damage_all();
            }
            return true;
        }
        if self.launchpad.is_some() {
            let items = self.lp_items();
            for (k, a) in items.iter().enumerate() {
                if within(p, self.lp_cell(k)) {
                    let id = a.id;
                    self.launchpad = None;
                    self.damage_all();
                    self.open(id, None);
                    return true;
                }
            }
            self.launchpad = None;
            self.damage_all();
            return true;
        }
        if self.mission.is_some() {
            let rects = self.mission_rects();
            let pick = rects.iter().rev().find(|(_, _, t)| within(p, *t)).map(|(id, _, _)| *id);
            self.mission = None;
            if let Some(id) = pick {
                self.raise(id);
            }
            self.damage_all();
            return true;
        }
        false
    }

    fn spot_box(&self) -> (i32, i32, i32) {
        let w = 620.min(self.sw - 40);
        ((self.sw - w) / 2, self.sh / 5, w)
    }

    // ---- keyboard --------------------------------------------------------------

    fn poll_keys(&mut self) {
        while input::pop_raw_key().is_some() {}
        let mods_now = input::modifiers();
        if mods_now & SUPER != 0 {
            let mut released = false;
            for w in self.wins.iter_mut() {
                if w.captured {
                    w.captured = false;
                    released = true;
                }
            }
            if released {
                self.damage_all();
            }
        }
        if let Some((k, _)) = self.switcher {
            if mods_now & (ALT | SUPER) == 0 {
                self.switcher = None;
                if let Some(id) = self.switch_list().get(k).copied() {
                    self.unminimize(id);
                    self.raise(id);
                }
                self.damage_all();
            }
        }
        while let Some((key, mods)) = input::pop_gui_key() {
            sched::note_progress();
            if self.wins.iter().any(|w| w.captured && !w.minimized) {
                if let Some(id) = self.focused() {
                    self.send(id, Event::Key(key, mods));
                }
                continue;
            }
            if self.spot.is_some() {
                self.spot_key(key);
                continue;
            }
            if self.launchpad.is_some() {
                match key {
                    Key::Char('\u{1b}') | Key::F(4) => self.toggle_launchpad(),
                    Key::Char('\u{8}') => {
                        if let Some((_, q)) = &mut self.launchpad {
                            q.pop();
                        }
                        self.damage_all();
                    }
                    Key::Char('\n') => {
                        if let Some(a) = self.lp_items().first() {
                            let id = a.id;
                            self.toggle_launchpad();
                            self.open(id, None);
                        }
                    }
                    Key::Char(c) if !c.is_control() => {
                        if let Some((_, q)) = &mut self.launchpad {
                            q.push(c);
                        }
                        self.damage_all();
                    }
                    _ => {}
                }
                continue;
            }
            if self.mission.is_some() && matches!(key, Key::Char('\u{1b}') | Key::F(3)) {
                self.toggle_mission();
                continue;
            }
            if self.menu.is_some() && key == Key::Char('\u{1b}') {
                self.menu = None;
                self.damage_all();
                continue;
            }
            if self.shortcut(key, mods) {
                continue;
            }
            if let Some(id) = self.focused() {
                self.send(id, Event::Key(key, mods));
            } else if let (Key::Delete, Some(i)) = (key, self.icon_sel) {
                if i < self.icons.len() {
                    super::files::move_to_trash(&alloc::format!("/Desktop/{}", self.icons[i].name));
                    self.icon_sel = None;
                    self.reload_icons();
                }
            }
        }
    }

    fn switch_list(&self) -> Vec<u32> {
        self.wins.iter().rev().filter(|w| !w.dying).map(|w| w.id).collect()
    }

    fn shortcut(&mut self, key: Key, mods: u8) -> bool {
        let sup = mods & SUPER != 0;
        let alt = mods & ALT != 0;
        let ctrl = mods & CTRL != 0;
        let focused = self.focused();
        let wa = self.work_area();
        match key {
            Key::F(3) => self.toggle_mission(),
            Key::F(4) => self.toggle_launchpad(),
            Key::Up if ctrl => self.toggle_mission(),
            Key::Char('\t') if sup || alt => {
                let n = self.switch_list().len();
                if n == 0 {
                    return true;
                }
                let next = match self.switcher {
                    Some((k, _)) => (k + if mods & SHIFT != 0 { n - 1 } else { 1 }) % n,
                    None => 1 % n,
                };
                self.switcher = Some((next, crate::time::uptime_ms()));
                self.damage_all();
            }
            Key::Char(' ') if sup || ctrl => self.open_spotlight(),
            Key::Char('\n') if sup => self.open(AppId::Terminal, None),
            Key::Char(',') if sup => self.open(AppId::Settings, None),
            Key::Char('e') | Key::Char('E') if sup => self.open(AppId::Files, None),
            Key::Char('d') | Key::Char('D') if sup => self.show_desktop(),
            Key::Char('w') | Key::Char('W') if sup => {
                if let Some(id) = focused {
                    self.close(id);
                }
            }
            Key::Char('q') | Key::Char('Q') if sup => {
                if let Some((_, app)) = self.front_app() {
                    self.quit_app(app);
                }
            }
            Key::Char('m') | Key::Char('M') | Key::Char('h') | Key::Char('H') if sup => {
                if let Some(id) = focused {
                    self.minimize(id);
                }
            }
            Key::Char('f') | Key::Char('F') if sup => {
                if let Some(id) = focused {
                    self.zoom(id);
                }
            }
            Key::Char('t') | Key::Char('T') if sup => self.set_tiling(!self.tiling),
            Key::Char(c) if sup && c.is_ascii_digit() && c != '0' => {
                if let Some(app) = apps::DOCK.get(c as usize - '1' as usize) {
                    self.open(*app, None);
                }
            }
            Key::Left if sup => {
                if let Some(id) = focused {
                    self.snap(id, (wa.0 + 6, wa.1 + 6, wa.2 / 2 - 9, wa.3 - 12));
                }
            }
            Key::Right if sup => {
                if let Some(id) = focused {
                    self.snap(id, (wa.0 + wa.2 / 2 + 3, wa.1 + 6, wa.2 / 2 - 9, wa.3 - 12));
                }
            }
            Key::Up if sup => {
                if let Some(id) = focused {
                    if let Some(i) = self.win_index(id) {
                        if !self.wins[i].maximized {
                            self.zoom(id);
                        }
                    }
                }
            }
            Key::Down if sup => {
                if let Some(id) = focused {
                    if let Some(i) = self.win_index(id) {
                        if self.wins[i].maximized || self.wins[i].snapped {
                            self.zoom(id);
                        } else {
                            self.minimize(id);
                        }
                    }
                }
            }
            _ => return false,
        }
        true
    }

    fn spot_key(&mut self, key: Key) {
        let n = self.spot_hits().len();
        let Some(s) = self.spot.as_mut() else { return };
        match key {
            Key::Char('\u{1b}') => self.spot = None,
            Key::Char('\n') => {
                let sel = s.selected;
                if let Some(h) = self.spot_hits().get(sel).cloned() {
                    self.run_hit(h);
                }
            }
            Key::Char('\u{8}') => {
                s.query.pop();
                s.selected = 0;
            }
            Key::Up => s.selected = s.selected.saturating_sub(1),
            Key::Down => s.selected = (s.selected + 1).min(n.saturating_sub(1)),
            Key::Char(c) if !c.is_control() => {
                s.query.push(c);
                s.selected = 0;
            }
            _ => {}
        }
        self.damage_all();
    }

    // ---- painting --------------------------------------------------------------

    fn buddy_area(&self) -> Bx {
        let (bar, _) = self.dock_layout();
        (bar.0 - 220, bar.1 - 190, bar.0 + bar.2 + 220, bar.1 + 8)
    }

    fn buddy_hit(&self, p: (i32, i32)) -> bool {
        if !SHOW_BUDDY.load(Ordering::Relaxed) || self.blocked() {
            return false;
        }
        let (bar, _) = self.dock_layout();
        within(p, self.buddy.rect(bar))
    }

    fn compose(&mut self) {
        let started = crate::time::uptime_ms();
        self.frame_now = started;
        let rects = core::mem::take(&mut self.damage);
        self.frames += 1;
        let (sw, sh) = (self.sw, self.sh);
        // hold every window's picture while the cores paint
        let guards: Vec<_> = self.wins.iter().map(|w| w.ctx.surface.lock()).collect();
        let surfs: Vec<Surf> = guards.iter().map(|g| Surf { px: g.pixels32_ref().as_ptr(), w: g.width(), h: g.height() }).collect();
        struct Out(*mut u32);
        unsafe impl Sync for Out {}
        impl Out {
            fn ptr(&self) -> *mut u32 {
                self.0
            }
        }
        for d in &rects {
            let clip = (d.0.max(0), d.1.max(0), d.2.min(sw), d.3.min(sh));
            if clip.2 <= clip.0 || clip.3 <= clip.1 {
                continue;
            }
            let area = (clip.2 - clip.0) * (clip.3 - clip.1);
            let cpus = crate::smp::cpu_count() as i32;
            let bands = if area > 50_000 && cpus > 1 { (cpus * 2).min((clip.3 - clip.1) / 16).max(1) } else { 1 };
            let bh = (clip.3 - clip.1 + bands - 1) / bands;
            let out = Out(self.scr.as_mut_ptr());
            let this = &*self;
            let surfs_ref = &surfs;
            let job = |b: usize| {
                let y0 = clip.1 + b as i32 * bh;
                let y1 = (y0 + bh).min(clip.3);
                if y1 <= y0 {
                    return;
                }
                let rows = unsafe { core::slice::from_raw_parts_mut(out.ptr().add((y0 * sw) as usize), ((y1 - y0) * sw) as usize) };
                let mut cv = Canvas::new(rows, sw as usize, (y1 - y0) as usize);
                cv.shift_origin(0, -y0);
                cv.clip_abs((clip.0, 0, clip.2, y1 - y0));
                this.paint_base(&mut cv, surfs_ref, (clip.0, y0, clip.2, y1));
            };
            crate::smp::parallel_for(bands as usize, &job);
            // what needs the whole picture (blur) is drawn by this core
            let out = Out(self.scr.as_mut_ptr());
            let all = unsafe { core::slice::from_raw_parts_mut(out.ptr(), (sw * sh) as usize) };
            let mut cv = Canvas::new(all, sw as usize, sh as usize);
            cv.clip_abs(clip);
            paint_top(self, &mut cv, clip);
        }
        drop(surfs);
        drop(guards);
        let scr = &self.scr;
        framebuffer::with_screen(|fb| {
            for d in &rects {
                let (x0, y0, x1, y1) = (d.0.max(0) as usize, d.1.max(0) as usize, d.2.min(sw) as usize, d.3.min(sh) as usize);
                fb.present32(scr, x0, y0, x1, y1);
            }
        });
        FRAME_US.store(((crate::time::uptime_ms() - started) * 1000) as u32, Ordering::Relaxed);
    }

    /// Wallpaper, desktop icons and windows: safe to draw in bands on any core.
    fn paint_base(&self, cv: &mut Canvas, surfs: &[Surf], clip: Bx) {
        let sw = self.sw as usize;
        for y in clip.1..clip.3 {
            let s = &self.wall[y as usize * sw + clip.0 as usize..y as usize * sw + clip.2 as usize];
            cv.blit(clip.0, y, s, s.len(), 1);
        }
        for (k, icon) in self.icons.iter().enumerate() {
            let r = self.icon_rect(k);
            if !meets(bx(r), clip) {
                continue;
            }
            let sel = self.icon_sel == Some(k);
            if sel {
                cv.round_a(r.0 + 14, r.1 + 2, 68, 62, 9, 0x000000, 70);
            }
            icons::draw(cv, apps::file_icon(&icon.name, icon.is_dir), r.0 + 18, r.1 + 4, 60);
            let label = gfx::ellipsize(&self.f, &icon.name, 92);
            let tw = gfx::text_width(&self.f, &label);
            let lx = r.0 + (r.2 - tw) / 2;
            if sel {
                cv.round(lx - 5, r.1 + 66, tw + 10, 20, 5, self.th.accent);
            } else {
                cv.text_a(lx + 1, r.1 + 68, &label, &self.f, 0x000000, 160);
            }
            cv.text(lx, r.1 + 67, &label, &self.f, 0xFFFFFF);
        }
        let focused = self.focused();
        let now = self.frame_now;
        let mission = self.mission.map(|(t, _)| ease_out((((now.saturating_sub(t)) * 1000) / 300).min(1000) as i32));
        let mrects = if mission.is_some() { self.mission_rects() } else { Vec::new() };
        if mission.is_some() {
            cv.rect_a(clip.0, clip.1, clip.2 - clip.0, clip.3 - clip.1, 0x000000, (mission.unwrap() * 110 / 1000) as u32);
        }
        for (k, w) in self.wins.iter().enumerate() {
            if w.minimized && w.anim.is_none() {
                continue;
            }
            let surf = &surfs[k];
            let src = unsafe { core::slice::from_raw_parts(surf.px, surf.w * surf.h) };
            let is_front = focused == Some(w.id);
            if let Some(t) = mission {
                if let Some((_, from, to)) = mrects.iter().find(|(id, _, _)| *id == w.id) {
                    let r = lerp_rect(*from, *to, t);
                    if meets(bx((r.0 - 40, r.1 - 40, r.2 + 80, r.3 + 90)), clip) {
                        cv.drop_shadow(r.0, r.1, r.2, r.3, RADIUS, 24, 120, 6);
                        draw_scaled(cv, &self.th, r, w.h, src, surf.w, surf.h);
                        if within(self.mouse, r) {
                            cv.round_outline(r.0 - 3, r.1 - 3, r.2 + 6, r.3 + 6, RADIUS + 3, self.th.accent, 255);
                            cv.round_outline(r.0 - 2, r.1 - 2, r.2 + 4, r.3 + 4, RADIUS + 2, self.th.accent, 255);
                        }
                        let title = gfx::ellipsize(&self.fb, &w.title, r.2);
                        let tw = gfx::text_width(&self.fb, &title);
                        cv.text(r.0 + (r.2 - tw) / 2, r.1 + r.3 + 10, &title, &self.fb, 0xFFFFFF);
                    }
                }
                continue;
            }
            if let Some(a) = w.anim {
                let r = self.anim_rect(w, &a, now);
                let t = (((now.saturating_sub(a.start)) * 1000) / a.dur.max(1)).min(1000) as i32;
                let alpha = match a.kind {
                    AnimKind::Open => (255 * ease_out(t) / 1000) as u32,
                    AnimKind::Close => (255 - 255 * ease_out(t) / 1000) as u32,
                    AnimKind::Minimize => (255 - 180 * ease_in_out(t) / 1000) as u32,
                    AnimKind::Restore => (75 + 180 * ease_out(t) / 1000) as u32,
                    AnimKind::Zoom => 255,
                };
                if !meets(bx((r.0 - 50, r.1 - 50, r.2 + 100, r.3 + 100)), clip) {
                    continue;
                }
                let old = cv.opacity;
                cv.opacity = alpha;
                cv.drop_shadow(r.0, r.1, r.2, r.3, RADIUS, 28, 120, 10);
                match &w.snap {
                    Some((buf, bw, bh)) => cv.blit_scaled(r, buf, *bw, *bh, false, false),
                    None => {
                        if r.2 == w.w && r.3 == w.h {
                            draw_frame(cv, &self.th, &self.fb, r, &w.title, is_front, None, Some((src, surf.w, surf.h)), w.maximized);
                        } else {
                            draw_scaled(cv, &self.th, r, w.h, src, surf.w, surf.h);
                        }
                    }
                }
                cv.opacity = old;
                continue;
            }
            if w.dying {
                continue;
            }
            let r = w.rect();
            if !meets(bx((r.0 - 40, r.1 - 30, r.2 + 80, r.3 + 80)), clip) {
                continue;
            }
            cv.drop_shadow(r.0, r.1, r.2, r.3, RADIUS, if is_front { 32 } else { 20 }, if is_front { 140 } else { 90 }, if is_front { 12 } else { 6 });
            let hover = if within(self.mouse, (r.0, r.1, 80, TITLE_H)) { Some(self.mouse) } else { None };
            draw_frame(cv, &self.th, &self.fb, r, &w.title, is_front, hover, Some((src, surf.w, surf.h)), w.maximized);
            if w.captured {
                let msg = "Mouse captured. Press Super to release.";
                let tw = gfx::text_width(&self.f, msg) + 24;
                let (mx, my) = (r.0 + (r.2 - tw) / 2, r.1 + TITLE_H + 10);
                cv.round_a(mx, my, tw, 28, 14, 0x000000, 170);
                cv.text(mx + 12, my + 5, msg, &self.f, 0xFFFFFF);
            }
        }
        if let Some(t) = self.snap_preview {
            cv.round_a(t.0, t.1, t.2, t.3, RADIUS, self.th.accent, 45);
            cv.round_outline(t.0, t.1, t.2, t.3, RADIUS, self.th.accent, 200);
        }
    }

    fn drag_is_move(&self) -> bool {
        matches!(self.drag, Drag::Move { .. })
    }

    fn notif_rect(&self, k: usize, now: u64) -> Rect {
        let n = &self.notifs[k];
        let age = now.saturating_sub(n.at) as i32;
        let slide = if age < 400 {
            380 - 380 * ease_back(age * 1000 / 400) / 1000
        } else if age > 4800 {
            (age - 4800) * 380 / 400
        } else {
            0
        };
        (self.sw - 360 + slide, MENU_H + 10 + k as i32 * 82, 350, 72)
    }
}

fn isqrt_i(n: i32) -> i32 {
    gfx::isqrt(n.max(0) as i64) as i32
}

struct Surf {
    px: *const u32,
    w: usize,
    h: usize,
}

unsafe impl Sync for Surf {}
unsafe impl Send for Surf {}

#[derive(Clone, Copy)]
enum DockItem {
    App(AppId, Rect, usize),
    Sep(i32),
    Trash(Rect, usize),
}

fn scale_about(r: Rect, s: i32) -> Rect {
    let (w, h) = (r.2 * s / 1000, r.3 * s / 1000);
    (r.0 + (r.2 - w) / 2, r.1 + (r.3 - h) / 2, w, h)
}

/// A window drawn at another size (Mission Control, animations).
fn draw_scaled(cv: &mut Canvas, th: &Theme, r: Rect, full_h: i32, src: &[u32], sw: usize, sh: usize) {
    let s_title = (TITLE_H * r.3 / full_h.max(1)).max(4);
    let rad = (RADIUS * r.3 / full_h.max(1)).max(4);
    cv.round(r.0, r.1, r.2, r.3, rad, th.titlebar);
    cv.blit_scaled((r.0, r.1 + s_title, r.2, r.3 - s_title - rad / 2), src, sw, sh, true, false);
    cv.round_outline(r.0, r.1, r.2, r.3, rad, if th.dark { 0xFFFFFF } else { 0x000000 }, 40);
}

/// A window's frame and picture: rounded, anti-aliased corners and a title bar.
fn draw_frame(cv: &mut Canvas, th: &Theme, f: &Font, r: Rect, title: &str, focused: bool, hover: Option<(i32, i32)>, content: Option<(&[u32], usize, usize)>, maximized: bool) {
    let rad = if maximized { 8 } else { RADIUS };
    let tb = if focused { th.titlebar } else { gfx::mix(th.titlebar, th.window, 140) };
    cv.push((r.0, r.1, r.2, TITLE_H), false);
    cv.round(r.0, r.1, r.2, TITLE_H + rad + 2, rad, tb);
    cv.pop();
    if let Some((src, sw, sh)) = content {
        let (cx, cy) = (r.0, r.1 + TITLE_H);
        let ch = (r.3 - TITLE_H).max(0) as usize;
        let cw = r.2 as usize;
        let (vw, vh) = (cw.min(sw), ch.min(sh));
        let table = gfx::corner_table(rad);
        let op = cv.opacity;
        for row in 0..vh {
            let y = cy + row as i32;
            let from_bottom = ch as i32 - 1 - row as i32;
            let line = &src[row * sw..row * sw + vw];
            if from_bottom >= rad {
                cv.blit(cx, y, line, vw, 1);
                continue;
            }
            let t = &table[(from_bottom * rad) as usize..((from_bottom + 1) * rad) as usize];
            let mut full = rad as usize;
            for (i, &c) in t.iter().enumerate() {
                if c == 255 {
                    full = i;
                    break;
                }
            }
            if vw > 2 * full {
                cv.blit(cx + full as i32, y, &line[full..vw - full], vw - 2 * full, 1);
            }
            for i in 0..full.min(vw) {
                let cov = t[i] as u32;
                if cov > 0 {
                    cv.put(cx + i as i32, y, line[i], cov * op / 255);
                    let j = vw - 1 - i;
                    cv.put(cx + j as i32, y, line[j], cov * op / 255);
                }
            }
        }
        if vh < ch {
            cv.rect(cx, cy + vh as i32, r.2, (ch - vh) as i32 - rad, th.window);
        }
    }
    cv.hline(r.0, r.1 + TITLE_H - 1, r.2, th.separator, if th.dark { 255 } else { 200 });
    cv.round_outline(r.0, r.1, r.2, r.3, rad, if th.dark { 0xFFFFFF } else { 0x000000 }, if th.dark { 36 } else { 50 });
    let cy = r.1 + TITLE_H / 2;
    let hovering = hover.map(|p| within(p, (r.0 + 6, r.1, 70, TITLE_H))).unwrap_or(false);
    let colors = if focused || hovering { [0xFF5F57, 0xFEBC2E, 0x28C840] } else if th.dark { [0x4A4B51; 3] } else { [0xCDCDD2; 3] };
    for (k, c) in colors.iter().enumerate() {
        let cx = r.0 + 20 + k as i32 * 20;
        cv.circle(cx, cy, 7, gfx::darken(*c, 28));
        cv.circle(cx, cy, 6, *c);
        if hovering {
            match k {
                0 => {
                    cv.line16((cx - 3) * 16 + 8, (cy - 3) * 16 + 8, (cx + 3) * 16 + 8, (cy + 3) * 16 + 8, 0x4D0D08, 230);
                    cv.line16((cx + 3) * 16 + 8, (cy - 3) * 16 + 8, (cx - 3) * 16 + 8, (cy + 3) * 16 + 8, 0x4D0D08, 230);
                }
                1 => cv.rect(cx - 3, cy, 7, 1, 0x6B4000),
                _ => {
                    cv.rect(cx - 3, cy, 7, 1, 0x0A4F12);
                    cv.rect(cx, cy - 3, 1, 7, 0x0A4F12);
                }
            }
        }
    }
    let title = gfx::ellipsize(f, title, (r.2 - 170).max(20));
    let tw = gfx::text_width(f, &title);
    cv.text(r.0 + (r.2 - tw) / 2, r.1 + (TITLE_H - f.height() as i32) / 2 - 1, &title, f, if focused { th.text } else { th.text3 });
}

// ---- drawn on top, by one core -----------------------------------------------------

fn paint_top(d: &Desktop, cv: &mut Canvas, clip: Bx) {
    let th = d.th;
    let now = d.frame_now;
    if clip.1 < MENU_H {
        paint_menubar(d, cv);
    }
    if meets(d.dock_zone(), clip) && d.launchpad.is_none() {
        paint_dock(d, cv);
    }
    if SHOW_BUDDY.load(Ordering::Relaxed) && d.mission.is_none() && d.launchpad.is_none() && meets(d.buddy_area(), clip) {
        let (bar, _) = d.dock_layout();
        d.buddy.draw(cv, bar, &th, &d.f);
    }
    for (k, n) in d.notifs.iter().enumerate() {
        let r = d.notif_rect(k, now);
        if !meets(bx((r.0 - 20, r.1 - 20, r.2 + 40, r.3 + 40)), clip) {
            continue;
        }
        cv.shadow(r.0, r.1 + 4, r.2, r.3, 14, 20, 100);
        cv.blur(r.0, r.1, r.2, r.3, 8);
        cv.round_a(r.0, r.1, r.2, r.3, 14, th.menu, 225);
        cv.round_outline(r.0, r.1, r.2, r.3, 14, th.menu_border, 160);
        icons::draw(cv, Icon::Buddy, r.0 + 12, r.1 + 16, 38);
        cv.text(r.0 + 62, r.1 + 13, &gfx::ellipsize(&d.fb, &n.title, r.2 - 74), &d.fb, th.text);
        cv.text(r.0 + 62, r.1 + 37, &gfx::ellipsize(&d.f, &n.body, r.2 - 74), &d.f, th.text2);
    }
    match d.panel {
        Panel::Control => paint_control(d, cv),
        Panel::Notifications => paint_notification_centre(d, cv),
        Panel::None => {}
    }
    if let Some(m) = &d.menu {
        let (frame, rows) = ui::menu_layout(&d.f, m.x, m.y, &m.items, d.sw, d.sh);
        if meets(bx((frame.0 - 30, frame.1 - 30, frame.2 + 60, frame.3 + 60)), clip) {
            cv.blur(frame.0, frame.1, frame.2, frame.3, 8);
            ui::draw_menu(cv, &th, &d.f, frame, &rows, &m.items, m.hover, now.saturating_sub(m.opened));
        }
    }
    if let Some((t, q)) = &d.launchpad {
        let e = ease_out((((now.saturating_sub(*t)) * 1000) / 280).min(1000) as i32);
        if d.lp_blur.len() == (d.sw * d.sh) as usize {
            let old = cv.opacity;
            cv.opacity = (e * 255 / 1000) as u32;
            cv.blit(0, 0, &d.lp_blur, d.sw as usize, d.sh as usize);
            cv.opacity = old;
        }
        let fw = 260;
        let fr = ((d.sw - fw) / 2, 64, fw, 32);
        cv.round_a(fr.0, fr.1, fr.2, fr.3, 9, 0xFFFFFF, 45);
        let shown = if q.is_empty() { "Search" } else { q.as_str() };
        cv.text_center(fr.0 + fr.2 / 2, fr.1 + 7, shown, &d.f, if q.is_empty() { 0xCFD3DA } else { 0xFFFFFF });
        let s = 850 + 150 * e / 1000;
        let old = cv.opacity;
        cv.opacity = (e * 255 / 1000) as u32;
        for (k, a) in d.lp_items().iter().enumerate() {
            let c = d.lp_cell(k);
            let size = 72 * s / 1000;
            let hover = within(d.mouse, c);
            if hover {
                cv.round_a(c.0 + 14, c.1, c.2 - 28, c.3 - 8, 16, 0xFFFFFF, 30);
            }
            icons::draw(cv, a.icon, c.0 + (c.2 - size) / 2, c.1 + 8 + (72 - size) / 2, size);
            let name = gfx::ellipsize(&d.f, a.name, c.2 - 8);
            let tw = gfx::text_width(&d.f, &name);
            cv.text(c.0 + (c.2 - tw) / 2, c.1 + 88, &name, &d.f, 0xFFFFFF);
        }
        cv.opacity = old;
    }
    if d.mission.is_some() && clip.1 < MENU_H + 50 {
        cv.text_center(d.sw / 2, MENU_H + 16, "Mission Control", &d.fbig, 0xFFFFFF);
        if d.wins.iter().all(|w| w.minimized || w.dying) {
            cv.text_center(d.sw / 2, d.sh / 2, "No open windows", &d.fbig, 0xFFFFFF);
        }
    }
    if let Some((sel, _)) = d.switcher {
        let list = d.switch_list();
        let n = list.len() as i32;
        let (cell, pad) = (92, 18);
        let w = n * cell + pad * 2;
        let r = ((d.sw - w) / 2, d.sh / 2 - 74, w, 148);
        cv.shadow(r.0, r.1 + 6, r.2, r.3, 20, 28, 120);
        cv.blur(r.0, r.1, r.2, r.3, 10);
        cv.round_a(r.0, r.1, r.2, r.3, 20, th.menu, 225);
        for (k, id) in list.iter().enumerate() {
            if let Some(i) = d.win_index(*id) {
                let x = r.0 + pad + k as i32 * cell;
                if k == sel {
                    cv.round_a(x, r.1 + 14, cell - 4, cell, 14, th.text, 40);
                }
                icons::draw(cv, apps::info(d.wins[i].app).icon, x + 12, r.1 + 24, 64);
                if k == sel {
                    let t = gfx::ellipsize(&d.f, &d.wins[i].title, 220);
                    cv.text_center(r.0 + r.2 / 2, r.1 + r.3 - 28, &t, &d.f, th.text);
                }
            }
        }
    }
    if let Some(s) = &d.spot {
        paint_spotlight(d, cv, s);
    }
    if !d.wins.iter().any(|w| w.captured && !w.minimized) {
        draw_cursor(cv, d.mouse, if d.drag_is_move() { Cursor::Arrow } else { d.cursor });
    }
}

fn paint_menubar(d: &Desktop, cv: &mut Canvas) {
    let th = d.th;
    cv.blur_fast(0, 0, d.sw, MENU_H, 10);
    cv.rect_a(0, 0, d.sw, MENU_H, th.bar_tint, th.bar_alpha);
    cv.hline(0, MENU_H - 1, d.sw, 0x000000, 40);
    let fg = th.text;
    let open = d.menu.as_ref().map(|m| m.kind);
    for (kind, label, x, w) in d.menubar_items() {
        if Some(kind) == open {
            cv.round_a(x - 10, 3, w, MENU_H - 6, 5, th.text, 40);
        }
        if kind == MenuKind::Logo {
            let (cx, cy) = (x + 6, MENU_H / 2);
            cv.circle(cx, cy, 8, fg);
            cv.circle(cx, cy, 6, gfx::mix(th.bar_tint, fg, 20));
            cv.line16((cx - 4) * 16 + 8, (cy + 4) * 16 + 8, (cx + 4) * 16 + 8, (cy - 4) * 16 + 8, fg, 255);
            continue;
        }
        let f = if kind == MenuKind::App { &d.fb } else { &d.f };
        cv.text(x, (MENU_H - f.height() as i32) / 2 - 1, &label, f, fg);
    }
    let st = d.status_items();
    let (h, m, _, day, month, _) = d.clock;
    let months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let mn = months.get((month as usize).wrapping_sub(1)).copied().unwrap_or("");
    let wd = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"][weekday(d.clock.5 as i32, month as i32, day as i32) as usize];
    let clock = if CLOCK_24H.load(Ordering::Relaxed) {
        alloc::format!("{} {} {}  {:02}:{:02}", wd, mn, day, h, m)
    } else {
        let h12 = if h % 12 == 0 { 12 } else { h % 12 };
        alloc::format!("{} {} {}  {}:{:02} {}", wd, mn, day, h12, m, if h < 12 { "AM" } else { "PM" })
    };
    cv.text_right(st[0].0 + st[0].1, (MENU_H - d.f.height() as i32) / 2 - 1, &clock, &d.f, fg);
    // control centre
    let (x, cy) = (st[1].0 + 8, MENU_H / 2);
    cv.round_outline(x, cy - 7, 15, 6, 3, fg, 255);
    cv.circle(x + 11, cy - 4, 2, fg);
    cv.round_outline(x, cy + 1, 15, 6, 3, fg, 255);
    cv.circle(x + 4, cy + 4, 2, fg);
    // spotlight
    let (sx, sy) = (st[2].0 + 13, MENU_H / 2 - 2);
    cv.circle(sx, sy, 6, fg);
    cv.circle(sx, sy, 4, gfx::mix(th.bar_tint, fg, 25));
    cv.thick_line(sx + 4, sy + 4, sx + 8, sy + 8, 2, fg);
    // network
    let up = crate::net::is_up();
    for k in 0..3 {
        let bh = 4 + k * 3;
        cv.rect(st[3].0 + 8 + k * 5, MENU_H / 2 + 6 - bh, 3, bh, if up { fg } else { gfx::mix(fg, th.bar_tint, 160) });
    }
    // volume
    let (vx, vy) = (st[4].0 + 8, MENU_H / 2);
    cv.rect(vx, vy - 3, 4, 6, fg);
    for i in 0..5 {
        cv.rect(vx + 4 + i, vy - 3 - i, 1, 6 + 2 * i, fg);
    }
    if crate::audio::VOLUME.load(Ordering::Relaxed) > 30 {
        cv.circle(vx + 12, vy, 3, fg);
        cv.circle(vx + 12, vy, 2, gfx::mix(th.bar_tint, fg, 25));
    }
}

fn paint_spotlight(d: &Desktop, cv: &mut Canvas, s: &Spotlight) {
    let th = d.th;
    let now = d.frame_now;
    let hits = d.spot_hits();
    let (x, y, w) = d.spot_box();
    let e = ease_out((((now.saturating_sub(s.opened)) * 1000) / 200).min(1000) as i32);
    let h = 58 + if hits.is_empty() { 0 } else { hits.len() as i32 * 44 + 12 };
    let yy = y - 14 + 14 * e / 1000;
    let old = cv.opacity;
    cv.opacity = (e * 255 / 1000).max(30) as u32;
    cv.shadow(x, yy + 8, w, h, 16, 30, 140);
    cv.blur(x, yy, w, h, 10);
    cv.round_a(x, yy, w, h, 16, th.menu, 235);
    cv.round_outline(x, yy, w, h, 16, th.menu_border, 200);
    cv.circle(x + 28, yy + 27, 9, th.text2);
    cv.circle(x + 28, yy + 27, 7, th.menu);
    cv.thick_line(x + 34, yy + 33, x + 40, yy + 39, 2, th.text2);
    let shown = if s.query.is_empty() { "Spotlight Search" } else { s.query.as_str() };
    let qw = cv.text(x + 52, yy + 17, shown, &d.fbig, if s.query.is_empty() { th.text3 } else { th.text });
    if (now / 530) % 2 == 0 {
        cv.rect(if s.query.is_empty() { x + 52 } else { x + 53 + qw }, yy + 16, 2, 26, th.accent);
    }
    for (k, hit) in hits.iter().enumerate() {
        let r = (x + 8, yy + 62 + k as i32 * 44, w - 16, 42);
        let sel = k == s.selected;
        if sel {
            cv.round(r.0, r.1, r.2, r.3, 8, th.accent);
        }
        icons::draw(cv, hit.icon, r.0 + 8, r.1 + 5, 32);
        let (c1, c2) = if sel { (th.on_accent, th.on_accent) } else { (th.text, th.text3) };
        cv.text(r.0 + 50, r.1 + 3, &gfx::ellipsize(&d.fb, &hit.title, r.2 - 60), &d.fb, c1);
        cv.text(r.0 + 50, r.1 + 21, &gfx::ellipsize(&d.f, &hit.sub, r.2 - 60), &d.f, c2);
    }
    cv.opacity = old;
}

fn paint_dock(d: &Desktop, cv: &mut Canvas) {
    let th = d.th;
    let (bar, items) = d.dock_layout();
    let now = d.frame_now;
    cv.shadow(bar.0, bar.1 + 4, bar.2, bar.3, 18, 24, 90);
    cv.blur_fast(bar.0, bar.1, bar.2, bar.3, 12);
    cv.round_a(bar.0, bar.1, bar.2, bar.3, 18, th.bar_tint, (th.bar_alpha + 30).min(255));
    cv.round_outline(bar.0, bar.1, bar.2, bar.3, 18, if th.dark { 0xFFFFFF } else { 0x000000 }, if th.dark { 45 } else { 30 });
    let trash_full = d.trash_full;
    for it in items.iter() {
        match *it {
            DockItem::App(app, r, k) => {
                let mut dy = 0;
                if let Some((_, t)) = d.dock_bounce.iter().find(|(a, _)| *a == app) {
                    let age = now.saturating_sub(*t) as i32;
                    let amp = 20 * (900 - age).max(0) / 900;
                    dy = -(amp * icons::sin_deg((age % 450) * 180 / 450).abs() / 1000);
                }
                icons::draw(cv, apps::info(app).icon, r.0, r.1 + dy, r.2);
                if d.running(app) {
                    cv.circle(r.0 + r.2 / 2, bar.1 + bar.3 - 4, 2, th.text);
                }
                if d.dock_hover == Some(k) && d.menu.is_none() {
                    tooltip(cv, &th, &d.f, r.0 + r.2 / 2, r.1 - 10, apps::info(app).name);
                }
            }
            DockItem::Sep(x) => cv.vline(x, bar.1 + 10, bar.3 - 20, if th.dark { 0xFFFFFF } else { 0x000000 }, 50),
            DockItem::Trash(r, k) => {
                icons::draw(cv, if trash_full { Icon::TrashFull } else { Icon::Trash }, r.0, r.1, r.2);
                if d.dock_hover == Some(k) && d.menu.is_none() {
                    tooltip(cv, &th, &d.f, r.0 + r.2 / 2, r.1 - 10, "Trash");
                }
            }
        }
    }
}

fn tooltip(cv: &mut Canvas, th: &Theme, f: &Font, cx: i32, bottom: i32, text: &str) {
    let w = gfx::text_width(f, text) + 22;
    let h = f.height() as i32 + 10;
    let (x, y) = (cx - w / 2, bottom - h);
    cv.shadow(x, y + 2, w, h, 7, 8, 80);
    cv.round(x, y, w, h, 7, th.menu);
    cv.round_outline(x, y, w, h, 7, th.menu_border, 180);
    cv.text(x + 11, y + 4, text, f, th.text);
}

fn paint_control(d: &Desktop, cv: &mut Canvas) {
    let th = d.th;
    let r = d.cc_rect();
    let now = d.frame_now;
    let e = ease_out((((now.saturating_sub(d.panel_at)) * 1000) / 220).min(1000) as i32);
    let old = cv.opacity;
    cv.opacity = (e * 255 / 1000).max(40) as u32;
    let y = r.1 - 10 + 10 * e / 1000;
    let r = (r.0, y, r.2, r.3);
    cv.shadow(r.0, r.1 + 6, r.2, r.3, 18, 28, 120);
    cv.blur(r.0, r.1, r.2, r.3, 10);
    cv.round_a(r.0, r.1, r.2, r.3, 18, th.menu, 230);
    cv.round_outline(r.0, r.1, r.2, r.3, 18, th.menu_border, 180);
    let states = [theme::is_dark(), d.tiling, DND.load(Ordering::Relaxed), SHOW_BUDDY.load(Ordering::Relaxed), false, false];
    for (k, (t, label)) in d.cc_tiles().iter().enumerate() {
        let on = states[k];
        let hover = within(d.mouse, *t);
        cv.round(t.0, t.1, t.2, t.3, 12, if hover { gfx::mix(th.control, th.text, 20) } else { th.control });
        let (cx, cy) = (t.0 + 26, t.1 + t.3 / 2);
        let disc = if on { th.accent } else { gfx::mix(th.control, th.text, 40) };
        cv.circle(cx, cy, 15, disc);
        match k {
            0 | 2 => {
                cv.circle(cx, cy, 7, 0xFFFFFF);
                cv.circle(cx + 4, cy - 3, 6, disc);
            }
            1 => {
                cv.rect(cx - 7, cy - 6, 6, 12, 0xFFFFFF);
                cv.rect(cx + 1, cy - 6, 6, 5, 0xFFFFFF);
                cv.rect(cx + 1, cy + 1, 6, 5, 0xFFFFFF);
            }
            3 => {
                cv.round(cx - 7, cy - 5, 14, 10, 2, 0xFFFFFF);
                cv.rect(cx - 4, cy - 2, 2, 3, 0x222222);
                cv.rect(cx + 2, cy - 2, 2, 3, 0x222222);
            }
            4 => {
                for (dx, dy) in [(-7, -6), (1, -6), (-7, 1), (1, 1)] {
                    cv.round(cx + dx, cy + dy, 6, 5, 1, 0xFFFFFF);
                }
            }
            _ => {
                cv.rect(cx - 7, cy - 6, 14, 2, 0xFFFFFF);
                cv.rect(cx - 7, cy - 1, 14, 2, 0xFFFFFF);
                cv.rect(cx - 7, cy + 4, 14, 2, 0xFFFFFF);
            }
        }
        cv.text(t.0 + 50, t.1 + 10, label, &d.fb, th.text);
        let sub = match k {
            0 | 2 => if on { "On" } else { "Off" },
            1 => if on { "Automatic" } else { "Off" },
            3 => if on { "Visible" } else { "Hidden" },
            _ => "",
        };
        cv.text(t.0 + 50, t.1 + 30, sub, &d.f, th.text2);
    }
    let s = d.cc_slider();
    let s = (s.0, s.1 + y - d.cc_rect().1, s.2, s.3);
    cv.text(s.0, s.1 - 26, "Sound", &d.fb, th.text);
    let v = crate::audio::VOLUME.load(Ordering::Relaxed) as i32;
    cv.round(s.0, s.1 + 6, s.2, 12, 6, gfx::mix(th.control, th.text, 30));
    let fx = s.0 + s.2 * v / 100;
    cv.round(s.0, s.1 + 6, (fx - s.0).max(12), 12, 6, 0xFFFFFF);
    cv.shadow(fx - 10, s.1 + 2, 20, 20, 10, 4, 80);
    cv.circle(fx, s.1 + 12, 10, 0xFFFFFF);
    let net = match crate::net::info() {
        Some(i) if i.up => alloc::format!("Network: {}", crate::net::format_ip(i.ip)),
        Some(_) => String::from("Network: connecting..."),
        None => String::from("Network: no card"),
    };
    cv.text(r.0 + 24, r.1 + r.3 - 34, &net, &d.f, th.text2);
    cv.opacity = old;
}

fn paint_notification_centre(d: &Desktop, cv: &mut Canvas) {
    let th = d.th;
    let r = (d.sw - 370, MENU_H + 6, 360, d.sh - MENU_H - 120);
    let now = d.frame_now;
    let e = ease_out((((now.saturating_sub(d.panel_at)) * 1000) / 220).min(1000) as i32);
    let old = cv.opacity;
    cv.opacity = (e * 255 / 1000).max(40) as u32;
    let x = r.0 + 30 - 30 * e / 1000;
    cv.shadow(x, r.1 + 6, r.2, r.3, 18, 28, 120);
    cv.blur(x, r.1, r.2, r.3, 10);
    cv.round_a(x, r.1, r.2, r.3, 18, th.menu, 225);
    cv.round_outline(x, r.1, r.2, r.3, 18, th.menu_border, 180);
    let (_, _, _, day, month, year) = d.clock;
    let months = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
    let mn = months.get((month as usize).wrapping_sub(1)).copied().unwrap_or("");
    cv.text(x + 20, r.1 + 16, &alloc::format!("{} {}", mn, year), &d.fbig, th.text);
    let first = weekday(year as i32, month as i32, 1);
    for (k, l) in ["S", "M", "T", "W", "T", "F", "S"].iter().enumerate() {
        cv.text_center(x + 36 + k as i32 * 46, r.1 + 52, l, &d.fb, th.text3);
    }
    for dd in 1..=days_in(year as i32, month as i32) {
        let idx = first + dd - 1;
        let (cx, cy) = (x + 36 + (idx % 7) * 46, r.1 + 80 + (idx / 7) * 30);
        let label = alloc::format!("{}", dd);
        if dd == day as i32 {
            cv.circle(cx, cy + 9, 13, th.accent);
            cv.text_center(cx, cy, &label, &d.f, th.on_accent);
        } else {
            cv.text_center(cx, cy, &label, &d.f, th.text);
        }
    }
    let mut yy = r.1 + 280;
    cv.text(x + 20, yy, "Notifications", &d.fb, th.text);
    yy += 28;
    if d.history.is_empty() {
        cv.text(x + 20, yy, "Nothing new.", &d.f, th.text3);
    }
    for n in d.history.iter() {
        if yy + 64 > r.1 + r.3 - 10 {
            break;
        }
        cv.round(x + 12, yy, r.2 - 24, 58, 12, th.control);
        cv.text(x + 26, yy + 8, &gfx::ellipsize(&d.fb, &n.title, r.2 - 60), &d.fb, th.text);
        cv.text(x + 26, yy + 30, &gfx::ellipsize(&d.f, &n.body, r.2 - 60), &d.f, th.text2);
        yy += 66;
    }
    cv.opacity = old;
}

pub fn draw_cursor(cv: &mut Canvas, p: (i32, i32), c: Cursor) {
    let (x, y) = p;
    match c {
        Cursor::Arrow | Cursor::Move => {
            const ARROW: [&str; 17] = [
                "X          ",
                "XX         ",
                "XOX        ",
                "XOOX       ",
                "XOOOX      ",
                "XOOOOX     ",
                "XOOOOOX    ",
                "XOOOOOOX   ",
                "XOOOOOOOX  ",
                "XOOOOOOOOX ",
                "XOOOOOXXXXX",
                "XOOXOOX    ",
                "XOX XOOX   ",
                "XX  XOOX   ",
                "X    XOOX  ",
                "     XOOX  ",
                "      XX   ",
            ];
            for (ry, row) in ARROW.iter().enumerate() {
                for (rx, ch) in row.bytes().enumerate() {
                    if ch != b' ' {
                        cv.put(x + rx as i32 + 1, y + ry as i32 + 2, 0x000000, 60);
                    }
                }
            }
            for (ry, row) in ARROW.iter().enumerate() {
                for (rx, ch) in row.bytes().enumerate() {
                    match ch {
                        b'X' => cv.put(x + rx as i32, y + ry as i32, 0xFFFFFF, 255),
                        b'O' => cv.put(x + rx as i32, y + ry as i32, 0x111111, 255),
                        _ => {}
                    }
                }
            }
        }
        Cursor::Hand => {
            cv.round(x - 3, y - 1, 8, 14, 4, 0x111111);
            cv.round(x - 2, y, 6, 12, 3, 0xFFFFFF);
            cv.round(x - 7, y + 8, 18, 14, 6, 0x111111);
            cv.round(x - 6, y + 9, 16, 12, 5, 0xFFFFFF);
            cv.rect(x - 1, y + 8, 4, 3, 0xFFFFFF);
        }
        Cursor::Text => {
            cv.rect(x - 1, y - 9, 3, 19, 0xFFFFFF);
            cv.rect(x, y - 8, 1, 17, 0x111111);
            cv.rect(x - 3, y - 9, 7, 1, 0x111111);
            cv.rect(x - 3, y + 9, 7, 1, 0x111111);
        }
        _ => {
            let (dx, dy) = match c {
                Cursor::ResizeH => (1, 0),
                Cursor::ResizeV => (0, 1),
                Cursor::ResizeD1 => (1, 1),
                _ => (1, -1),
            };
            for t in -9..=9 {
                cv.circle(x + dx * t, y + dy * t, 2, 0xFFFFFF);
            }
            for t in -8..=8 {
                cv.put(x + dx * t, y + dy * t, 0x111111, 255);
            }
            for s in [-1, 1] {
                cv.circle(x + dx * 8 * s, y + dy * 8 * s, 3, 0x111111);
            }
        }
    }
}

fn weekday(y: i32, m: i32, d: i32) -> i32 {
    let t = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let y = if m < 3 { y - 1 } else { y };
    (y + y / 4 - y / 100 + y / 400 + t[((m - 1).clamp(0, 11)) as usize] + d).rem_euclid(7)
}

fn days_in(y: i32, m: i32) -> i32 {
    match m {
        2 => {
            if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 { 29 } else { 28 }
        }
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn find_files(fs: &crate::fs::Filesystem, dir: &str, q: &str, out: &mut Vec<String>, depth: usize) {
    if depth > 6 || out.len() >= 8 {
        return;
    }
    let Ok(entries) = fs.entries(dir) else { return };
    for e in entries {
        let path = if dir == "/" { alloc::format!("/{}", e.name) } else { alloc::format!("{}/{}", dir, e.name) };
        if e.name.to_ascii_lowercase().contains(q) {
            out.push(path.clone());
        }
        if e.is_dir {
            find_files(fs, &path, q, out, depth + 1);
        }
    }
}

/// A tiny calculator for Spotlight: + - * / % and parentheses, integers.
pub fn calc(s: &str) -> Option<i64> {
    let chars: Vec<char> = s.chars().filter(|c| !c.is_whitespace()).collect();
    if chars.is_empty() || !chars.iter().any(|c| "+-*/%".contains(*c)) || !chars.iter().all(|c| c.is_ascii_digit() || "+-*/%()".contains(*c)) {
        return None;
    }
    let mut pos = 0;
    let v = calc_expr(&chars, &mut pos)?;
    if pos == chars.len() { Some(v) } else { None }
}

fn calc_expr(c: &[char], p: &mut usize) -> Option<i64> {
    let mut v = calc_term(c, p)?;
    while *p < c.len() && (c[*p] == '+' || c[*p] == '-') {
        let op = c[*p];
        *p += 1;
        let r = calc_term(c, p)?;
        v = if op == '+' { v.checked_add(r)? } else { v.checked_sub(r)? };
    }
    Some(v)
}

fn calc_term(c: &[char], p: &mut usize) -> Option<i64> {
    let mut v = calc_atom(c, p)?;
    while *p < c.len() && "*/%".contains(c[*p]) {
        let op = c[*p];
        *p += 1;
        let r = calc_atom(c, p)?;
        v = match op {
            '*' => v.checked_mul(r)?,
            '/' => v.checked_div(r)?,
            _ => v.checked_rem(r)?,
        };
    }
    Some(v)
}

fn calc_atom(c: &[char], p: &mut usize) -> Option<i64> {
    if *p < c.len() && c[*p] == '(' {
        *p += 1;
        let v = calc_expr(c, p)?;
        if *p < c.len() && c[*p] == ')' {
            *p += 1;
            return Some(v);
        }
        return None;
    }
    if *p < c.len() && c[*p] == '-' {
        *p += 1;
        return calc_atom(c, p).map(|v| -v);
    }
    let start = *p;
    while *p < c.len() && c[*p].is_ascii_digit() {
        *p += 1;
    }
    if start == *p {
        return None;
    }
    c[start..*p].iter().collect::<String>().parse().ok()
}

/// (hour, minute, second, day, month, year) from the CMOS clock.
pub fn read_rtc() -> (u8, u8, u8, u8, u8, u16) {
    use x86_64::instructions::port::Port;
    let mut index: Port<u8> = Port::new(0x70);
    let mut data: Port<u8> = Port::new(0x71);
    let mut read = |reg: u8| unsafe {
        index.write(reg);
        data.read()
    };
    for _ in 0..1000 {
        if read(0x0A) & 0x80 == 0 {
            break;
        }
    }
    let (s, m, h, d, mo, y, b) = (read(0x00), read(0x02), read(0x04), read(0x07), read(0x08), read(0x09), read(0x0B));
    let bcd = b & 4 == 0;
    let conv = |v: u8| if bcd { (v & 0x0F) + (v >> 4) * 10 } else { v };
    let pm = h & 0x80 != 0;
    let mut hour = conv(h & 0x7F);
    if b & 2 == 0 && pm {
        hour = (hour + 12) % 24;
    }
    (hour, conv(m), conv(s), conv(d), conv(mo), 2000 + conv(y) as u16)
}
