//! The file explorer: browse the file system, open text files, run
//! programs, make folders and delete things.

use super::draw::{self, accent, Icon, BG, BG_DARK, BG_HOVER, BG_PANEL, BORDER, TEXT, TEXT_FAINT, TEXT_MUTED};
use super::widgets::{button, inside, AppUi};
use super::Event;
use crate::fs::{Entry, FILESYSTEM};
use crate::graphics::display::Display;
use crate::graphics::font::Font;
use crate::graphics::framebuffer::Framebuffer;
use crate::input::Key;
use alloc::string::String;
use alloc::vec::Vec;

enum Mode {
    Browse,
    /// Reading a file.
    View { name: String, lines: Vec<String>, scroll: usize },
    /// Typing a name for a new folder / file.
    Prompt { folder: bool, text: String },
}

pub struct Files {
    cwd: String,
    entries: Vec<Entry>,
    selected: Option<usize>,
    scroll: usize,
    hover: Option<usize>,
    mode: Mode,
    status: String,
    last_click: (u64, usize),
    history: Vec<String>,
    mouse: (i32, i32),
}

const PLACES: [(&str, &str); 3] = [("Home", "/"), ("Programs", "/programs"), ("Documents", "/documents")];

impl Files {
    pub fn new() -> Files {
        let mut f = Files {
            cwd: String::from("/"),
            entries: Vec::new(),
            selected: None,
            scroll: 0,
            hover: None,
            mode: Mode::Browse,
            status: String::new(),
            last_click: (0, usize::MAX),
            history: Vec::new(),
            mouse: (0, 0),
        };
        f.reload();
        f
    }

    fn reload(&mut self) {
        let cwd = self.cwd.clone();
        self.entries = FILESYSTEM.lock().as_ref().and_then(|fs| fs.entries(&cwd).ok()).unwrap_or_default();
        // folders first
        self.entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
        self.selected = None;
        self.scroll = 0;
    }

    fn go(&mut self, path: &str) {
        let ok = FILESYSTEM.lock().as_ref().map(|fs| fs.is_dir(path)).unwrap_or(false);
        if ok {
            self.history.push(self.cwd.clone());
            self.cwd = String::from(path);
            self.reload();
            self.status.clear();
        } else {
            self.status = alloc::format!("{} does not exist yet", path);
        }
    }

    fn up(&mut self) {
        if self.cwd == "/" {
            return;
        }
        let parent = match self.cwd.rfind('/') {
            Some(0) | None => String::from("/"),
            Some(i) => String::from(&self.cwd[..i]),
        };
        self.go(&parent);
    }

    fn back(&mut self) {
        if let Some(p) = self.history.pop() {
            self.cwd = p;
            self.reload();
        }
    }

    fn path_of(&self, name: &str) -> String {
        if self.cwd == "/" {
            alloc::format!("/{}", name)
        } else {
            alloc::format!("{}/{}", self.cwd, name)
        }
    }

    fn open_selected(&mut self) {
        let Some(i) = self.selected else { return };
        let Some(entry) = self.entries.get(i).cloned() else { return };
        let path = self.path_of(&entry.name);
        if entry.is_dir {
            self.go(&path);
            return;
        }
        let data = FILESYSTEM.lock().as_ref().and_then(|fs| fs.read_file(&path).ok());
        match data {
            Some(bytes) => {
                let text = String::from_utf8_lossy(&bytes).into_owned();
                let lines = text.lines().map(String::from).collect();
                self.mode = Mode::View { name: entry.name, lines, scroll: 0 };
            }
            None => self.status = String::from("could not read that file"),
        }
    }

    fn delete_selected(&mut self) {
        let Some(i) = self.selected else { return };
        let Some(entry) = self.entries.get(i).cloned() else { return };
        let path = self.path_of(&entry.name);
        let result = FILESYSTEM.lock().as_mut().map(|fs| fs.delete(&path));
        match result {
            Some(Ok(())) => {
                crate::pfs::sync_if_dirty();
                self.status = alloc::format!("deleted {}", entry.name);
                self.reload();
            }
            _ => self.status = String::from("could not delete that"),
        }
    }

    fn create(&mut self, folder: bool, name: &str) {
        let name = name.trim();
        if name.is_empty() || name.contains('/') {
            self.status = String::from("that is not a usable name");
            return;
        }
        let path = self.path_of(name);
        let result = FILESYSTEM.lock().as_mut().map(|fs| if folder { fs.create_dir(&path) } else { fs.create_file(&path) });
        match result {
            Some(Ok(())) => {
                crate::pfs::sync_if_dirty();
                self.status = alloc::format!("created {}", name);
                self.reload();
            }
            _ => self.status = String::from("could not create that (does it exist already?)"),
        }
    }

    // ---- layout ----------------------------------------------------------------

    fn font(&self) -> Font {
        draw::font(15, false)
    }

    fn side_w(&self) -> i32 {
        150
    }

    fn row_h(&self) -> i32 {
        28
    }

    fn list_top(&self) -> i32 {
        44 + 26
    }

    fn visible_rows(&self, h: i32) -> usize {
        ((h - self.list_top() - 28) / self.row_h()).max(1) as usize
    }

    fn toolbar_buttons(&self) -> [(i32, i32, i32, i32); 5] {
        let x = self.side_w() + 10;
        [(x, 8, 30, 28), (x + 36, 8, 30, 28), (x + 72, 8, 30, 28), (0, 8, 96, 28), (0, 8, 96, 28)]
    }
}

impl AppUi for Files {
    fn event(&mut self, ev: Event, w: i32, h: i32) -> bool {
        let rows = self.visible_rows(h);
        match (&mut self.mode, ev) {
            (Mode::View { scroll, lines, .. }, Event::Key(k)) => {
                match k {
                    Key::Char('\u{1b}') | Key::Char('q') => self.mode = Mode::Browse,
                    Key::Down => *scroll = (*scroll + 1).min(lines.len().saturating_sub(1)),
                    Key::Up => *scroll = scroll.saturating_sub(1),
                    Key::PageDown => *scroll = (*scroll + 15).min(lines.len().saturating_sub(1)),
                    Key::PageUp => *scroll = scroll.saturating_sub(15),
                    _ => {}
                }
                return true;
            }
            (Mode::View { .. }, Event::MouseDown { x, y, .. }) => {
                if inside(x, y, (w - 90, 8, 80, 28)) {
                    self.mode = Mode::Browse;
                }
                return true;
            }
            (Mode::Prompt { folder, text }, Event::Key(k)) => {
                let folder = *folder;
                match k {
                    Key::Char('\u{1b}') => self.mode = Mode::Browse,
                    Key::Char('\n') => {
                        let name = text.clone();
                        self.mode = Mode::Browse;
                        self.create(folder, &name);
                    }
                    Key::Char('\u{8}') => {
                        text.pop();
                    }
                    Key::Char(c) if !c.is_control() => text.push(c),
                    _ => {}
                }
                return true;
            }
            _ => {}
        }
        match ev {
            Event::MouseMove { x, y } => {
                self.mouse = (x, y);
                let top = self.list_top();
                let hover = if x > self.side_w() && y >= top {
                    let i = self.scroll + ((y - top) / self.row_h()) as usize;
                    if i < self.entries.len() { Some(i) } else { None }
                } else {
                    None
                };
                let changed = hover != self.hover;
                self.hover = hover;
                changed
            }
            Event::MouseDown { x, y, button: 0 } => {
                let now = crate::time::uptime_ms();
                // sidebar
                if x < self.side_w() {
                    for (k, (_, path)) in PLACES.iter().enumerate() {
                        if inside(x, y, (8, 50 + k as i32 * 30, self.side_w() - 16, 26)) {
                            self.go(path);
                        }
                    }
                    return true;
                }
                let b = self.toolbar_buttons();
                if inside(x, y, b[0]) {
                    self.back();
                    return true;
                }
                if inside(x, y, b[1]) {
                    self.up();
                    return true;
                }
                if inside(x, y, (w - 110, 8, 100, 28)) {
                    self.mode = Mode::Prompt { folder: true, text: String::new() };
                    return true;
                }
                if inside(x, y, (w - 220, 8, 100, 28)) {
                    self.mode = Mode::Prompt { folder: false, text: String::new() };
                    return true;
                }
                let top = self.list_top();
                if y >= top {
                    let i = self.scroll + ((y - top) / self.row_h()) as usize;
                    if i < self.entries.len() {
                        let double = now < self.last_click.0 + 400 && self.last_click.1 == i;
                        self.last_click = (now, i);
                        self.selected = Some(i);
                        if double {
                            self.open_selected();
                        }
                    } else {
                        self.selected = None;
                    }
                }
                true
            }
            Event::Key(k) => {
                match k {
                    Key::Down => {
                        let n = self.entries.len();
                        self.selected = Some(self.selected.map(|i| (i + 1).min(n.saturating_sub(1))).unwrap_or(0));
                    }
                    Key::Up => self.selected = Some(self.selected.map(|i| i.saturating_sub(1)).unwrap_or(0)),
                    Key::Char('\n') => self.open_selected(),
                    Key::Char('\u{8}') => self.up(),
                    Key::Delete => self.delete_selected(),
                    Key::Char('n') => self.mode = Mode::Prompt { folder: true, text: String::new() },
                    _ => {}
                }
                if let Some(i) = self.selected {
                    if i < self.scroll {
                        self.scroll = i;
                    } else if i >= self.scroll + rows {
                        self.scroll = i + 1 - rows;
                    }
                }
                true
            }
            _ => false,
        }
    }

    fn draw(&mut self, fb: &mut Framebuffer) {
        let (w, h) = (fb.width() as i32, fb.height() as i32);
        let f = self.font();
        let fb_bold = draw::font(15, true);
        draw::rect(fb, 0, 0, w, h, BG);

        // sidebar
        draw::rect(fb, 0, 0, self.side_w(), h, BG_DARK);
        draw::rect(fb, self.side_w(), 0, 1, h, BORDER);
        draw::text(fb, &draw::font(15, true), 14, 22, "Places", TEXT_FAINT);
        for (k, (name, path)) in PLACES.iter().enumerate() {
            let r = (8, 50 + k as i32 * 30, self.side_w() - 16, 26);
            let active = *path == self.cwd;
            if active {
                draw::round(fb, r.0, r.1, r.2, r.3, 6, BG_HOVER);
            }
            draw::icon(fb, Icon::Folder, r.0 + 6, r.1 + 4, 18, draw::full(fb));
            draw::text(fb, &f, r.0 + 30, r.1 + 4, name, if active { TEXT } else { TEXT_MUTED });
        }

        // toolbar
        let b = self.toolbar_buttons();
        let hover = |r: (i32, i32, i32, i32)| inside(self.mouse.0, self.mouse.1, r);
        button(fb, &f, b[0], "<", hover(b[0]), false);
        button(fb, &f, b[1], "^", hover(b[1]), false);
        let path_x = b[1].0 + 44;
        draw::text(fb, &fb_bold, path_x, 14, &draw::fit_text(&fb_bold, &self.cwd, w - path_x - 240), TEXT);
        let (nf, nd) = ((w - 220, 8, 100, 28), (w - 110, 8, 100, 28));
        button(fb, &f, nf, "New file", hover(nf), false);
        button(fb, &f, nd, "New folder", hover(nd), false);
        draw::rect(fb, self.side_w() + 1, 43, w, 1, BORDER);

        // list header
        let lx = self.side_w() + 12;
        draw::text(fb, &draw::font(15, true), lx + 30, 50, "Name", TEXT_FAINT);
        draw::text(fb, &draw::font(15, true), w - 110, 50, "Size", TEXT_FAINT);

        let rows = self.visible_rows(h);
        let top = self.list_top();
        for (k, entry) in self.entries.iter().enumerate().skip(self.scroll).take(rows) {
            let y = top + (k - self.scroll) as i32 * self.row_h();
            let selected = self.selected == Some(k);
            if selected {
                draw::round(fb, self.side_w() + 6, y, w - self.side_w() - 12, self.row_h() - 2, 6, accent());
            } else if self.hover == Some(k) {
                draw::round(fb, self.side_w() + 6, y, w - self.side_w() - 12, self.row_h() - 2, 6, BG_PANEL);
            }
            let icon = if entry.is_dir {
                Icon::Folder
            } else if entry.name.ends_with(".zs") {
                Icon::Text
            } else {
                Icon::Document
            };
            draw::icon(fb, icon, lx + 4, y + 3, 20, draw::full(fb));
            let color = if selected { 0x14171C } else { TEXT };
            let name = draw::fit_text(&f, &entry.name, w - lx - 190);
            draw::text(fb, &f, lx + 32, y + 5, &name, color);
            if !entry.is_dir {
                let size = format_size(entry.size);
                draw::text(fb, &f, w - 110, y + 5, &size, if selected { 0x14171C } else { TEXT_MUTED });
            }
        }
        if self.entries.is_empty() {
            draw::text(fb, &f, lx + 8, top + 8, "This folder is empty.", TEXT_FAINT);
        }

        // status line
        draw::rect(fb, self.side_w() + 1, h - 26, w, 26, BG_DARK);
        let status = if self.status.is_empty() {
            alloc::format!("{} items", self.entries.len())
        } else {
            self.status.clone()
        };
        draw::text(fb, &draw::font(15, false), lx, h - 22, &status, TEXT_MUTED);

        match &self.mode {
            Mode::View { name, lines, scroll } => {
                draw::rect(fb, 0, 0, w, h, BG);
                draw::rect(fb, 0, 0, w, 44, BG_DARK);
                draw::rect(fb, 0, 44, w, 1, BORDER);
                draw::text(fb, &fb_bold, 16, 14, &draw::fit_text(&fb_bold, name, w - 140), TEXT);
                let close = (w - 90, 8, 80, 28);
                button(fb, &f, close, "Close", inside(self.mouse.0, self.mouse.1, close), false);
                let mono = draw::font(16, false);
                let rows = ((h - 56) / (mono.height() as i32 + 2)).max(1) as usize;
                for (k, line) in lines.iter().enumerate().skip(*scroll).take(rows) {
                    let y = 52 + (k - *scroll) as i32 * (mono.height() as i32 + 2);
                    let text = draw::fit_text(&mono, line, w - 24);
                    draw::text(fb, &mono, 12, y, &text, TEXT);
                }
            }
            Mode::Prompt { folder, text } => {
                draw::tint(fb, 0, 0, w, h, 0x000000, 120);
                let (bw, bh) = (360, 110);
                let (bx, by) = ((w - bw) / 2, (h - bh) / 2);
                draw::round(fb, bx - 1, by - 1, bw + 2, bh + 2, 10, BORDER);
                draw::round(fb, bx, by, bw, bh, 10, BG_PANEL);
                let title = if *folder { "New folder" } else { "New file" };
                draw::text(fb, &fb_bold, bx + 16, by + 14, title, TEXT);
                draw::round(fb, bx + 16, by + 44, bw - 32, 32, 6, BG_DARK);
                let shown = alloc::format!("{}_", text);
                draw::text(fb, &f, bx + 24, by + 52, &draw::fit_text(&f, &shown, bw - 48), TEXT);
                draw::text(fb, &draw::font(15, false), bx + 16, by + 84, "Enter to create, Esc to cancel", TEXT_FAINT);
            }
            Mode::Browse => {}
        }
    }
}

fn format_size(n: usize) -> String {
    if n < 1024 {
        alloc::format!("{} B", n)
    } else if n < 1024 * 1024 {
        alloc::format!("{} KB", n / 1024)
    } else {
        alloc::format!("{} MB", n / (1024 * 1024))
    }
}
