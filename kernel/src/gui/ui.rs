//! The toolkit apps are written with: an immediate-mode UI. Every frame the
//! app describes its window (`ui.button(...)` draws a button and says if it
//! was clicked); state that must survive frames lives in the app and in
//! [`UiState`]. The runner [`run`] repaints only when something happened.

use super::gfx::{self, Canvas, Rect};
use super::theme::{self, Theme};
use super::{Cursor, Event};
use crate::graphics::display::Display;
use crate::graphics::font::Font;
use crate::input::Key;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::Ordering;

pub const SHIFT: u8 = 1;
pub const CTRL: u8 = 2;
pub const ALT: u8 = 4;
pub const SUPER: u8 = 8;

#[derive(Default, Clone)]
pub struct Input {
    pub mouse: (i32, i32),
    pub down: bool,
    pub pressed: bool,
    pub released: bool,
    pub rpressed: bool,
    pub clicks: u8,
    pub wheel: i32,
    pub keys: Vec<(Key, u8)>,
    pub inside: bool,
    pub focused: bool,
    pub menu: Option<(usize, usize)>,
    pub resized: bool,
    pub drop_path: Option<String>,
}

#[derive(Clone)]
pub struct MenuItem {
    pub label: String,
    pub hint: String,
    pub enabled: bool,
    pub separator: bool,
}

impl MenuItem {
    pub fn new(label: &str) -> MenuItem {
        MenuItem { label: String::from(label), hint: String::new(), enabled: true, separator: false }
    }
    pub fn hint(mut self, h: &str) -> MenuItem {
        self.hint = String::from(h);
        self
    }
    pub fn disabled(mut self, off: bool) -> MenuItem {
        self.enabled = !off;
        self
    }
    pub fn sep() -> MenuItem {
        MenuItem { label: String::new(), hint: String::new(), enabled: false, separator: true }
    }
}

pub struct OpenMenu {
    pub id: u64,
    pub x: i32,
    pub y: i32,
    pub items: Vec<MenuItem>,
    pub hover: Option<usize>,
    pub opened: u64,
}

#[derive(Default)]
pub struct UiState {
    pub active: u64,
    pub focus: u64,
    pub caret: usize,
    pub scroll: BTreeMap<u64, i32>,
    pub menu: Option<OpenMenu>,
    pub picked: Option<(u64, usize)>,
    pub animate_until: u64,
    pub drag_value: i32,
    pub last_click: (u64, i32, i32),
    pub caret_owner: u64,
}

pub fn id(s: &str) -> u64 {
    // FNV-1a
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

pub fn idn(s: &str, n: usize) -> u64 {
    id(s) ^ (n as u64).wrapping_mul(0x9E3779B97F4A7C15)
}

pub fn inside(p: (i32, i32), r: Rect) -> bool {
    p.0 >= r.0 && p.1 >= r.1 && p.0 < r.0 + r.2 && p.1 < r.1 + r.3
}

pub struct Ui<'a> {
    pub cv: Canvas<'a>,
    pub th: Theme,
    pub inp: Input,
    pub st: &'a mut UiState,
    pub w: i32,
    pub h: i32,
    pub font: Font,
    pub bold: Font,
    pub big: Font,
    pub title: Font,
    pub cursor: Cursor,
    pub now: u64,
    pub redraw_soon: bool,
}

#[derive(Default, Clone, Copy)]
pub struct Resp {
    pub clicked: bool,
    pub double: bool,
    pub right: bool,
    pub hover: bool,
    pub pressed: bool,
}

impl<'a> Ui<'a> {
    pub fn new(cv: Canvas<'a>, inp: Input, st: &'a mut UiState) -> Ui<'a> {
        let (w, h) = (cv.w, cv.h);
        let mut ui = Ui {
            cv,
            th: theme::current(),
            inp,
            st,
            w,
            h,
            font: gfx::font(16, false),
            bold: gfx::font(16, true),
            big: gfx::font(20, true),
            title: gfx::font(24, true),
            cursor: Cursor::Arrow,
            now: crate::time::uptime_ms(),
            redraw_soon: false,
        };
        ui.begin_menu();
        ui
    }

    /// Asks for another frame soon (animations).
    pub fn animate(&mut self, ms: u64) {
        self.st.animate_until = self.st.animate_until.max(self.now + ms);
    }

    pub fn mouse(&self) -> (i32, i32) {
        let (ox, oy) = self.cv.origin();
        (self.inp.mouse.0 - ox, self.inp.mouse.1 - oy)
    }

    pub fn hovered(&self, r: Rect) -> bool {
        self.inp.inside && inside(self.mouse(), r)
    }

    /// Clicks, hover and right-clicks on a rectangle.
    pub fn interact(&mut self, id: u64, r: Rect) -> Resp {
        let hover = self.hovered(r);
        let mut resp = Resp { hover, ..Default::default() };
        if hover && self.inp.pressed {
            self.st.active = id;
            resp.pressed = true;
            if self.inp.clicks >= 2 {
                resp.double = true;
            }
        }
        if hover && self.inp.rpressed {
            resp.right = true;
        }
        if self.inp.released && self.st.active == id {
            if hover {
                resp.clicked = true;
            }
            self.st.active = 0;
        }
        resp
    }

    pub fn keys(&self) -> &[(Key, u8)] {
        &self.inp.keys
    }

    pub fn key(&self, k: Key) -> bool {
        self.inp.keys.iter().any(|(x, _)| *x == k)
    }

    pub fn key_mod(&self, k: Key, m: u8) -> bool {
        self.inp.keys.iter().any(|(x, mm)| *x == k && mm & m != 0)
    }

    // ---- drawing helpers ----

    pub fn fill(&mut self, c: u32) {
        self.cv.fill(c);
    }

    pub fn text(&mut self, x: i32, y: i32, s: &str, c: u32) -> i32 {
        let f = self.font;
        self.cv.text(x, y, s, &f, c)
    }

    pub fn text_bold(&mut self, x: i32, y: i32, s: &str, c: u32) -> i32 {
        let f = self.bold;
        self.cv.text(x, y, s, &f, c)
    }

    pub fn label(&mut self, x: i32, y: i32, s: &str) {
        let c = self.th.text;
        self.text(x, y, s, c);
    }

    pub fn muted(&mut self, x: i32, y: i32, s: &str) {
        let c = self.th.text2;
        self.text(x, y, s, c);
    }

    pub fn heading(&mut self, x: i32, y: i32, s: &str) {
        let (f, c) = (self.title, self.th.text);
        self.cv.text(x, y, s, &f, c);
    }

    pub fn wrapped(&mut self, x: i32, y: i32, w: i32, s: &str, c: u32) -> i32 {
        let f = self.font;
        let mut yy = y;
        for line in gfx::wrap(&f, s, w) {
            self.cv.text(x, yy, &line, &f, c);
            yy += f.height() as i32 + 4;
        }
        yy - y
    }

    pub fn separator(&mut self, x: i32, y: i32, w: i32) {
        let c = self.th.separator;
        self.cv.hline(x, y, w, c, 255);
    }

    // ---- controls ----

    pub fn button(&mut self, r: Rect, label: &str) -> bool {
        self.button_ex(id(label) ^ (r.0 as u64) << 32 ^ r.1 as u64, r, label, false)
    }

    pub fn primary(&mut self, r: Rect, label: &str) -> bool {
        self.button_ex(id(label) ^ (r.0 as u64) << 32 ^ r.1 as u64, r, label, true)
    }

    pub fn button_ex(&mut self, id: u64, r: Rect, label: &str, primary: bool) -> bool {
        let resp = self.interact(id, r);
        let pressed = self.st.active == id && resp.hover;
        let th = self.th;
        let (bg, fg) = if primary {
            let c = if pressed { gfx::darken(th.accent, 40) } else if resp.hover { gfx::lighten(th.accent, 20) } else { th.accent };
            (c, th.on_accent)
        } else {
            let c = if pressed { gfx::darken(th.control_hover, 20) } else if resp.hover { th.control_hover } else { th.control };
            (c, th.text)
        };
        if !primary {
            self.cv.shadow(r.0, r.1 + 1, r.2, r.3, 6, 2, 40);
            self.cv.round_border(r.0, r.1, r.2, r.3, 6, bg, th.control_border);
        } else {
            self.cv.shadow(r.0, r.1 + 1, r.2, r.3, 6, 3, 60);
            self.cv.round(r.0, r.1, r.2, r.3, 6, bg);
        }
        let f = self.font;
        let tw = gfx::text_width(&f, label);
        self.cv.text(r.0 + (r.2 - tw) / 2, r.1 + (r.3 - f.height() as i32) / 2 - 1, label, &f, fg);
        if resp.hover {
            self.cursor = Cursor::Hand;
        }
        resp.clicked
    }

    /// A borderless button that only shows its background when hovered.
    pub fn flat(&mut self, id: u64, r: Rect, selected: bool) -> Resp {
        let resp = self.interact(id, r);
        let th = self.th;
        if selected {
            self.cv.round(r.0, r.1, r.2, r.3, 6, th.accent);
        } else if resp.hover {
            self.cv.round_a(r.0, r.1, r.2, r.3, 6, th.text, if th.dark { 22 } else { 18 });
        }
        resp
    }

    pub fn toggle(&mut self, id: u64, x: i32, y: i32, value: &mut bool) -> bool {
        let r = (x, y, 42, 24);
        let resp = self.interact(id, r);
        if resp.clicked {
            *value = !*value;
        }
        let th = self.th;
        let bg = if *value { th.accent } else if th.dark { 0x48494F } else { 0xD5D5DA };
        self.cv.round(r.0, r.1, r.2, r.3, 12, bg);
        let kx = if *value { x + 42 - 22 } else { x + 2 };
        self.cv.shadow(kx, y + 2, 20, 20, 10, 3, 60);
        self.cv.circle(kx + 10, y + 12, 10, 0xFFFFFF);
        if resp.hover {
            self.cursor = Cursor::Hand;
        }
        resp.clicked
    }

    pub fn slider(&mut self, id: u64, r: Rect, value: &mut i32, min: i32, max: i32) -> bool {
        let resp = self.interact(id, r);
        let mut changed = false;
        if self.st.active == id && self.inp.down {
            let mx = self.mouse().0;
            let v = min + (mx - r.0).clamp(0, r.2) * (max - min) / r.2.max(1);
            if v != *value {
                *value = v;
                changed = true;
            }
        }
        let th = self.th;
        let cy = r.1 + r.3 / 2;
        let track = if th.dark { 0x4A4B51 } else { 0xD0D0D5 };
        self.cv.round(r.0, cy - 2, r.2, 4, 2, track);
        let fx = r.0 + (*value - min) * r.2 / (max - min).max(1);
        self.cv.round(r.0, cy - 2, (fx - r.0).max(4), 4, 2, th.accent);
        self.cv.shadow(fx - 9, cy - 9, 18, 18, 9, 3, 70);
        self.cv.circle(fx, cy, 9, 0xFFFFFF);
        if resp.hover {
            self.cursor = Cursor::Hand;
        }
        changed
    }

    pub fn segmented(&mut self, id: u64, r: Rect, labels: &[&str], value: &mut usize) -> bool {
        let th = self.th;
        self.cv.round_border(r.0, r.1, r.2, r.3, 7, if th.dark { 0x2C2D32 } else { 0xE3E3E7 }, th.control_border);
        let n = labels.len().max(1) as i32;
        let seg = r.2 / n;
        let mut changed = false;
        for (i, l) in labels.iter().enumerate() {
            let sr = (r.0 + i as i32 * seg + 2, r.1 + 2, seg - 4, r.3 - 4);
            let resp = self.interact(id ^ (i as u64 + 1) << 40, sr);
            if resp.clicked && *value != i {
                *value = i;
                changed = true;
            }
            if *value == i {
                self.cv.shadow(sr.0, sr.1, sr.2, sr.3, 5, 2, 50);
                self.cv.round(sr.0, sr.1, sr.2, sr.3, 5, if th.dark { 0x5A5B62 } else { 0xFFFFFF });
            }
            let f = self.font;
            let tw = gfx::text_width(&f, l);
            self.cv.text(sr.0 + (sr.2 - tw) / 2, sr.1 + (sr.3 - f.height() as i32) / 2 - 1, l, &f, th.text);
        }
        changed
    }

    /// A one-line text field. Returns true when Enter was pressed.
    pub fn field(&mut self, id: u64, r: Rect, text: &mut String, placeholder: &str) -> bool {
        let resp = self.interact(id, r);
        if resp.pressed {
            self.st.focus = id;
            self.st.caret = text.chars().count();
        }
        if self.inp.pressed && !resp.hover && self.st.focus == id {
            self.st.focus = 0;
        }
        let focused = self.st.focus == id;
        if focused && self.st.caret_owner != id {
            // focus arrived from elsewhere: start at the end of the text
            self.st.caret_owner = id;
            self.st.caret = text.chars().count();
        }
        let mut submitted = false;
        if focused {
            let keys = self.inp.keys.clone();
            for (k, m) in keys {
                let n = text.chars().count();
                self.st.caret = self.st.caret.min(n);
                match k {
                    Key::Char('\n') => submitted = true,
                    Key::Char('\u{8}') => {
                        if self.st.caret > 0 {
                            let idx: usize = text.char_indices().nth(self.st.caret - 1).map(|(i, _)| i).unwrap_or(0);
                            text.remove(idx);
                            self.st.caret -= 1;
                        }
                    }
                    Key::Delete => {
                        if self.st.caret < n {
                            let idx: usize = text.char_indices().nth(self.st.caret).map(|(i, _)| i).unwrap_or(0);
                            text.remove(idx);
                        }
                    }
                    Key::Left => self.st.caret = self.st.caret.saturating_sub(1),
                    Key::Right => self.st.caret = (self.st.caret + 1).min(n),
                    Key::Home => self.st.caret = 0,
                    Key::End => self.st.caret = n,
                    Key::Char(c) if !c.is_control() && m & (CTRL | SUPER) == 0 => {
                        let idx = text.char_indices().nth(self.st.caret).map(|(i, _)| i).unwrap_or(text.len());
                        text.insert(idx, c);
                        self.st.caret += 1;
                    }
                    _ => {}
                }
            }
        }
        let th = self.th;
        if focused {
            self.cv.round(r.0 - 3, r.1 - 3, r.2 + 6, r.3 + 6, 9, gfx::mix(th.window, th.accent, 120));
        }
        self.cv.round_border(r.0, r.1, r.2, r.3, 6, th.field, if focused { th.accent } else { th.control_border });
        let f = self.font;
        let ty = r.1 + (r.3 - f.height() as i32) / 2 - 1;
        self.cv.push((r.0 + 6, r.1, r.2 - 12, r.3), false);
        if text.is_empty() && !focused {
            self.cv.text(r.0 + 9, ty, placeholder, &f, th.text3);
        } else {
            let before: String = text.chars().take(self.st.caret.min(text.chars().count())).collect();
            let cw = gfx::text_width(&f, &before);
            let shift = (cw - (r.2 - 24)).max(0);
            self.cv.text(r.0 + 9 - shift, ty, text, &f, th.text);
            if focused && (self.now / 530) % 2 == 0 {
                self.cv.rect(r.0 + 9 - shift + cw, ty, 2, f.height() as i32, th.accent);
            }
        }
        self.cv.pop();
        if focused {
            self.animate(600);
        }
        if resp.hover {
            self.cursor = Cursor::Text;
        }
        submitted
    }

    pub fn progress(&mut self, r: Rect, frac_1000: i32) {
        let th = self.th;
        self.cv.round(r.0, r.1, r.2, r.3, r.3 / 2, if th.dark { 0x3A3B41 } else { 0xDADADF });
        let w = r.2 * frac_1000.clamp(0, 1000) / 1000;
        if w > 0 {
            self.cv.round(r.0, r.1, w.max(r.3), r.3, r.3 / 2, th.accent);
        }
    }

    /// A vertical scroll area: returns the scroll offset to draw the content
    /// at. Call `scroll_end` after drawing.
    pub fn scroll_begin(&mut self, id: u64, r: Rect, content_h: i32) -> i32 {
        let max = (content_h - r.3).max(0);
        let mut off = *self.st.scroll.get(&id).unwrap_or(&0);
        if self.hovered(r) && self.inp.wheel != 0 {
            off -= self.inp.wheel * 48;
        }
        off = off.clamp(0, max);
        self.st.scroll.insert(id, off);
        self.cv.push(r, false);
        off
    }

    pub fn scroll_end(&mut self, id: u64, r: Rect, content_h: i32) {
        self.cv.pop();
        if content_h > r.3 {
            let off = *self.st.scroll.get(&id).unwrap_or(&0);
            let th = h_bar(r.3, content_h);
            let ty = r.1 + off * (r.3 - th) / (content_h - r.3).max(1);
            let c = if self.th.dark { 0x8A8C93 } else { 0x8A8A90 };
            self.cv.round_a(r.0 + r.2 - 8, ty + 2, 5, th - 4, 3, c, 150);
            // drag the bar
            let bar = (r.0 + r.2 - 12, r.1, 12, r.3);
            let rid = id ^ 0xBA5E;
            let resp = self.interact(rid, bar);
            let _ = resp;
            if self.st.active == rid && self.inp.down {
                let my = self.mouse().1;
                let off = ((my - r.1 - th / 2) * (content_h - r.3) / (r.3 - th).max(1)).clamp(0, content_h - r.3);
                self.st.scroll.insert(id, off);
                self.redraw_soon = true;
            }
        }
    }

    pub fn scroll_to(&mut self, id: u64, off: i32) {
        self.st.scroll.insert(id, off.max(0));
    }

    pub fn scroll_of(&self, id: u64) -> i32 {
        *self.st.scroll.get(&id).unwrap_or(&0)
    }

    // ---- menus ----

    /// Opens a pop-up menu at a point of the window.
    pub fn open_menu(&mut self, id: u64, x: i32, y: i32, items: Vec<MenuItem>) {
        let (ox, oy) = self.cv.origin();
        self.st.menu = Some(OpenMenu { id, x: x + ox, y: y + oy, items, hover: None, opened: self.now });
    }

    /// The item picked from menu `id` this frame, if any.
    pub fn menu_pick(&mut self, id: u64) -> Option<usize> {
        match self.st.picked {
            Some((m, i)) if m == id => {
                self.st.picked = None;
                Some(i)
            }
            _ => None,
        }
    }

    pub fn menu_open(&self) -> bool {
        self.st.menu.is_some()
    }

    fn menu_geometry(&self, m: &OpenMenu) -> (Rect, Vec<Rect>) {
        let f = self.font;
        let mut w = 160;
        for it in &m.items {
            w = w.max(gfx::text_width(&f, &it.label) + gfx::text_width(&f, &it.hint) + 60);
        }
        let mut h = 6;
        let mut rows = Vec::new();
        for it in &m.items {
            let ih = if it.separator { 9 } else { 26 };
            rows.push((0, h, w, ih));
            h += ih;
        }
        h += 6;
        let x = m.x.min(self.w - w - 4).max(4);
        let y = if m.y + h > self.h - 4 { (m.y - h).max(4) } else { m.y };
        let rows = rows.into_iter().map(|(_, ry, rw, rh)| (x, y + ry, rw, rh)).collect();
        ((x, y, w, h), rows)
    }

    /// Before the widgets see the mouse: clicks on an open menu go to it.
    fn begin_menu(&mut self) {
        let Some(m) = self.st.menu.take() else { return };
        let (frame, rows) = self.menu_geometry(&m);
        let mut keep = Some(m);
        let p = self.inp.mouse;
        if let Some(m) = keep.as_mut() {
            m.hover = rows.iter().position(|r| inside(p, *r)).filter(|&i| m.items[i].enabled && !m.items[i].separator);
        }
        let esc = self.inp.keys.iter().any(|(k, _)| *k == Key::Char('\u{1b}'));
        if self.inp.pressed || self.inp.rpressed || esc {
            if let Some(m) = keep.take() {
                if inside(p, frame) && !esc {
                    if let Some(i) = m.hover {
                        self.st.picked = Some((m.id, i));
                    } else {
                        keep = Some(m);
                    }
                }
            }
            self.inp.pressed = false;
            self.inp.rpressed = false;
            self.inp.released = false;
        }
        if keep.is_some() {
            // nothing under the menu reacts to the mouse
            self.inp.inside = !inside(p, frame) && self.inp.inside;
            self.inp.keys.retain(|(k, _)| !matches!(k, Key::Up | Key::Down | Key::Char('\n')));
        }
        self.st.menu = keep;
    }

    /// Draws what floats above everything (the open menu).
    pub fn finish(&mut self) {
        let Some(m) = self.st.menu.take() else { return };
        let (frame, rows) = self.menu_geometry(&m);
        draw_menu(&mut self.cv, &self.th, &self.font, frame, &rows, &m.items, m.hover, self.now.saturating_sub(m.opened));
        if self.now < m.opened + 150 {
            self.animate(160);
        }
        self.st.menu = Some(m);
    }
}

fn h_bar(view: i32, content: i32) -> i32 {
    (view * view / content.max(1)).clamp(24, view)
}

/// A menu in the macOS style (shared with the desktop's own menus).
pub fn draw_menu(cv: &mut Canvas, th: &Theme, f: &Font, frame: Rect, rows: &[Rect], items: &[MenuItem], hover: Option<usize>, age_ms: u64) {
    let fade = (age_ms.min(120) * 255 / 120) as u32;
    let saved = cv.opacity;
    cv.opacity = fade.max(60);
    cv.shadow(frame.0, frame.1 + 4, frame.2, frame.3, 9, 18, 90);
    cv.round_border(frame.0, frame.1, frame.2, frame.3, 9, th.menu, th.menu_border);
    for (i, (it, r)) in items.iter().zip(rows).enumerate() {
        if it.separator {
            cv.hline(r.0 + 10, r.1 + 4, r.2 - 20, th.separator, 255);
            continue;
        }
        let selected = hover == Some(i);
        if selected {
            cv.round(r.0 + 5, r.1, r.2 - 10, r.3, 5, th.accent);
        }
        let c = if !it.enabled { th.text3 } else if selected { th.on_accent } else { th.text };
        cv.text(r.0 + 16, r.1 + (r.3 - f.height() as i32) / 2 - 1, &it.label, f, c);
        if !it.hint.is_empty() {
            let hc = if selected { th.on_accent } else { th.text3 };
            cv.text_right(r.0 + r.2 - 14, r.1 + (r.3 - f.height() as i32) / 2 - 1, &it.hint, f, hc);
        }
    }
    cv.opacity = saved;
}

/// Geometry of a menu of `items` at (x, y) inside a w x h area (shared with the desktop).
pub fn menu_layout(f: &Font, x: i32, y: i32, items: &[MenuItem], area_w: i32, area_h: i32) -> (Rect, Vec<Rect>) {
    let mut w = 180;
    for it in items {
        w = w.max(gfx::text_width(f, &it.label) + gfx::text_width(f, &it.hint) + 60);
    }
    let mut h = 6;
    let mut rows = Vec::new();
    for it in items {
        let ih = if it.separator { 9 } else { 26 };
        rows.push((h, ih));
        h += ih;
    }
    h += 6;
    let x = x.min(area_w - w - 4).max(4);
    let y = if y + h > area_h - 4 { (y - h).max(4) } else { y };
    (
        (x, y, w, h),
        rows.into_iter().map(|(ry, rh)| (x, y + ry, w, rh)).collect(),
    )
}

// ---------------------------------------------------------------- the runner

pub trait App {
    /// Draws the whole window.
    fn frame(&mut self, ui: &mut Ui);
    /// The window title (shown in the title bar and the menu bar).
    fn title(&self) -> String;
    /// Menus for the menu bar: (menu name, items).
    fn menus(&self) -> Vec<(&'static str, Vec<MenuItem>)> {
        Vec::new()
    }
    fn on_menu(&mut self, _menu: usize, _item: usize) {}
    /// Called before each frame; return true to keep animating.
    fn tick(&mut self) -> bool {
        false
    }
    /// The app wants to end (closes its window).
    fn done(&self) -> bool {
        false
    }
}

/// Runs an app in the calling thread's window until the window is closed.
pub fn run(app: &mut dyn App) {
    let Some(ctx) = crate::sched::context() else { return };
    let mut st = UiState::default();
    let mut dirty = true;
    let mut inp = Input::default();
    let mut theme_gen = theme::GENERATION.load(Ordering::Relaxed);
    let mut last_frame = 0u64;
    let mut last_menus_title = String::new();
    loop {
        if ctx.is_closed() || app.done() {
            break;
        }
        // gather everything that happened
        inp.pressed = false;
        inp.released = false;
        inp.rpressed = false;
        inp.wheel = 0;
        inp.keys.clear();
        inp.menu = None;
        inp.resized = false;
        inp.clicks = 0;
        inp.drop_path = None;
        let mut got = false;
        loop {
            let ev = ctx.events.lock().pop_front();
            let Some(ev) = ev else { break };
            got = true;
            match ev {
                Event::MouseMove { x, y } => {
                    inp.mouse = (x, y);
                    inp.inside = true;
                }
                Event::MouseLeave => inp.inside = false,
                Event::MouseDown { x, y, button, clicks } => {
                    inp.mouse = (x, y);
                    inp.inside = true;
                    if button == 0 {
                        inp.down = true;
                        inp.pressed = true;
                        inp.clicks = clicks;
                    } else if button == 1 {
                        inp.rpressed = true;
                    }
                    // one press per frame: handle the rest next time
                    if inp.pressed || inp.rpressed {
                        break;
                    }
                }
                Event::MouseUp { x, y, button } => {
                    inp.mouse = (x, y);
                    if button == 0 {
                        inp.down = false;
                        inp.released = true;
                        break;
                    }
                }
                Event::Wheel { dy } => inp.wheel += dy,
                Event::Key(k, m) => inp.keys.push((k, m)),
                Event::Menu(a, b) => {
                    inp.menu = Some((a, b));
                    app.on_menu(a, b);
                }
                Event::Resize { .. } => inp.resized = true,
                Event::Focus(f) => inp.focused = f,
                Event::Drop(p) => inp.drop_path = Some(p),
                _ => {}
            }
        }
        inp.focused = ctx.focused.load(Ordering::Relaxed);
        let now = crate::time::uptime_ms();
        let tg = theme::GENERATION.load(Ordering::Relaxed);
        if tg != theme_gen {
            theme_gen = tg;
            dirty = true;
        }
        let animating = app.tick() || now < st.animate_until;
        if got || dirty || (animating && now >= last_frame + 15) {
            last_frame = now;
            dirty = false;
            let mut redraw_soon = false;
            let mut cursor = Cursor::Arrow;
            {
                let mut surface = ctx.surface.lock();
                let (w, h) = (surface.width(), surface.height());
                let px = surface.pixels32();
                let cv = Canvas::new(px, w, h);
                let mut ui = Ui::new(cv, inp.clone(), &mut st);
                app.frame(&mut ui);
                ui.finish();
                cursor = ui.cursor;
                redraw_soon = ui.redraw_soon;
                drop(ui);
                surface.damage_rows(0, h);
            }
            ctx.cursor.store(cursor as u8, Ordering::Relaxed);
            let title = app.title();
            if title != last_menus_title {
                *ctx.title.lock() = title.clone();
                last_menus_title = title;
            }
            {
                let menus = app.menus();
                let mut m = ctx.menus.lock();
                m.clear();
                for (name, items) in menus {
                    m.push((String::from(name), items.iter().map(|i| if i.separator { String::from("-") } else { i.label.clone() }).collect()));
                }
            }
            if redraw_soon {
                dirty = true;
            }
            crate::sched::note_progress();
        }
        crate::sched::yield_idle();
    }
}
