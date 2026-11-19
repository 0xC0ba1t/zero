//! Files (the file manager) and Notes (a text editor).

use super::apps::{self, AppId};
use super::gfx::{self, Rect};
use super::icons::{self, Icon};
use super::ui::{self, id, idn, App, MenuItem, Ui, CTRL, SHIFT, SUPER};
use super::Request;
use crate::fs::{Entry, FILESYSTEM};
use crate::input::Key;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

// ---- helpers shared with the desktop -------------------------------------------

pub fn join(dir: &str, name: &str) -> String {
    if dir == "/" { alloc::format!("/{}", name) } else { alloc::format!("{}/{}", dir, name) }
}

pub fn parent(path: &str) -> String {
    match path.rfind('/') {
        Some(0) | None => String::from("/"),
        Some(i) => String::from(&path[..i]),
    }
}

pub fn base(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// A name in `dir` that is not taken yet, based on `name`.
pub fn unique(dir: &str, name: &str) -> String {
    let taken = FILESYSTEM.lock().as_ref().and_then(|fs| fs.list(dir).ok()).unwrap_or_default();
    if !taken.iter().any(|t| t == name) {
        return String::from(name);
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    for n in 2..1000 {
        let candidate = alloc::format!("{} {}{}", stem, n, ext);
        if !taken.iter().any(|t| *t == candidate) {
            return candidate;
        }
    }
    String::from(name)
}

pub fn move_to_trash(path: &str) {
    let name = unique("/Trash", base(path));
    let ok = FILESYSTEM.lock().as_mut().map(|fs| fs.rename(path, &join("/Trash", &name)).is_ok()).unwrap_or(false);
    if ok {
        // remember where it came from, for "Put Back"
        let mut origins = crate::settings::get_or("trash_from", "");
        origins.push_str(&alloc::format!("{}={};", name, path));
        crate::settings::set("trash_from", &origins);
        crate::audio::effect(crate::audio::Effect::Close);
    }
    crate::pfs::sync_if_dirty();
}

pub fn empty_trash() {
    if let Some(fs) = FILESYSTEM.lock().as_mut() {
        for name in fs.list("/Trash").unwrap_or_default() {
            let _ = fs.delete(&join("/Trash", &name));
        }
    }
    crate::settings::set("trash_from", "");
    crate::pfs::sync_if_dirty();
}

fn put_back(name: &str) -> bool {
    let origins = crate::settings::get_or("trash_from", "");
    let from = origins.split(';').find_map(|e| e.split_once('=').filter(|(n, _)| *n == name).map(|(_, p)| String::from(p)));
    let target = from.unwrap_or_else(|| join("/Documents", name));
    let ok = FILESYSTEM.lock().as_mut().map(|fs| fs.rename(&join("/Trash", name), &target).is_ok()).unwrap_or(false);
    crate::pfs::sync_if_dirty();
    ok
}

pub fn kind_of(name: &str, is_dir: bool) -> &'static str {
    if is_dir {
        return "Folder";
    }
    match name.to_ascii_lowercase().rsplit('.').next().unwrap_or("") {
        "png" | "bmp" | "ppm" | "pgm" | "zimg" => "Image",
        "gif" => "Animation",
        "wav" => "Audio",
        "zs" => "ZeroScript program",
        "txt" | "md" => "Text document",
        _ => "Document",
    }
}

pub fn human_size(n: usize) -> String {
    if n < 1024 {
        alloc::format!("{} bytes", n)
    } else if n < 1024 * 1024 {
        alloc::format!("{}.{} KB", n / 1024, n % 1024 * 10 / 1024)
    } else {
        alloc::format!("{}.{} MB", n / (1024 * 1024), n % (1024 * 1024) * 10 / (1024 * 1024))
    }
}

// ---- the file manager -------------------------------------------------------------

const PLACES: [(&str, &str, Icon); 8] = [
    ("Desktop", "/Desktop", Icon::Folder),
    ("Documents", "/Documents", Icon::Folder),
    ("Downloads", "/Downloads", Icon::Folder),
    ("Pictures", "/Pictures", Icon::Image),
    ("Music", "/Music", Icon::Audio),
    ("Movies", "/Movies", Icon::Video),
    ("Apps", "/Apps", Icon::Code),
    ("Home", "/", Icon::Folder),
];

static CLIPBOARD: spin::Mutex<Option<String>> = spin::Mutex::new(None);

pub struct Files {
    cwd: String,
    entries: Vec<Entry>,
    selected: Option<usize>,
    back: Vec<String>,
    forward: Vec<String>,
    list_view: usize,
    search: String,
    renaming: Option<(usize, String)>,
    info: Option<usize>,
    status: String,
    thumbs: BTreeMap<String, Option<(Vec<u32>, usize, usize)>>,
    drag: Option<(usize, (i32, i32), bool)>,
    sort: usize,
    last_arg: Option<String>,
}

const SIDE: i32 = 186;
const BAR: i32 = 52;

impl Files {
    pub fn new(arg: Option<String>) -> Files {
        let cwd = arg.clone().filter(|p| FILESYSTEM.lock().as_ref().map(|fs| fs.is_dir(p)).unwrap_or(false)).unwrap_or_else(|| String::from("/Documents"));
        let mut f = Files {
            cwd,
            entries: Vec::new(),
            selected: None,
            back: Vec::new(),
            forward: Vec::new(),
            list_view: 0,
            search: String::new(),
            renaming: None,
            info: None,
            status: String::new(),
            thumbs: BTreeMap::new(),
            drag: None,
            sort: 0,
            last_arg: arg,
        };
        f.reload();
        f
    }

    fn reload(&mut self) {
        let mut e = FILESYSTEM.lock().as_ref().and_then(|fs| fs.entries(&self.cwd).ok()).unwrap_or_default();
        if !self.search.is_empty() {
            let q = self.search.to_ascii_lowercase();
            e.retain(|x| x.name.to_ascii_lowercase().contains(&q));
        }
        match self.sort {
            1 => e.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(b.size.cmp(&a.size))),
            2 => e.sort_by(|a, b| kind_of(&a.name, a.is_dir).cmp(kind_of(&b.name, b.is_dir)).then(a.name.cmp(&b.name))),
            _ => e.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase()))),
        }
        self.entries = e;
        if self.selected.map(|s| s >= self.entries.len()).unwrap_or(false) {
            self.selected = None;
        }
    }

    fn go(&mut self, path: &str) {
        if path == self.cwd {
            return;
        }
        if FILESYSTEM.lock().as_ref().map(|fs| fs.is_dir(path)).unwrap_or(false) {
            self.back.push(core::mem::replace(&mut self.cwd, String::from(path)));
            self.forward.clear();
            self.selected = None;
            self.search.clear();
            self.renaming = None;
            self.info = None;
            self.reload();
        }
    }

    fn path(&self, i: usize) -> String {
        join(&self.cwd, &self.entries[i].name)
    }

    fn open(&mut self, i: usize) {
        if i >= self.entries.len() {
            return;
        }
        let p = self.path(i);
        if self.entries[i].is_dir {
            self.go(&p);
        } else {
            super::open_file(&p);
        }
    }

    fn create(&mut self, folder: bool) {
        let name = unique(&self.cwd, if folder { "untitled folder" } else { "untitled.txt" });
        let p = join(&self.cwd, &name);
        if let Some(fs) = FILESYSTEM.lock().as_mut() {
            let _ = if folder { fs.create_dir(&p) } else { fs.create_file(&p) };
        }
        crate::pfs::sync_if_dirty();
        self.reload();
        if let Some(i) = self.entries.iter().position(|e| e.name == name) {
            self.selected = Some(i);
            self.renaming = Some((i, name));
        }
    }

    fn trash_selected(&mut self) {
        let Some(i) = self.selected else { return };
        if i >= self.entries.len() {
            return;
        }
        let p = self.path(i);
        if self.cwd == "/Trash" {
            if let Some(fs) = FILESYSTEM.lock().as_mut() {
                let _ = fs.delete(&p);
            }
            crate::pfs::sync_if_dirty();
        } else {
            move_to_trash(&p);
        }
        self.selected = None;
        self.reload();
    }

    fn duplicate(&mut self, i: usize) {
        let name = &self.entries[i].name;
        let (stem, ext) = match name.rfind('.') {
            Some(k) if k > 0 => (&name[..k], &name[k..]),
            _ => (name.as_str(), ""),
        };
        let new = unique(&self.cwd, &alloc::format!("{} copy{}", stem, ext));
        let from = self.path(i);
        if let Some(fs) = FILESYSTEM.lock().as_mut() {
            let _ = fs.copy(&from, &join(&self.cwd, &new));
        }
        crate::pfs::sync_if_dirty();
        self.reload();
    }

    fn paste(&mut self) {
        let Some(from) = CLIPBOARD.lock().clone() else { return };
        let name = unique(&self.cwd, base(&from));
        let ok = FILESYSTEM.lock().as_mut().map(|fs| fs.copy(&from, &join(&self.cwd, &name)).is_ok()).unwrap_or(false);
        self.status = if ok { alloc::format!("Pasted {}", name) } else { String::from("Could not paste") };
        crate::pfs::sync_if_dirty();
        self.reload();
    }

    fn finish_rename(&mut self) {
        let Some((i, name)) = self.renaming.take() else { return };
        if i >= self.entries.len() || name.trim().is_empty() || name.contains('/') || name == self.entries[i].name {
            return;
        }
        let from = self.path(i);
        let to = join(&self.cwd, name.trim());
        let ok = FILESYSTEM.lock().as_mut().map(|fs| fs.rename(&from, &to).is_ok()).unwrap_or(false);
        self.status = if ok { String::new() } else { String::from("That name is taken.") };
        crate::pfs::sync_if_dirty();
        self.reload();
        if let Some(k) = self.entries.iter().position(|e| e.name == name.trim()) {
            self.selected = Some(k);
        }
    }

    fn thumb(&mut self, path: &str, size: usize) -> Option<&(Vec<u32>, usize, usize)> {
        if !self.thumbs.contains_key(path) {
            let t = super::media::load_image(path).map(|img| {
                let s = (size * 1000 / img.w.max(1)).min(size * 1000 / img.h.max(1));
                let (tw, th) = ((img.w * s / 1000).max(1), (img.h * s / 1000).max(1));
                let mut out = alloc::vec![0u32; tw * th];
                let mut cv = gfx::Canvas::new(&mut out, tw, th);
                cv.blit_scaled((0, 0, tw as i32, th as i32), &img.px, img.w, img.h, true, false);
                (out, tw, th)
            });
            self.thumbs.insert(String::from(path), t);
        }
        self.thumbs.get(path).and_then(|t| t.as_ref())
    }

    fn item_rects(&self, w: i32) -> Vec<Rect> {
        let area_x = SIDE + 12;
        let area_w = w - SIDE - 24;
        if self.list_view == 1 {
            return (0..self.entries.len()).map(|k| (SIDE + 6, BAR + 30 + k as i32 * 28, w - SIDE - 12, 28)).collect();
        }
        let (cw, ch) = (104, 104);
        let cols = (area_w / cw).max(1);
        (0..self.entries.len()).map(|k| (area_x + (k as i32 % cols) * cw, BAR + 12 + (k as i32 / cols) * ch, cw, ch)).collect()
    }

    fn item_menu(&self) -> Vec<MenuItem> {
        let m = MenuItem::new;
        if self.cwd == "/Trash" {
            return alloc::vec![m("Put Back"), m("Delete Immediately"), MenuItem::sep(), m("Empty Trash")];
        }
        alloc::vec![
            m("Open"),
            m("Open in Notes"),
            m("Open in Media"),
            MenuItem::sep(),
            m("Get Info").hint("Super I"),
            m("Rename").hint("F2"),
            m("Duplicate").hint("Super D"),
            m("Copy").hint("Super C"),
            MenuItem::sep(),
            m("Set as Wallpaper"),
            m("Move to Trash").hint("Del"),
        ]
    }

    fn item_action(&mut self, i: usize, k: usize) {
        if i >= self.entries.len() {
            return;
        }
        let p = self.path(i);
        if self.cwd == "/Trash" {
            match k {
                0 => {
                    let name = self.entries[i].name.clone();
                    put_back(&name);
                }
                1 => {
                    if let Some(fs) = FILESYSTEM.lock().as_mut() {
                        let _ = fs.delete(&p);
                    }
                    crate::pfs::sync_if_dirty();
                }
                3 => empty_trash(),
                _ => {}
            }
            self.reload();
            return;
        }
        match k {
            0 => self.open(i),
            1 => super::request(Request::Open(AppId::Notes, Some(p))),
            2 => super::request(Request::Open(AppId::Media, Some(p))),
            4 => self.info = Some(i),
            5 => self.renaming = Some((i, self.entries[i].name.clone())),
            6 => self.duplicate(i),
            7 => {
                *CLIPBOARD.lock() = Some(p);
                self.status = String::from("Copied");
            }
            9 => {
                if super::media::load_image(&p).is_some() {
                    *super::desktop::WALL_IMAGE.lock() = Some(p.clone());
                    crate::settings::set("wall_image", &p);
                    super::desktop::WALL_GEN.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                } else {
                    self.status = String::from("Only pictures can be wallpapers.");
                }
            }
            10 => {
                self.selected = Some(i);
                self.trash_selected();
            }
            _ => {}
        }
    }

    fn back_menu(&self) -> Vec<MenuItem> {
        let m = MenuItem::new;
        alloc::vec![
            m("New Folder").hint("Super N"),
            m("New Text File"),
            m("Paste").hint("Super V").disabled(CLIPBOARD.lock().is_none()),
            MenuItem::sep(),
            m("View as Icons"),
            m("View as List"),
            MenuItem::sep(),
            m("Sort by Name"),
            m("Sort by Size"),
            m("Sort by Kind"),
            MenuItem::sep(),
            m("Open Terminal"),
        ]
    }

    fn back_action(&mut self, k: usize) {
        match k {
            0 => self.create(true),
            1 => self.create(false),
            2 => self.paste(),
            4 => self.list_view = 0,
            5 => self.list_view = 1,
            7 | 8 | 9 => {
                self.sort = k - 7;
                self.reload();
            }
            11 => super::request(Request::Open(AppId::Terminal, None)),
            _ => {}
        }
    }
}

impl App for Files {
    fn title(&self) -> String {
        let n = base(&self.cwd);
        String::from(if n.is_empty() { "Home" } else { n })
    }

    fn menus(&self) -> Vec<(&'static str, Vec<MenuItem>)> {
        let m = MenuItem::new;
        alloc::vec![
            ("File", alloc::vec![m("New Folder"), m("New Text File"), MenuItem::sep(), m("Open"), m("Get Info"), m("Rename"), m("Duplicate"), MenuItem::sep(), m("Move to Trash")]),
            ("Edit", alloc::vec![m("Copy"), m("Paste")]),
            ("View", alloc::vec![m("as Icons"), m("as List")]),
            ("Go", alloc::vec![m("Back"), m("Forward"), m("Enclosing Folder"), MenuItem::sep(), m("Home"), m("Desktop"), m("Documents"), m("Downloads"), m("Trash")]),
        ]
    }

    fn on_menu(&mut self, menu: usize, item: usize) {
        let sel = self.selected;
        match (menu, item) {
            (0, 0) => self.create(true),
            (0, 1) => self.create(false),
            (0, 3) => {
                if let Some(i) = sel {
                    self.open(i)
                }
            }
            (0, 4) => self.info = sel,
            (0, 5) => {
                if let Some(i) = sel {
                    self.renaming = Some((i, self.entries[i].name.clone()));
                }
            }
            (0, 6) => {
                if let Some(i) = sel {
                    self.duplicate(i)
                }
            }
            (0, 8) => self.trash_selected(),
            (1, 0) => {
                if let Some(i) = sel {
                    *CLIPBOARD.lock() = Some(self.path(i));
                }
            }
            (1, 1) => self.paste(),
            (2, k) => self.list_view = k,
            (3, 0) => {
                if let Some(p) = self.back.pop() {
                    self.forward.push(core::mem::replace(&mut self.cwd, p));
                    self.reload();
                }
            }
            (3, 1) => {
                if let Some(p) = self.forward.pop() {
                    self.back.push(core::mem::replace(&mut self.cwd, p));
                    self.reload();
                }
            }
            (3, 2) => {
                let p = parent(&self.cwd);
                self.go(&p);
            }
            (3, k) => {
                let p = ["/", "/Desktop", "/Documents", "/Downloads", "/Trash"].get(k.wrapping_sub(4)).copied().unwrap_or("/");
                self.go(p);
            }
            _ => {}
        }
    }

    fn frame(&mut self, ui: &mut Ui) {
        // the desktop asked us to show another folder
        if let Some(a) = super::app_arg() {
            if Some(&a) != self.last_arg.as_ref() {
                self.last_arg = Some(a.clone());
                self.go(&a);
            }
        }
        if ui.inp.focused || ui.inp.pressed {
            // files may have changed elsewhere
            let fresh = FILESYSTEM.lock().as_ref().and_then(|fs| fs.list(&self.cwd).ok()).unwrap_or_default();
            if fresh.len() != self.entries.len() && self.search.is_empty() {
                self.reload();
            }
        }
        let th = ui.th;
        let (w, h) = (ui.w, ui.h);
        ui.fill(th.window);

        // ---- sidebar ----
        ui.cv.rect(0, 0, SIDE, h, th.sidebar);
        ui.cv.vline(SIDE, 0, h, th.separator, 255);
        let small = gfx::font(16, true);
        ui.cv.text(16, 14, "Favorites", &small, th.text3);
        for (k, (name, path, icon)) in PLACES.iter().enumerate() {
            let r = (8, 38 + k as i32 * 32, SIDE - 16, 30);
            let resp = ui.flat(idn("place", k), r, self.cwd == *path);
            if resp.clicked {
                self.go(path);
            }
            icons::draw(&mut ui.cv, *icon, r.0 + 8, r.1 + 5, 20);
            let c = if self.cwd == *path { th.on_accent } else { th.text };
            ui.text(r.0 + 36, r.1 + 6, name, c);
        }
        let ty = 38 + PLACES.len() as i32 * 32 + 14;
        ui.cv.text(16, ty, "Locations", &small, th.text3);
        let tr = (8, ty + 24, SIDE - 16, 30);
        let full = FILESYSTEM.lock().as_ref().and_then(|fs| fs.list("/Trash").ok()).map(|l| !l.is_empty()).unwrap_or(false);
        if ui.flat(id("trash"), tr, self.cwd == "/Trash").clicked {
            self.go("/Trash");
        }
        icons::draw(&mut ui.cv, if full { Icon::TrashFull } else { Icon::Trash }, tr.0 + 8, tr.1 + 4, 22);
        let c = if self.cwd == "/Trash" { th.on_accent } else { th.text };
        ui.text(tr.0 + 36, tr.1 + 6, "Trash", c);

        // ---- toolbar ----
        ui.cv.rect(SIDE + 1, 0, w - SIDE - 1, BAR, th.window);
        ui.cv.hline(SIDE + 1, BAR - 1, w, th.separator, 255);
        let bx = SIDE + 12;
        if ui.button((bx, 11, 30, 30), "<") {
            if let Some(p) = self.back.pop() {
                self.forward.push(core::mem::replace(&mut self.cwd, p));
                self.selected = None;
                self.reload();
            }
        }
        if ui.button((bx + 34, 11, 30, 30), ">") {
            if let Some(p) = self.forward.pop() {
                self.back.push(core::mem::replace(&mut self.cwd, p));
                self.selected = None;
                self.reload();
            }
        }
        let title = self.title();
        let tf = ui.big;
        ui.cv.text(bx + 76, 14, &gfx::ellipsize(&tf, &title, w - bx - 400), &tf, th.text);
        let mut view = self.list_view;
        if ui.segmented(id("view"), (w - 330, 11, 96, 30), &["Icons", "List"], &mut view) {
            self.list_view = view;
        }
        let before = self.search.clone();
        ui.field(id("search"), (w - 222, 11, 210, 30), &mut self.search, "Search");
        if self.search != before {
            self.reload();
        }

        // ---- content ----
        let rects = self.item_rects(w);
        let content = (SIDE + 1, BAR, w - SIDE - 1, h - BAR - 26);
        let total_h = rects.last().map(|r| r.1 + r.3 - BAR + 16).unwrap_or(0);
        let sid = idn("scroll", self.cwd.len());
        let off = ui.scroll_begin(sid, content, total_h);
        if self.list_view == 1 {
            let hf = small;
            ui.cv.text(SIDE + 44, BAR + 8 - off, "Name", &hf, th.text3);
            ui.cv.text(w - 300, BAR + 8 - off, "Size", &hf, th.text3);
            ui.cv.text(w - 180, BAR + 8 - off, "Kind", &hf, th.text3);
        }
        let mut clicked_item = false;
        let menu_open = ui.menu_open();
        for k in 0..self.entries.len() {
            let r = rects[k];
            let r = (r.0, r.1 - off, r.2, r.3);
            if r.1 + r.3 < BAR || r.1 > h {
                continue;
            }
            let e = self.entries[k].clone();
            let resp = ui.interact(idn("item", k), r);
            let sel = self.selected == Some(k);
            if resp.pressed || resp.right {
                self.selected = Some(k);
                clicked_item = true;
                if self.renaming.as_ref().map(|(i, _)| *i != k).unwrap_or(false) {
                    self.finish_rename();
                }
                self.drag = Some((k, ui.mouse(), false));
            }
            if resp.double {
                self.open(k);
                return;
            }
            if resp.right && !menu_open {
                let (mx, my) = ui.mouse();
                ui.open_menu(id("itemmenu"), mx, my, self.item_menu());
            }
            let path = join(&self.cwd, &e.name);
            let icon = apps::file_icon(&e.name, e.is_dir);
            let drop_target = e.is_dir && self.drag.map(|(d, _, moved)| moved && d != k).unwrap_or(false) && resp.hover;
            if self.list_view == 1 {
                if sel {
                    ui.cv.round(r.0, r.1, r.2, r.3, 5, if ui.inp.focused { th.accent } else { gfx::mix(th.window, th.text, 40) });
                } else if k % 2 == 1 {
                    ui.cv.rect_a(r.0, r.1, r.2, r.3, th.text, 8);
                }
                if drop_target {
                    ui.cv.round_outline(r.0, r.1, r.2, r.3, 5, th.accent, 255);
                }
                icons::draw(&mut ui.cv, icon, r.0 + 10, r.1 + 3, 22);
                let c = if sel { th.on_accent } else { th.text };
                let c2 = if sel { th.on_accent } else { th.text2 };
                if self.renaming.as_ref().map(|(i, _)| *i == k).unwrap_or(false) {
                    let (_, mut name) = self.renaming.take().unwrap();
                    let done = ui.field(id("rename"), (r.0 + 38, r.1 + 2, 300, 24), &mut name, "");
                    ui.st.focus = id("rename");
                    self.renaming = Some((k, name));
                    if done {
                        self.finish_rename();
                    }
                } else {
                    let f = ui.font;
                    ui.cv.text(r.0 + 38, r.1 + 5, &gfx::ellipsize(&f, &e.name, w - 380 - r.0), &f, c);
                }
                let size = if e.is_dir { String::from("--") } else { human_size(e.size) };
                ui.text(w - 300, r.1 + 5, &size, c2);
                ui.text(w - 180, r.1 + 5, kind_of(&e.name, e.is_dir), c2);
            } else {
                let isz = 60;
                let ix = r.0 + (r.2 - isz) / 2;
                let iy = r.1 + 6;
                if sel {
                    ui.cv.round_a(ix - 6, iy - 4, isz + 12, isz + 8, 8, th.text, 30);
                }
                if drop_target {
                    ui.cv.round_outline(ix - 6, iy - 4, isz + 12, isz + 8, 8, th.accent, 255);
                }
                let mut drew = false;
                if matches!(icon, Icon::Image) && e.size < 2 * 1024 * 1024 {
                    if let Some((px, tw, thh)) = self.thumb(&path, isz as usize) {
                        let (tw, thh) = (*tw as i32, *thh as i32);
                        let px = px.clone();
                        ui.cv.shadow(ix + (isz - tw) / 2, iy + (isz - thh) / 2, tw, thh, 2, 4, 70);
                        ui.cv.blit(ix + (isz - tw) / 2, iy + (isz - thh) / 2, &px, tw as usize, thh as usize);
                        drew = true;
                    }
                }
                if !drew {
                    icons::draw(&mut ui.cv, icon, ix, iy, isz);
                }
                if self.renaming.as_ref().map(|(i, _)| *i == k).unwrap_or(false) {
                    let (_, mut name) = self.renaming.take().unwrap();
                    let done = ui.field(id("rename"), (r.0 - 10, r.1 + 70, r.2 + 20, 26), &mut name, "");
                    ui.st.focus = id("rename");
                    self.renaming = Some((k, name));
                    if done {
                        self.finish_rename();
                    }
                } else {
                    let f = ui.font;
                    let label = gfx::ellipsize(&f, &e.name, r.2 - 6);
                    let tw = gfx::text_width(&f, &label);
                    let lx = r.0 + (r.2 - tw) / 2;
                    if sel {
                        ui.cv.round(lx - 5, r.1 + 72, tw + 10, 21, 5, if ui.inp.focused { th.accent } else { gfx::mix(th.window, th.text, 40) });
                    }
                    ui.cv.text(lx, r.1 + 74, &label, &f, if sel { th.on_accent } else { th.text });
                }
            }
        }
        if self.entries.is_empty() {
            let msg = if !self.search.is_empty() { "No matches." } else if self.cwd == "/Trash" { "The trash is empty." } else { "This folder is empty." };
            let f = ui.big;
            ui.cv.text_center(SIDE + (w - SIDE) / 2, BAR + (h - BAR) / 2 - 30, msg, &f, th.text3);
        }
        ui.scroll_end(sid, content, total_h);

        // dragging a file onto a folder moves it
        if let Some((k, start, moved)) = self.drag {
            let p = ui.mouse();
            if ui.inp.down && !moved && ((p.0 - start.0).abs() > 6 || (p.1 - start.1).abs() > 6) {
                self.drag = Some((k, start, true));
            }
            if ui.inp.released {
                if moved {
                    let target = (0..self.entries.len()).find(|&j| j != k && self.entries[j].is_dir && ui::inside((p.0, p.1 + off), rects[j]));
                    if let Some(j) = target {
                        let from = self.path(k);
                        let to = join(&self.path(j), &self.entries[k].name);
                        let ok = FILESYSTEM.lock().as_mut().map(|fs| fs.rename(&from, &to).is_ok()).unwrap_or(false);
                        self.status = if ok { alloc::format!("Moved to {}", self.entries[j].name) } else { String::from("Could not move it there") };
                        crate::pfs::sync_if_dirty();
                        self.reload();
                    }
                }
                self.drag = None;
            }
            if moved && k < self.entries.len() {
                let icon = apps::file_icon(&self.entries[k].name, self.entries[k].is_dir);
                ui.cv.opacity = 160;
                icons::draw(&mut ui.cv, icon, p.0 - 20, p.1 - 20, 40);
                ui.cv.opacity = 255;
            }
        }
        // a click on nothing clears the selection; right-click shows the folder menu
        if ui.hovered(content) && !clicked_item {
            if ui.inp.pressed {
                self.selected = None;
                self.finish_rename();
            }
            if ui.inp.rpressed && !menu_open {
                let (mx, my) = ui.mouse();
                ui.open_menu(id("backmenu"), mx, my, self.back_menu());
            }
        }
        if let Some(k) = ui.menu_pick(id("itemmenu")) {
            if let Some(i) = self.selected {
                self.item_action(i, k);
            }
        }
        if let Some(k) = ui.menu_pick(id("backmenu")) {
            self.back_action(k);
        }
        // a file dropped from the desktop is moved here
        if let Some(p) = ui.inp.drop_path.clone() {
            let to = join(&self.cwd, &unique(&self.cwd, base(&p)));
            let _ = FILESYSTEM.lock().as_mut().map(|fs| fs.rename(&p, &to));
            crate::pfs::sync_if_dirty();
            self.reload();
        }

        // keyboard
        if self.renaming.is_none() && ui.st.focus != id("search") {
            let n = self.entries.len();
            let cols = if self.list_view == 1 { 1 } else { ((w - SIDE - 24) / 104).max(1) as usize };
            for (k, m) in ui.keys().to_vec() {
                let cmd = m & (CTRL | SUPER) != 0;
                match k {
                    Key::Right if n > 0 => self.selected = Some(self.selected.map(|s| (s + 1).min(n - 1)).unwrap_or(0)),
                    Key::Left if n > 0 => self.selected = Some(self.selected.map(|s| s.saturating_sub(1)).unwrap_or(0)),
                    Key::Down if n > 0 => self.selected = Some(self.selected.map(|s| (s + cols).min(n - 1)).unwrap_or(0)),
                    Key::Up if n > 0 => self.selected = Some(self.selected.map(|s| s.saturating_sub(cols)).unwrap_or(0)),
                    Key::Char('\n') => {
                        if let Some(i) = self.selected {
                            self.open(i);
                            return;
                        }
                    }
                    Key::Char('\u{8}') if cmd => self.trash_selected(),
                    Key::Delete => self.trash_selected(),
                    Key::F(2) => {
                        if let Some(i) = self.selected {
                            self.renaming = Some((i, self.entries[i].name.clone()));
                        }
                    }
                    Key::Char('n') | Key::Char('N') if cmd => self.create(m & SHIFT == 0),
                    Key::Char('c') if cmd => {
                        if let Some(i) = self.selected {
                            *CLIPBOARD.lock() = Some(self.path(i));
                            self.status = String::from("Copied");
                        }
                    }
                    Key::Char('v') if cmd => self.paste(),
                    Key::Char('d') if cmd => {
                        if let Some(i) = self.selected {
                            self.duplicate(i);
                        }
                    }
                    Key::Char('i') if cmd => self.info = self.selected,
                    Key::Char('\u{8}') => {
                        let p = parent(&self.cwd);
                        self.go(&p);
                    }
                    _ => {}
                }
            }
        }

        // status bar
        ui.cv.rect(SIDE + 1, h - 26, w, 26, th.window);
        ui.cv.hline(SIDE + 1, h - 26, w, th.separator, 255);
        let used: usize = self.entries.iter().map(|e| e.size).sum();
        let status = if self.status.is_empty() { alloc::format!("{} items, {}", self.entries.len(), human_size(used)) } else { self.status.clone() };
        let c = th.text2;
        ui.cv.text_center(SIDE + (w - SIDE) / 2, h - 22, &status, &ui.font.clone(), c);

        // Get Info sheet
        if let Some(i) = self.info {
            if i >= self.entries.len() {
                self.info = None;
                return;
            }
            let e = self.entries[i].clone();
            let (pw, ph) = (340, 260);
            let (px, py) = ((w - pw) / 2, (h - ph) / 2);
            ui.cv.rect_a(0, 0, w, h, 0x000000, 70);
            ui.cv.shadow(px, py + 6, pw, ph, 14, 30, 120);
            ui.cv.round_border(px, py, pw, ph, 14, th.window, th.control_border);
            icons::draw(&mut ui.cv, apps::file_icon(&e.name, e.is_dir), px + 20, py + 20, 56);
            let bf = ui.big;
            ui.cv.text(px + 90, py + 26, &gfx::ellipsize(&bf, &e.name, pw - 110), &bf, th.text);
            ui.muted(px + 90, py + 54, kind_of(&e.name, e.is_dir));
            let path = self.path(i);
            let size = FILESYSTEM.lock().as_ref().map(|fs| fs.size_of(&path)).unwrap_or(0);
            let rows = [("Kind", String::from(kind_of(&e.name, e.is_dir))), ("Size", human_size(size)), ("Where", self.cwd.clone())];
            for (k, (a, b)) in rows.iter().enumerate() {
                let y = py + 100 + k as i32 * 28;
                ui.cv.text_right(px + 100, y, a, &ui.font.clone(), th.text3);
                ui.text(px + 112, y, &gfx::ellipsize(&ui.font.clone(), b, pw - 130), th.text);
            }
            if ui.primary((px + pw - 100, py + ph - 46, 84, 30), "Done") || ui.key(Key::Char('\u{1b}')) {
                self.info = None;
            }
        }
    }
}

// ---- Notes -------------------------------------------------------------------------

pub struct Notes {
    path: Option<String>,
    lines: Vec<String>,
    cy: usize,
    cx: usize,
    top: usize,
    modified: bool,
    naming: Option<String>,
    status: String,
}

impl Notes {
    pub fn new(arg: Option<String>) -> Notes {
        let mut n = Notes { path: None, lines: alloc::vec![String::new()], cy: 0, cx: 0, top: 0, modified: false, naming: None, status: String::new() };
        if let Some(p) = arg {
            n.load(&p);
        }
        n
    }

    fn load(&mut self, p: &str) {
        let data = FILESYSTEM.lock().as_ref().and_then(|fs| fs.read_file(p).ok()).unwrap_or_default();
        let text = String::from_utf8_lossy(&data).into_owned();
        self.lines = text.split('\n').map(|l| String::from(l.trim_end_matches('\r'))).collect();
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        self.path = Some(String::from(p));
        self.cy = 0;
        self.cx = 0;
        self.top = 0;
        self.modified = false;
    }

    fn save(&mut self) {
        let Some(p) = self.path.clone() else {
            self.naming = Some(String::from("Untitled.txt"));
            return;
        };
        let data = self.lines.join("\n").into_bytes();
        let ok = FILESYSTEM.lock().as_mut().map(|fs| fs.write_file(&p, data).is_ok()).unwrap_or(false);
        crate::pfs::sync_if_dirty();
        self.modified = !ok;
        self.status = String::from(if ok { "Saved" } else { "Could not save" });
    }

    fn char_index(line: &str, cx: usize) -> usize {
        line.char_indices().nth(cx).map(|(i, _)| i).unwrap_or(line.len())
    }
}

impl App for Notes {
    fn title(&self) -> String {
        let name = self.path.as_deref().map(base).unwrap_or("Untitled");
        if self.modified { alloc::format!("{} - Edited", name) } else { String::from(name) }
    }

    fn menus(&self) -> Vec<(&'static str, Vec<MenuItem>)> {
        let m = MenuItem::new;
        alloc::vec![("File", alloc::vec![m("New"), m("Save"), m("Save As..."), MenuItem::sep(), m("Run as ZeroScript")])]
    }

    fn on_menu(&mut self, _menu: usize, item: usize) {
        match item {
            0 => {
                *self = Notes::new(None);
            }
            1 => self.save(),
            2 => self.naming = Some(self.path.as_deref().map(base).map(String::from).unwrap_or_else(|| String::from("Untitled.txt"))),
            4 => {
                self.save();
                if let Some(p) = &self.path {
                    super::request(Request::Open(AppId::Script, Some(p.clone())));
                }
            }
            _ => {}
        }
    }

    fn frame(&mut self, ui: &mut Ui) {
        let th = ui.th;
        let (w, h) = (ui.w, ui.h);
        let paper = if th.dark { 0x1E1F23 } else { 0xFFFFFF };
        ui.fill(paper);
        let f = gfx::font(16, false);
        let lh = f.height() as i32 + 6;
        let bar = 44;
        ui.cv.rect(0, 0, w, bar, th.window);
        ui.cv.hline(0, bar - 1, w, th.separator, 255);
        if ui.button((10, 7, 70, 30), "Save") {
            self.save();
        }
        if ui.button((86, 7, 70, 30), "New") {
            *self = Notes::new(None);
            return;
        }
        let words: usize = self.lines.iter().map(|l| l.split_whitespace().count()).sum();
        let info = if self.status.is_empty() { alloc::format!("{} lines, {} words", self.lines.len(), words) } else { self.status.clone() };
        ui.muted(170, 14, &info);
        let rows = ((h - bar - 16) / lh).max(1) as usize;

        // naming sheet for "save as"
        if let Some(mut name) = self.naming.take() {
            ui.cv.rect_a(0, 0, w, h, 0x000000, 70);
            let (pw, ph) = (360, 150);
            let (px, py) = ((w - pw) / 2, 80);
            ui.cv.shadow(px, py + 6, pw, ph, 14, 30, 120);
            ui.cv.round_border(px, py, pw, ph, 14, th.window, th.control_border);
            ui.text_bold(px + 20, py + 18, "Save in Documents as:", th.text);
            ui.st.focus = id("savename");
            let enter = ui.field(id("savename"), (px + 20, py + 50, pw - 40, 30), &mut name, "name");
            let ok = ui.primary((px + pw - 100, py + ph - 46, 80, 30), "Save");
            let cancel = ui.button((px + pw - 190, py + ph - 46, 80, 30), "Cancel") || ui.key(Key::Char('\u{1b}'));
            if (enter || ok) && !name.trim().is_empty() {
                self.path = Some(join("/Documents", name.trim()));
                self.save();
            } else if !cancel {
                self.naming = Some(name);
            }
            return;
        }

        // editing
        for (k, m) in ui.keys().to_vec() {
            let cmd = m & (CTRL | SUPER) != 0;
            self.status.clear();
            let len = self.lines[self.cy].chars().count();
            self.cx = self.cx.min(len);
            match k {
                Key::Char('s') if cmd => self.save(),
                Key::Char('\n') => {
                    let i = Self::char_index(&self.lines[self.cy], self.cx);
                    let rest = self.lines[self.cy].split_off(i);
                    self.lines.insert(self.cy + 1, rest);
                    self.cy += 1;
                    self.cx = 0;
                    self.modified = true;
                }
                Key::Char('\u{8}') => {
                    if self.cx > 0 {
                        let i = Self::char_index(&self.lines[self.cy], self.cx - 1);
                        self.lines[self.cy].remove(i);
                        self.cx -= 1;
                    } else if self.cy > 0 {
                        let line = self.lines.remove(self.cy);
                        self.cy -= 1;
                        self.cx = self.lines[self.cy].chars().count();
                        self.lines[self.cy].push_str(&line);
                    }
                    self.modified = true;
                }
                Key::Delete => {
                    if self.cx < len {
                        let i = Self::char_index(&self.lines[self.cy], self.cx);
                        self.lines[self.cy].remove(i);
                    } else if self.cy + 1 < self.lines.len() {
                        let next = self.lines.remove(self.cy + 1);
                        self.lines[self.cy].push_str(&next);
                    }
                    self.modified = true;
                }
                Key::Left => {
                    if self.cx > 0 {
                        self.cx -= 1;
                    } else if self.cy > 0 {
                        self.cy -= 1;
                        self.cx = self.lines[self.cy].chars().count();
                    }
                }
                Key::Right => {
                    if self.cx < len {
                        self.cx += 1;
                    } else if self.cy + 1 < self.lines.len() {
                        self.cy += 1;
                        self.cx = 0;
                    }
                }
                Key::Up => self.cy = self.cy.saturating_sub(1),
                Key::Down => self.cy = (self.cy + 1).min(self.lines.len() - 1),
                Key::PageUp => self.cy = self.cy.saturating_sub(rows),
                Key::PageDown => self.cy = (self.cy + rows).min(self.lines.len() - 1),
                Key::Home => self.cx = 0,
                Key::End => self.cx = len,
                Key::Char('\t') => {
                    let i = Self::char_index(&self.lines[self.cy], self.cx);
                    self.lines[self.cy].insert_str(i, "    ");
                    self.cx += 4;
                    self.modified = true;
                }
                Key::Char(c) if !c.is_control() && !cmd => {
                    let i = Self::char_index(&self.lines[self.cy], self.cx);
                    self.lines[self.cy].insert(i, c);
                    self.cx += 1;
                    self.modified = true;
                }
                _ => {}
            }
        }
        if ui.inp.wheel != 0 {
            self.top = (self.top as i32 + ui.inp.wheel * 3).clamp(0, self.lines.len().saturating_sub(1) as i32) as usize;
        } else {
            if self.cy < self.top {
                self.top = self.cy;
            } else if self.cy >= self.top + rows {
                self.top = self.cy + 1 - rows;
            }
        }
        // clicking places the caret
        let text_x = 56;
        if ui.inp.pressed && ui.mouse().1 > bar {
            let (mx, my) = ui.mouse();
            let row = ((my - bar - 8) / lh).max(0) as usize + self.top;
            self.cy = row.min(self.lines.len() - 1);
            let line = &self.lines[self.cy];
            let mut x = text_x;
            let mut col = 0;
            for c in line.chars() {
                let cw = gfx::text_width(&f, &alloc::format!("{}", c));
                if x + cw / 2 > mx {
                    break;
                }
                x += cw;
                col += 1;
            }
            self.cx = col;
        }
        if ui.hovered((0, bar, w, h - bar)) {
            ui.cursor = super::Cursor::Text;
        }
        ui.cv.push((0, bar, w, h - bar), false);
        let gutter = if th.dark { 0x26272C } else { 0xF4F4F6 };
        ui.cv.rect(0, bar, 44, h - bar, gutter);
        for (k, line) in self.lines.iter().enumerate().skip(self.top).take(rows + 1) {
            let y = bar + 8 + (k - self.top) as i32 * lh;
            if k == self.cy {
                ui.cv.rect_a(44, y - 3, w - 44, lh, th.accent, 22);
            }
            ui.cv.text_right(36, y, &alloc::format!("{}", k + 1), &f, th.text3);
            ui.cv.text(text_x, y, line, &f, th.text);
            if k == self.cy && (ui.now / 530) % 2 == 0 {
                let before: String = line.chars().take(self.cx).collect();
                let cx = text_x + gfx::text_width(&f, &before);
                ui.cv.rect(cx, y - 1, 2, f.height() as i32 + 2, th.accent);
            }
        }
        ui.cv.pop();
        ui.animate(600);
    }
}
