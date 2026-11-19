//! System Settings, "About zero" and the Activity monitor.

use super::desktop::{self, CLOCK_24H, DOCK_SIZE, MAGNIFY, SHOW_BUDDY, TILING, WALLPAPER, WALLPAPERS, WALL_GEN, WALL_IMAGE};
use super::gfx::{self, Rect};
use super::icons::{self, Icon};
use super::theme;
use super::ui::{id, idn, App, Ui};
use crate::fs::FILESYSTEM;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::Ordering;
use spin::Mutex;

const PANES: [(&str, u32); 12] = [
    ("General", 0x8E8E93),
    ("Appearance", 0x3A3A3C),
    ("Desktop & Dock", 0x0A84FF),
    ("Wallpaper", 0x32ADE6),
    ("Displays", 0x0A84FF),
    ("Sound", 0xFF375F),
    ("Network", 0x0A84FF),
    ("Bit", 0x5E5CE6),
    ("Keyboard", 0x8E8E93),
    ("Date & Time", 0xFF9F0A),
    ("Storage", 0x8E8E93),
    ("Users", 0x30D158),
];

const SIDE: i32 = 230;
const RESOLUTIONS: [(usize, usize); 6] = [(1024, 640), (1280, 720), (1280, 800), (1440, 900), (1600, 900), (1920, 1080)];

pub struct Settings {
    pane: usize,
    search: String,
    name: String,
    ping: Arc<Mutex<String>>,
    thumbs: Vec<(String, Vec<u32>, usize, usize)>,
    thumbs_loaded: bool,
    res_choice: usize,
    last_arg: Option<String>,
}

impl Settings {
    pub fn new(arg: Option<String>) -> Settings {
        let mut s = Settings {
            pane: 0,
            search: String::new(),
            name: crate::settings::get_or("name", "friend"),
            ping: Arc::new(Mutex::new(String::new())),
            thumbs: Vec::new(),
            thumbs_loaded: false,
            res_choice: usize::MAX,
            last_arg: None,
        };
        s.apply_arg(arg);
        s
    }

    fn apply_arg(&mut self, arg: Option<String>) {
        if let Some(a) = &arg {
            if let Some(i) = PANES.iter().position(|(n, _)| n.to_ascii_lowercase().starts_with(&a.to_ascii_lowercase())) {
                self.pane = i;
            }
        }
        self.last_arg = arg;
    }
}

/// A grouped list of rows, like macOS settings.
struct Card {
    x: i32,
    y: i32,
    w: i32,
    rows: i32,
}

fn card(ui: &mut Ui, x: i32, y: i32, w: i32, rows: i32) -> Card {
    let th = ui.th;
    let bg = if th.dark { 0x2B2C31 } else { 0xFFFFFF };
    ui.cv.round_border(x, y, w, rows * 48, 10, bg, if th.dark { 0x38393F } else { 0xE0E0E4 });
    for k in 1..rows {
        ui.cv.hline(x + 14, y + k * 48, w - 28, th.separator, 255);
    }
    Card { x, y, w, rows }
}

impl Card {
    fn row(&self, k: i32) -> Rect {
        (self.x, self.y + k * 48, self.w, 48)
    }
    fn end(&self) -> i32 {
        self.y + self.rows * 48
    }
}

fn row_label(ui: &mut Ui, r: Rect, label: &str, sub: Option<&str>) {
    let (c, c2) = (ui.th.text, ui.th.text2);
    match sub {
        Some(s) => {
            ui.text(r.0 + 16, r.1 + 5, label, c);
            ui.text(r.0 + 16, r.1 + 25, s, c2);
        }
        None => {
            ui.text(r.0 + 16, r.1 + 14, label, c);
        }
    }
}

fn row_value(ui: &mut Ui, r: Rect, value: &str) {
    let f = ui.font;
    let c = ui.th.text2;
    ui.cv.text_right(r.0 + r.2 - 16, r.1 + 14, value, &f, c);
}

fn row_toggle(ui: &mut Ui, r: Rect, key: &str, value: &mut bool) -> bool {
    ui.toggle(id(key), r.0 + r.2 - 58, r.1 + 12, value)
}

fn pane_icon(ui: &mut Ui, x: i32, y: i32, color: u32, k: usize) {
    let cv = &mut ui.cv;
    cv.round(x, y, 24, 24, 6, color);
    let w = 0xFFFFFF;
    match k {
        0 => {
            for a in (0..360).step_by(45) {
                cv.circle(x + 12 + icons::cos_deg(a) * 7 / 1000, y + 12 + icons::sin_deg(a) * 7 / 1000, 2, w);
            }
            cv.circle(x + 12, y + 12, 6, w);
            cv.circle(x + 12, y + 12, 3, color);
        }
        1 => {
            cv.circle(x + 12, y + 12, 7, w);
            cv.push((x + 12, y, 12, 24), false);
            cv.circle(x + 12, y + 12, 6, color);
            cv.pop();
        }
        2 => {
            cv.round(x + 5, y + 15, 14, 4, 2, w);
            cv.round(x + 5, y + 6, 14, 7, 2, w);
        }
        3 => {
            cv.round(x + 5, y + 6, 14, 12, 2, w);
            cv.circle(x + 9, y + 10, 2, color);
        }
        4 => {
            cv.round(x + 5, y + 6, 14, 10, 2, w);
            cv.rect(x + 10, y + 16, 4, 3, w);
        }
        5 => {
            cv.rect(x + 7, y + 10, 4, 5, w);
            for i in 0..5 {
                cv.rect(x + 11 + i, y + 10 - i, 1, 5 + 2 * i, w);
            }
        }
        6 => {
            for i in 0..3 {
                cv.rect(x + 6 + i * 5, y + 16 - i * 4, 3, 3 + i * 4, w);
            }
        }
        7 => {
            cv.round(x + 5, y + 7, 14, 10, 2, w);
            cv.rect(x + 8, y + 10, 2, 3, color);
            cv.rect(x + 14, y + 10, 2, 3, color);
        }
        8 => {
            cv.round(x + 4, y + 8, 16, 9, 2, w);
            for i in 0..3 {
                cv.rect(x + 6 + i * 4, y + 10, 2, 2, color);
            }
        }
        9 => {
            cv.circle(x + 12, y + 12, 7, w);
            cv.rect(x + 12, y + 7, 1, 6, color);
            cv.rect(x + 12, y + 12, 4, 1, color);
        }
        10 => {
            cv.round(x + 5, y + 7, 14, 10, 3, w);
            cv.rect(x + 7, y + 11, 6, 2, color);
        }
        _ => {
            cv.circle(x + 12, y + 9, 4, w);
            cv.round(x + 6, y + 14, 12, 6, 3, w);
        }
    }
}

impl App for Settings {
    fn title(&self) -> String {
        String::from(PANES[self.pane].0)
    }

    fn frame(&mut self, ui: &mut Ui) {
        if let Some(a) = super::app_arg() {
            if Some(&a) != self.last_arg.as_ref() {
                self.apply_arg(Some(a));
            }
        }
        let th = ui.th;
        let (w, h) = (ui.w, ui.h);
        ui.fill(if th.dark { 0x1E1F23 } else { 0xF2F2F5 });
        ui.cv.rect(0, 0, SIDE, h, th.sidebar);
        ui.cv.vline(SIDE, 0, h, th.separator, 255);
        ui.field(id("search"), (12, 12, SIDE - 24, 30), &mut self.search, "Search");
        let name = self.name.clone();
        let initial: String = name.chars().next().map(|c| c.to_ascii_uppercase()).into_iter().collect();
        ui.cv.gradient(16, 56, 44, 44, 22, 0x8E8EFF, 0x5E5CE6);
        let bf = ui.big;
        ui.cv.text_center(38, 66, &initial, &bf, 0xFFFFFF);
        ui.text_bold(70, 60, &name, th.text);
        ui.muted(70, 80, "zero account");
        ui.cv.hline(12, 112, SIDE - 24, th.separator, 255);
        let q = self.search.to_ascii_lowercase();
        let mut y = 122;
        for (k, (label, color)) in PANES.iter().enumerate() {
            if !q.is_empty() && !label.to_ascii_lowercase().contains(&q) {
                continue;
            }
            let r = (8, y, SIDE - 16, 32);
            let sel = self.pane == k;
            if ui.flat(idn("pane", k), r, sel).clicked {
                self.pane = k;
            }
            pane_icon(ui, r.0 + 8, r.1 + 4, *color, k);
            ui.text(r.0 + 42, r.1 + 7, label, if sel { th.on_accent } else { th.text });
            y += 34;
        }
        let x = SIDE + 28;
        let cw = (w - x - 28).min(620);
        let tf = ui.title;
        ui.cv.text(x, 18, PANES[self.pane].0, &tf, th.text);
        let top = 64;
        match self.pane {
            0 => general(ui, x, top, cw),
            1 => appearance(ui, x, top, cw),
            2 => dock_pane(ui, x, top, cw),
            3 => self.wallpaper(ui, x, top, cw),
            4 => self.displays(ui, x, top, cw),
            5 => sound(ui, x, top, cw),
            6 => self.network(ui, x, top, cw),
            7 => bit_pane(ui, x, top, cw),
            8 => keyboard(ui, x, top, cw),
            9 => date_time(ui, x, top, cw),
            10 => storage(ui, x, top, cw),
            _ => self.users(ui, x, top, cw),
        }
    }
}

fn general(ui: &mut Ui, x: i32, y: i32, w: i32) {
    let th = ui.th;
    logo(ui, x + w / 2, y + 40, 40);
    let (big, f) = (ui.title, ui.font);
    ui.cv.text_center(x + w / 2, y + 92, "zero", &big, th.text);
    ui.cv.text_center(x + w / 2, y + 124, concat!("Version ", env!("CARGO_PKG_VERSION")), &f, th.text2);
    let c = card(ui, x, y + 156, w, 5);
    let (dw, dh) = crate::graphics::framebuffer::with_screen(|f| {
        use crate::graphics::display::Display;
        (f.width(), f.height())
    })
    .unwrap_or((0, 0));
    let (hh, mm, ss) = crate::time::split_hms(crate::time::uptime_secs());
    let rows = [
        ("Processors", alloc::format!("{} cores", crate::smp::cpu_count())),
        ("Memory", alloc::format!("{} MB", crate::allocator::HEAP_SIZE / (1024 * 1024))),
        ("Display", alloc::format!("{} x {}", dw, dh)),
        ("Uptime", alloc::format!("{:02}:{:02}:{:02}", hh, mm, ss)),
        ("Software Update", String::from("zero is up to date")),
    ];
    for (k, (a, b)) in rows.iter().enumerate() {
        row_label(ui, c.row(k as i32), a, None);
        row_value(ui, c.row(k as i32), b);
    }
    ui.animate(1000);
}

pub fn logo(ui: &mut Ui, cx: i32, cy: i32, r: i32) {
    icons::zero_logo(&mut ui.cv, cx, cy, r);
}

impl Settings {
    fn wallpaper(&mut self, ui: &mut Ui, x: i32, y: i32, _w: i32) {
        let th = ui.th;
        if !self.thumbs_loaded {
            self.thumbs_loaded = true;
            let pics: Vec<String> = FILESYSTEM.lock().as_ref().and_then(|fs| fs.entries("/Pictures").ok()).unwrap_or_default().into_iter().filter(|e| !e.is_dir).map(|e| super::files::join("/Pictures", &e.name)).collect();
            for p in pics.into_iter().take(8) {
                if let Some(img) = super::media::load_image(&p) {
                    let (tw, tht) = (132usize, 82usize);
                    let mut out = alloc::vec![0u32; tw * tht];
                    let mut cv = gfx::Canvas::new(&mut out, tw, tht);
                    let s = ((tw * 1000 / img.w.max(1)).max(tht * 1000 / img.h.max(1))) as i32;
                    let (dw, dh) = (img.w as i32 * s / 1000, img.h as i32 * s / 1000);
                    cv.blit_scaled(((tw as i32 - dw) / 2, (tht as i32 - dh) / 2, dw, dh), &img.px, img.w, img.h, true, false);
                    self.thumbs.push((p, out, tw, tht));
                }
            }
        }
        ui.text_bold(x, y, "Dynamic wallpapers", th.text);
        let current_img = WALL_IMAGE.lock().clone();
        let colors = [(0x4C6EF5, 0xC056A0), (0x1E90FF, 0x00D1B2), (0x3FA65D, 0x1F4A2E), (0x50555F, 0x2C2F37), (0xFFB45E, 0xC25B5B), (0x23D5AB, 0x0B1E33)];
        let f = ui.font;
        for (k, name) in WALLPAPERS.iter().enumerate() {
            let r = (x + (k as i32 % 4) * 146, y + 28 + (k as i32 / 4) * 118, 132, 82);
            if ui.interact(idn("wp", k), r).clicked {
                WALLPAPER.store(k, Ordering::Relaxed);
                *WALL_IMAGE.lock() = None;
                crate::settings::set("wallpaper", &alloc::format!("{}", k));
                crate::settings::set("wall_image", "");
                WALL_GEN.fetch_add(1, Ordering::Relaxed);
            }
            ui.cv.shadow(r.0, r.1 + 3, r.2, r.3, 10, 8, 70);
            ui.cv.gradient(r.0, r.1, r.2, r.3, 10, colors[k].0, colors[k].1);
            if current_img.is_none() && WALLPAPER.load(Ordering::Relaxed) == k {
                ui.cv.round_outline(r.0 - 3, r.1 - 3, r.2 + 6, r.3 + 6, 13, th.accent, 255);
                ui.cv.round_outline(r.0 - 2, r.1 - 2, r.2 + 4, r.3 + 4, 12, th.accent, 255);
            }
            ui.cv.text_center(r.0 + r.2 / 2, r.1 + r.3 + 6, name, &f, th.text2);
        }
        let py = y + 28 + 2 * 118 + 10;
        ui.text_bold(x, py, "Your pictures", th.text);
        if self.thumbs.is_empty() {
            ui.muted(x, py + 28, "Put pictures in the Pictures folder to use them here.");
        }
        let thumbs = core::mem::take(&mut self.thumbs);
        for (k, (path, px, tw, tht)) in thumbs.iter().enumerate() {
            let r = (x + (k as i32 % 4) * 146, py + 28 + (k as i32 / 4) * 104, *tw as i32, *tht as i32);
            if ui.interact(idn("pic", k), r).clicked {
                *WALL_IMAGE.lock() = Some(path.clone());
                crate::settings::set("wall_image", path);
                WALL_GEN.fetch_add(1, Ordering::Relaxed);
            }
            ui.cv.shadow(r.0, r.1 + 3, r.2, r.3, 6, 8, 70);
            ui.cv.blit(r.0, r.1, px, *tw, *tht);
            if current_img.as_deref() == Some(path.as_str()) {
                ui.cv.round_outline(r.0 - 3, r.1 - 3, r.2 + 6, r.3 + 6, 9, th.accent, 255);
                ui.cv.round_outline(r.0 - 2, r.1 - 2, r.2 + 4, r.3 + 4, 8, th.accent, 255);
            }
        }
        self.thumbs = thumbs;
    }

    fn displays(&mut self, ui: &mut Ui, x: i32, y: i32, w: i32) {
        let th = ui.th;
        let (pw, ph, lw, lh) = crate::graphics::framebuffer::with_screen(|f| {
            use crate::graphics::display::Display;
            let p = f.physical();
            (p.0, p.1, f.width(), f.height())
        })
        .unwrap_or((0, 0, 0, 0));
        let (mw, mh) = (200, 124);
        let mx = x + (w - mw) / 2;
        ui.cv.round(mx, y, mw, mh, 8, 0x2A2B30);
        ui.cv.gradient(mx + 6, y + 6, mw - 12, mh - 12, 4, 0x4C6EF5, 0xC056A0);
        ui.cv.rect(mx + mw / 2 - 20, y + mh, 40, 14, 0x8A8B90);
        ui.cv.round(mx + mw / 2 - 50, y + mh + 14, 100, 6, 3, 0x8A8B90);
        let c = card(ui, x, y + 170, w, 2);
        row_label(ui, c.row(0), "Screen", None);
        row_value(ui, c.row(0), &alloc::format!("{} x {}", pw, ph));
        row_label(ui, c.row(1), "Desktop resolution", None);
        row_value(ui, c.row(1), &alloc::format!("{} x {}", lw, lh));
        ui.text_bold(x, c.end() + 20, "Resolution", th.text);
        let saved = crate::settings::get_or("desk_res", "");
        if self.res_choice == usize::MAX {
            self.res_choice = RESOLUTIONS.iter().position(|(a, b)| alloc::format!("{}x{}", a, b) == saved).unwrap_or(99);
        }
        let f = ui.font;
        for (k, (a, b)) in RESOLUTIONS.iter().enumerate() {
            let fits = *a <= pw && *b <= ph;
            let r = (x + (k as i32 % 3) * (w / 3), c.end() + 48 + (k as i32 / 3) * 42, w / 3 - 10, 34);
            let label = alloc::format!("{} x {}", a, b);
            if fits {
                if ui.button_ex(idn("res", k), r, &label, self.res_choice == k) {
                    self.res_choice = k;
                    crate::settings::set("desk_res", &alloc::format!("{}x{}", a, b));
                }
            } else {
                ui.cv.round(r.0, r.1, r.2, r.3, 6, th.control);
                ui.cv.text_center(r.0 + r.2 / 2, r.1 + 8, &label, &f, th.text3);
            }
        }
        let by = c.end() + 146;
        ui.muted(x, by, "The new resolution is used after a restart.");
        if self.res_choice != 99 && ui.primary((x + w - 150, by - 6, 150, 32), "Restart Now") {
            crate::pfs::sync_if_dirty();
            crate::power::reboot();
        }
    }

    fn network(&mut self, ui: &mut Ui, x: i32, y: i32, w: i32) {
        let th = ui.th;
        let info = crate::net::info();
        let up = info.as_ref().map(|i| i.up).unwrap_or(false);
        let c = card(ui, x, y, w, 1);
        ui.cv.circle(c.x + 24, c.y + 24, 6, if up { 0x30D158 } else if info.is_some() { 0xFF9F0A } else { 0xFF453A });
        ui.text_bold(c.x + 40, c.y + 14, "Ethernet", th.text);
        row_value(ui, c.row(0), if up { "Connected" } else if info.is_some() { "Connecting..." } else { "No network card" });
        if let Some(i) = info {
            let c2 = card(ui, x, c.end() + 20, w, 6);
            let m = i.mac;
            let rows = [
                ("IP address", crate::net::format_ip(i.ip)),
                ("Configured", String::from(if i.dhcp { "Automatically (DHCP)" } else { "Manually" })),
                ("Router", crate::net::format_ip(i.gateway)),
                ("DNS server", crate::net::format_ip(i.dns)),
                ("Hardware address", alloc::format!("{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}", m[0], m[1], m[2], m[3], m[4], m[5])),
                ("Packets", alloc::format!("{} in, {} out", i.rx, i.tx)),
            ];
            for (k, (a, b)) in rows.iter().enumerate() {
                row_label(ui, c2.row(k as i32), a, None);
                row_value(ui, c2.row(k as i32), b);
            }
            let by = c2.end() + 20;
            if ui.button((x, by, 160, 32), "Test Connection") {
                let slot = self.ping.clone();
                *slot.lock() = String::from("Pinging example.com...");
                crate::sched::spawn("ping", 128, move || {
                    let msg = match crate::net::resolve("example.com") {
                        Ok(ip) => match crate::net::ping(ip) {
                            Some(ms) => alloc::format!("example.com answered in {} ms", ms),
                            None => String::from("example.com did not answer"),
                        },
                        Err(e) => alloc::format!("{}", e),
                    };
                    *slot.lock() = msg;
                });
            }
            let msg = self.ping.lock().clone();
            ui.muted(x + 176, by + 7, &msg);
        }
        ui.animate(500);
    }

    fn users(&mut self, ui: &mut Ui, x: i32, y: i32, w: i32) {
        let th = ui.th;
        let c = card(ui, x, y, w, 2);
        row_label(ui, c.row(0), "Your name", None);
        let before = self.name.clone();
        ui.field(id("name"), (c.x + w - 236, c.y + 9, 220, 30), &mut self.name, "name");
        if self.name != before && !self.name.trim().is_empty() {
            crate::settings::set("name", self.name.trim());
            crate::graphics::buddy::set_name(self.name.trim());
        }
        row_label(ui, c.row(1), "Account type", None);
        row_value(ui, c.row(1), "Administrator");
        ui.text_bold(x, c.end() + 24, "Setup", th.text);
        ui.muted(x, c.end() + 46, "Walk through the welcome setup again (after a restart).");
        if ui.button((x, c.end() + 74, 200, 32), "Run Setup Again") {
            crate::settings::set("desk_setup", "");
            super::notify("Setup", "The welcome setup will run on the next start.");
        }
    }
}

fn appearance(ui: &mut Ui, x: i32, y: i32, w: i32) {
    let th = ui.th;
    let dark = theme::is_dark();
    let f = ui.font;
    for (k, (label, d)) in [("Light", false), ("Dark", true)].iter().enumerate() {
        let r = (x + k as i32 * 170, y, 150, 100);
        if ui.interact(idn("mode", k), r).clicked && dark != *d {
            theme::set_dark(*d);
            theme::save();
        }
        let (bg, bar, win) = if *d { (0x3A2E5E, 0x1C1D21, 0x2B2C31) } else { (0xB9C6F5, 0xE9E9EC, 0xFFFFFF) };
        ui.cv.shadow(r.0, r.1 + 3, r.2, r.3, 10, 8, 70);
        ui.cv.round(r.0, r.1, r.2, r.3, 10, bg);
        ui.cv.rect(r.0 + 6, r.1 + 4, r.2 - 12, 8, bar);
        ui.cv.round(r.0 + 22, r.1 + 26, r.2 - 44, r.3 - 40, 6, win);
        ui.cv.round(r.0 + 22, r.1 + 26, r.2 - 44, 10, 5, bar);
        if dark == *d {
            ui.cv.round_outline(r.0 - 3, r.1 - 3, r.2 + 6, r.3 + 6, 13, th.accent, 255);
            ui.cv.round_outline(r.0 - 2, r.1 - 2, r.2 + 4, r.3 + 4, 12, th.accent, 255);
        }
        ui.cv.text_center(r.0 + r.2 / 2, r.1 + r.3 + 8, label, &f, th.text);
    }
    let c = card(ui, x, y + 150, w, 1);
    row_label(ui, c.row(0), "Accent colour", None);
    let n = theme::ACCENTS.len() as i32;
    for (k, (_, col)) in theme::ACCENTS.iter().enumerate() {
        let (cx, cy) = (c.x + w - 30 - (n - 1 - k as i32) * 28, c.y + 24);
        if ui.interact(idn("accent", k), (cx - 11, cy - 11, 22, 22)).clicked {
            theme::set_accent(*col);
            theme::save();
        }
        ui.cv.circle(cx, cy, 10, *col);
        if theme::accent() == *col {
            ui.cv.circle(cx, cy, 4, 0xFFFFFF);
        }
    }
}

fn dock_pane(ui: &mut Ui, x: i32, y: i32, w: i32) {
    let c = card(ui, x, y, w, 2);
    row_label(ui, c.row(0), "Size", None);
    let mut size = DOCK_SIZE.load(Ordering::Relaxed) as i32;
    if ui.slider(id("docksize"), (c.x + w - 236, c.y + 12, 210, 24), &mut size, 32, 80) {
        DOCK_SIZE.store(size as u32, Ordering::Relaxed);
        crate::settings::set("dock_size", &alloc::format!("{}", size));
    }
    row_label(ui, c.row(1), "Magnification", None);
    let mut m = MAGNIFY.load(Ordering::Relaxed);
    if row_toggle(ui, c.row(1), "mag", &mut m) {
        MAGNIFY.store(m, Ordering::Relaxed);
        desktop::save_flag("magnify", m);
    }
    let c2 = card(ui, x, c.end() + 20, w, 3);
    row_label(ui, c2.row(0), "Tile windows automatically", Some("Like a tiling window manager (Super+T)"));
    let mut t = TILING.load(Ordering::Relaxed);
    if row_toggle(ui, c2.row(0), "tile", &mut t) {
        TILING.store(t, Ordering::Relaxed);
        desktop::save_flag("tiling", t);
    }
    row_label(ui, c2.row(1), "Show Bit on the dock", None);
    let mut b = SHOW_BUDDY.load(Ordering::Relaxed);
    if row_toggle(ui, c2.row(1), "bit", &mut b) {
        SHOW_BUDDY.store(b, Ordering::Relaxed);
        desktop::save_flag("show_buddy", b);
    }
    row_label(ui, c2.row(2), "Hot corner", Some("Top left corner opens Mission Control"));
    let th = ui.th;
    ui.text_bold(x, c2.end() + 24, "Window snapping", th.text);
    ui.wrapped(x, c2.end() + 48, w, "Drag a window to the left or right edge to fill half the screen, to a corner for a quarter, or to the top to fill it. Double-click a title bar to zoom.", th.text2);
}

fn sound(ui: &mut Ui, x: i32, y: i32, w: i32) {
    let c = card(ui, x, y, w, 3);
    row_label(ui, c.row(0), "Output volume", None);
    let mut v = crate::audio::VOLUME.load(Ordering::Relaxed) as i32;
    if ui.slider(id("vol"), (c.x + w - 236, c.y + 12, 210, 24), &mut v, 0, 100) {
        crate::audio::VOLUME.store(v as u32, Ordering::Relaxed);
    }
    if ui.inp.released {
        crate::settings::set("volume", &alloc::format!("{}", crate::audio::VOLUME.load(Ordering::Relaxed)));
    }
    row_label(ui, c.row(1), "Interface sounds", Some("Window, notification and game effects"));
    let mut e = crate::audio::EFFECTS_ON.load(Ordering::Relaxed);
    if row_toggle(ui, c.row(1), "fx", &mut e) {
        crate::audio::EFFECTS_ON.store(e, Ordering::Relaxed);
        crate::settings::set("effects", if e { "1" } else { "0" });
    }
    row_label(ui, c.row(2), "Output device", None);
    row_value(ui, c.row(2), if crate::audio::available() { "Intel AC97" } else { "None found" });
    if ui.button((x, c.end() + 20, 140, 32), "Play Sound") {
        crate::audio::effect(crate::audio::Effect::Win);
    }
}

fn bit_pane(ui: &mut Ui, x: i32, y: i32, w: i32) {
    let th = ui.th;
    icons::draw(&mut ui.cv, Icon::Buddy, x, y, 64);
    let bf = ui.big;
    ui.cv.text(x + 80, y + 6, "Bit", &bf, th.text);
    ui.wrapped(x + 80, y + 34, w - 80, "Your desktop buddy. Bit learns which tricks you like and what you talk about.", th.text2);
    let c = card(ui, x, y + 90, w, 3);
    row_label(ui, c.row(0), "Show Bit", None);
    let mut b = SHOW_BUDDY.load(Ordering::Relaxed);
    if row_toggle(ui, c.row(0), "bit2", &mut b) {
        SHOW_BUDDY.store(b, Ordering::Relaxed);
        desktop::save_flag("show_buddy", b);
    }
    row_label(ui, c.row(1), "Chattiness", None);
    let mut chat = super::buddy_desk::chattiness() as i32;
    if ui.slider(id("chat"), (c.x + w - 236, c.y + 48 + 12, 210, 24), &mut chat, 0, 3) {
        super::buddy_desk::set_chattiness(chat as u8);
    }
    row_label(ui, c.row(2), "Mood", None);
    let s = crate::graphics::buddy::status();
    row_value(ui, c.row(2), &alloc::format!("{}, {}", s.mood, s.activity));
    ui.text_bold(x, c.end() + 24, "What Bit has learned", th.text);
    let mut yy = c.end() + 50;
    for line in super::buddy_desk::learned_summary() {
        ui.muted(x, yy, &line);
        yy += 22;
    }
    if ui.button((x, yy + 10, 200, 32), "Forget Everything") {
        super::buddy_desk::forget();
    }
    ui.animate(1000);
}

fn keyboard(ui: &mut Ui, x: i32, y: i32, w: i32) {
    let rows = [
        ("Spotlight", "Super Space"),
        ("Mission Control", "F3"),
        ("Launchpad", "F4"),
        ("Switch apps", "Super Tab"),
        ("Close window", "Super W"),
        ("Quit app", "Super Q"),
        ("Minimize", "Super M"),
        ("Zoom", "Super F"),
        ("Snap left / right", "Super Left/Right"),
        ("Show desktop", "Super D"),
        ("Terminal", "Super Enter"),
    ];
    let c = card(ui, x, y, w, rows.len() as i32);
    let f = ui.bold;
    for (k, (a, b)) in rows.iter().enumerate() {
        let r = c.row(k as i32);
        row_label(ui, r, a, None);
        let th = ui.th;
        let tw = gfx::text_width(&f, b) + 16;
        ui.cv.round(r.0 + r.2 - tw - 12, r.1 + 11, tw, 26, 6, th.control);
        ui.cv.text(r.0 + r.2 - tw - 4, r.1 + 14, b, &f, th.text2);
    }
}

fn date_time(ui: &mut Ui, x: i32, y: i32, w: i32) {
    let th = ui.th;
    let (h, m, s, d, mo, yr) = desktop::read_rtc();
    let huge = gfx::font(32, true);
    ui.cv.text(x, y, &alloc::format!("{:02}:{:02}:{:02}", h, m, s), &huge, th.text);
    ui.muted(x, y + 44, &alloc::format!("{}-{:02}-{:02}, from the machine's clock", yr, mo, d));
    let c = card(ui, x, y + 80, w, 1);
    row_label(ui, c.row(0), "24-hour time", None);
    let mut t = CLOCK_24H.load(Ordering::Relaxed);
    if row_toggle(ui, c.row(0), "24h", &mut t) {
        CLOCK_24H.store(t, Ordering::Relaxed);
        desktop::save_flag("clock24", t);
    }
    ui.animate(1000);
}

fn storage(ui: &mut Ui, x: i32, y: i32, w: i32) {
    let th = ui.th;
    let cats = [("Pictures", "/Pictures", 0xFF9F0A), ("Music", "/Music", 0xFF375F), ("Movies", "/Movies", 0x5E5CE6), ("Documents", "/Documents", 0x0A84FF), ("Apps", "/Apps", 0x30D158), ("Trash", "/Trash", 0x8E8E93)];
    let (sizes, total): (Vec<usize>, usize) = {
        let fs = FILESYSTEM.lock();
        match fs.as_ref() {
            Some(fs) => (cats.iter().map(|(_, p, _)| fs.size_of(p)).collect(), fs.size_of("/")),
            None => (alloc::vec![0; cats.len()], 0),
        }
    };
    let capacity: usize = 100 * 1024 * 1024;
    ui.text_bold(x, y, "Disk", th.text);
    ui.muted(x + 60, y, &alloc::format!("{} of 100 MB used", super::files::human_size(total)));
    let bar = (x, y + 30, w, 22);
    ui.cv.round(bar.0, bar.1, bar.2, bar.3, 6, th.control);
    let mut bx = bar.0;
    for (k, (_, _, col)) in cats.iter().enumerate() {
        let bw = ((sizes[k] as i64 * bar.2 as i64 / capacity as i64) as i32).max(if sizes[k] > 0 { 3 } else { 0 });
        ui.cv.rect(bx, bar.1, bw, bar.3, *col);
        bx += bw;
    }
    let c = card(ui, x, y + 76, w, cats.len() as i32);
    for (k, (name, _, col)) in cats.iter().enumerate() {
        let r = c.row(k as i32);
        ui.cv.circle(r.0 + 22, r.1 + 24, 6, *col);
        ui.text(r.0 + 38, r.1 + 14, name, th.text);
        row_value(ui, r, &super::files::human_size(sizes[k]));
    }
    let my = c.end() + 24;
    ui.text_bold(x, my, "Memory", th.text);
    let used = crate::allocator::heap_used();
    ui.progress((x, my + 28, w, 10), (used as i64 * 1000 / crate::allocator::HEAP_SIZE as i64) as i32);
    ui.muted(x, my + 46, &alloc::format!("{} in use of {}", super::files::human_size(used), super::files::human_size(crate::allocator::HEAP_SIZE)));
}

// ---------------------------------------------------------------- About

pub struct About;

impl About {
    pub fn new() -> About {
        About
    }
}

impl App for About {
    fn title(&self) -> String {
        String::from("About zero")
    }
    fn frame(&mut self, ui: &mut Ui) {
        let th = ui.th;
        let w = ui.w;
        ui.fill(th.window);
        let cx = w / 2;
        logo(ui, cx, 56, 40);
        let (tf, f) = (ui.title, ui.font);
        ui.cv.text_center(cx, 106, "zero", &tf, th.text);
        ui.cv.text_center(cx, 138, concat!("Version ", env!("CARGO_PKG_VERSION")), &f, th.text2);
        let rows = [
            alloc::format!("Processors   {} cores", crate::smp::cpu_count()),
            alloc::format!("Memory   {} MB", crate::allocator::HEAP_SIZE / (1024 * 1024)),
            String::from("Graphics   zero software renderer"),
        ];
        for (k, r) in rows.iter().enumerate() {
            ui.cv.text_center(cx, 170 + k as i32 * 22, r, &f, th.text);
        }
        if ui.button((cx - 60, ui.h - 44, 120, 30), "More Info...") {
            super::request(super::Request::Open(super::apps::AppId::Settings, Some(String::from("general"))));
        }
    }
}

// ---------------------------------------------------------------- Activity

pub struct Activity {
    fps: Vec<u32>,
    switches: Vec<u32>,
    last: (u64, u64),
}

impl Activity {
    pub fn new() -> Activity {
        Activity { fps: Vec::new(), switches: Vec::new(), last: (0, crate::sched::switches()) }
    }
}

fn graph(ui: &mut Ui, r: Rect, data: &[u32], color: u32, label: &str) {
    let th = ui.th;
    let bg = if th.dark { 0x2B2C31 } else { 0xFFFFFF };
    ui.cv.round_border(r.0, r.1, r.2, r.3, 10, bg, if th.dark { 0x38393F } else { 0xE0E0E4 });
    ui.text_bold(r.0 + 12, r.1 + 8, label, th.text);
    let max = data.iter().copied().max().unwrap_or(1).max(1);
    let (gx, gy, gw, gh) = (r.0 + 12, r.1 + 34, r.2 - 24, r.3 - 46);
    let n = data.len().max(2) as i32;
    let mut prev: Option<(i32, i32)> = None;
    for (k, v) in data.iter().enumerate() {
        let x = gx + k as i32 * gw / (n - 1).max(1);
        let y = gy + gh - (*v as i32 * gh / max as i32);
        ui.cv.rect_a(x, y, (gw / n).max(1) + 1, gy + gh - y, color, 50);
        if let Some(p) = prev {
            ui.cv.thick_line(p.0, p.1, x, y, 2, color);
        }
        prev = Some((x, y));
    }
    if let Some(v) = data.last() {
        let f = ui.bold;
        ui.cv.text_right(r.0 + r.2 - 12, r.1 + 8, &alloc::format!("{}", v), &f, color);
    }
}

impl App for Activity {
    fn title(&self) -> String {
        String::from("Activity")
    }

    fn tick(&mut self) -> bool {
        true
    }

    fn frame(&mut self, ui: &mut Ui) {
        let now = ui.now;
        if now >= self.last.0 + 500 {
            let sw = crate::sched::switches();
            self.switches.push(((sw - self.last.1) * 2) as u32);
            self.fps.push(desktop::FPS.load(Ordering::Relaxed));
            self.last = (now, sw);
            for v in [&mut self.switches, &mut self.fps] {
                if v.len() > 60 {
                    v.remove(0);
                }
            }
        }
        let th = ui.th;
        let (w, h) = (ui.w, ui.h);
        ui.fill(th.window);
        let half = (w - 36) / 2;
        let (fps, sw) = (self.fps.clone(), self.switches.clone());
        graph(ui, (12, 12, half, 150), &fps, 0x30D158, "Frames per second");
        graph(ui, (24 + half, 12, half, 150), &sw, 0x0A84FF, "Thread switches per second");
        let used = crate::allocator::heap_used();
        ui.text_bold(12, 176, "Memory", th.text);
        ui.progress((90, 182, w - 102, 10), (used as i64 * 1000 / crate::allocator::HEAP_SIZE as i64) as i32);
        ui.muted(12, 200, &alloc::format!("{} used, {} cores, last frame {} ms", super::files::human_size(used), crate::smp::cpu_count(), desktop::FRAME_US.load(Ordering::Relaxed) / 1000));
        ui.text_bold(12, 232, "Running", th.text);
        let names = crate::sched::names();
        let list = (12, 256, w - 24, h - 268);
        let bg = if th.dark { 0x2B2C31 } else { 0xFFFFFF };
        ui.cv.round_border(list.0, list.1, list.2, list.3, 10, bg, if th.dark { 0x38393F } else { 0xE0E0E4 });
        for (k, (tid, name)) in names.iter().enumerate() {
            let y = list.1 + 8 + k as i32 * 24;
            if y > list.1 + list.3 - 24 {
                break;
            }
            ui.muted(list.0 + 14, y, &alloc::format!("{:>3}", tid));
            ui.label(list.0 + 60, y, name);
        }
        ui.animate(600);
    }
}
