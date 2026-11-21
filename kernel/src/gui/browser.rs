//! A very basic web browser: plain HTTP, headings, paragraphs, lists, links,
//! preformatted text. Links to pictures, sounds and animations are saved to
//! Downloads and opened in Media.

use super::gfx;
use super::ui::{id, App, MenuItem, Ui};
use crate::fs::FILESYSTEM;
use crate::graphics::font::Font;
use crate::input::Key;
use crate::net::http;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
struct Style {
    bold: bool,
    heading: u8,
    pre: bool,
}

#[derive(Clone)]
enum Tok {
    Word { text: String, space_before: bool, style: Style, link: Option<usize> },
    Break,
    Gap,
    Rule,
    Bullet(String),
    Indent(i32),
}

struct Page {
    title: String,
    toks: Vec<Tok>,
    links: Vec<String>,
}

fn decode_entities(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        if let Some(end) = rest.find(';').filter(|&e| e <= 10) {
            let name = &rest[1..end];
            let rep = match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some(' '),
                "mdash" | "ndash" => Some('-'),
                "hellip" => Some('.'),
                "copy" => Some('c'),
                _ => name.strip_prefix('#').and_then(|n| {
                    let v = if let Some(h) = n.strip_prefix('x').or_else(|| n.strip_prefix('X')) { u32::from_str_radix(h, 16).ok() } else { n.parse().ok() };
                    v.and_then(char::from_u32)
                }),
            };
            if let Some(c) = rep {
                out.push(if (c as u32) < 128 { c } else { '?' });
                rest = &rest[end + 1..];
                continue;
            }
        }
        out.push('&');
        rest = &rest[1..];
    }
    out.push_str(rest);
    out
}

fn attribute(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find(name) {
        let at = from + i;
        let before_ok = at == 0 || lower.as_bytes()[at - 1].is_ascii_whitespace();
        let after = tag[at + name.len()..].trim_start();
        if before_ok && after.starts_with('=') {
            let v = after[1..].trim_start();
            let value = if let Some(q) = v.strip_prefix('"') {
                q.split('"').next().unwrap_or("")
            } else if let Some(q) = v.strip_prefix('\'') {
                q.split('\'').next().unwrap_or("")
            } else {
                v.split(|c: char| c.is_whitespace() || c == '>').next().unwrap_or("")
            };
            return Some(decode_entities(value));
        }
        from = at + name.len();
    }
    None
}

fn parse_html(html: &str) -> Page {
    let mut toks = Vec::new();
    let mut links = Vec::new();
    let mut title = String::new();
    let mut style = Style::default();
    let mut link: Option<usize> = None;
    let mut in_title = false;
    let mut skip_until: Option<&'static str> = None;
    let mut ordered: Vec<Option<u32>> = Vec::new();
    let mut pending_space = false;
    let lower_all = html.to_ascii_lowercase();
    let mut i = 0;
    while i < html.len() {
        if !html.is_char_boundary(i) {
            i += 1;
            continue;
        }
        let rest = &html[i..];
        if let Some(end_tag) = skip_until {
            match lower_all[i..].find(end_tag) {
                Some(j) => {
                    i += j + end_tag.len();
                    skip_until = None;
                    if let Some(k) = html[i..].find('>') {
                        i += k + 1;
                    }
                }
                None => break,
            }
            continue;
        }
        if rest.starts_with("<!--") {
            match rest.find("-->") {
                Some(j) => i += j + 3,
                None => break,
            }
            continue;
        }
        if rest.starts_with('<') {
            let Some(end) = rest.find('>') else { break };
            let inner = &rest[1..end];
            i += end + 1;
            let closing = inner.starts_with('/');
            let body = inner.trim_start_matches('/');
            let name: String = body.chars().take_while(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
            match name.as_str() {
                "script" if !closing => skip_until = Some("</script"),
                "style" if !closing => skip_until = Some("</style"),
                "title" => in_title = !closing,
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                    toks.push(Tok::Gap);
                    if closing {
                        style.heading = 0;
                        style.bold = false;
                    } else {
                        style.heading = name.as_bytes()[1] - b'0';
                        style.bold = true;
                    }
                    toks.push(Tok::Break);
                }
                "p" | "blockquote" | "table" | "form" | "section" | "article" => toks.push(Tok::Gap),
                "div" | "tr" | "header" | "footer" | "nav" | "main" | "aside" | "center" | "dt" | "dd" | "figure" => toks.push(Tok::Break),
                "br" => toks.push(Tok::Break),
                "hr" => {
                    toks.push(Tok::Break);
                    toks.push(Tok::Rule);
                }
                "ul" | "ol" => {
                    if closing {
                        ordered.pop();
                        toks.push(Tok::Indent(-1));
                    } else {
                        ordered.push(if name == "ol" { Some(0) } else { None });
                        toks.push(Tok::Indent(1));
                    }
                    toks.push(Tok::Break);
                }
                "li" if !closing => {
                    let bullet = match ordered.last_mut() {
                        Some(Some(n)) => {
                            *n += 1;
                            alloc::format!("{}.", n)
                        }
                        _ => String::from("*"),
                    };
                    toks.push(Tok::Bullet(bullet));
                }
                "pre" => {
                    style.pre = !closing;
                    toks.push(Tok::Break);
                }
                "b" | "strong" | "th" => style.bold = !closing || style.heading > 0,
                "td" => pending_space = true,
                "a" => {
                    if closing {
                        link = None;
                    } else if let Some(href) = attribute(body, "href") {
                        links.push(href);
                        link = Some(links.len() - 1);
                    }
                }
                "img" => {
                    let alt = attribute(body, "alt").filter(|a| !a.is_empty()).unwrap_or_else(|| String::from("image"));
                    if let Some(src) = attribute(body, "src") {
                        links.push(src);
                        toks.push(Tok::Word { text: alloc::format!("[{}]", alt), space_before: true, style, link: Some(links.len() - 1) });
                    }
                }
                _ => {}
            }
            continue;
        }
        let end = rest.find('<').unwrap_or(rest.len());
        let text = decode_entities(&rest[..end]);
        i += end;
        if in_title {
            title.push_str(text.trim());
            continue;
        }
        if style.pre {
            for (n, line) in text.split('\n').enumerate() {
                if n > 0 {
                    toks.push(Tok::Break);
                }
                if !line.is_empty() {
                    toks.push(Tok::Word { text: String::from(line.trim_end_matches('\r')), space_before: false, style, link });
                }
            }
            continue;
        }
        let mut space = text.starts_with(char::is_whitespace) || pending_space;
        pending_space = false;
        for word in text.split_whitespace() {
            toks.push(Tok::Word { text: String::from(word), space_before: space, style, link });
            space = true;
        }
        if text.ends_with(char::is_whitespace) && !text.trim().is_empty() {
            pending_space = true;
        }
    }
    Page { title, toks, links }
}

struct Placed {
    x: i32,
    y: i32,
    text: String,
    style: Style,
    link: Option<usize>,
    w: i32,
}

fn font_for(s: Style) -> Font {
    let px = match s.heading {
        1 => 32,
        2 => 24,
        3 => 20,
        _ => 16,
    };
    gfx::font(px, s.bold || s.heading > 0)
}

fn layout(page: &Page, width: i32) -> (Vec<Placed>, Vec<i32>, i32) {
    let mut words = Vec::new();
    let mut rules = Vec::new();
    let (mut x, mut y, mut line_h, mut indent) = (0, 0, 22, 0);
    let mut at_start = true;
    let mut blank = true;
    let max_x = (width - 40).max(80);
    let newline = |x: &mut i32, y: &mut i32, lh: &mut i32, at: &mut bool, bl: &mut bool| {
        if !*at {
            *y += *lh + 2;
            *x = 0;
            *lh = 22;
            *at = true;
            *bl = false;
        }
    };
    for t in &page.toks {
        match t {
            Tok::Break => newline(&mut x, &mut y, &mut line_h, &mut at_start, &mut blank),
            Tok::Gap => {
                newline(&mut x, &mut y, &mut line_h, &mut at_start, &mut blank);
                if !blank && y > 0 {
                    y += 12;
                    blank = true;
                }
            }
            Tok::Rule => {
                newline(&mut x, &mut y, &mut line_h, &mut at_start, &mut blank);
                rules.push(y + 6);
                y += 14;
            }
            Tok::Indent(d) => indent = (indent + d * 26).max(0),
            Tok::Bullet(b) => {
                newline(&mut x, &mut y, &mut line_h, &mut at_start, &mut blank);
                let f = font_for(Style::default());
                let bw = gfx::text_width(&f, b);
                words.push(Placed { x: indent - bw - 6, y, text: b.clone(), style: Style::default(), link: None, w: bw });
                at_start = false;
                blank = false;
            }
            Tok::Word { text, space_before, style, link } => {
                let f = font_for(*style);
                let w = gfx::text_width(&f, text);
                let sp = if at_start || x == 0 || !*space_before { 0 } else { gfx::text_width(&f, " ") };
                if !style.pre && !at_start && indent + x + sp + w > max_x {
                    newline(&mut x, &mut y, &mut line_h, &mut at_start, &mut blank);
                }
                let sp = if at_start { 0 } else { sp };
                words.push(Placed { x: indent + x + sp, y, text: text.clone(), style: *style, link: *link, w });
                x += sp + w;
                line_h = line_h.max(f.height() as i32 + 6);
                at_start = false;
                blank = false;
            }
        }
    }
    newline(&mut x, &mut y, &mut line_h, &mut at_start, &mut blank);
    (words, rules, y + 30)
}

enum Load {
    Idle,
    Loading(String, usize),
    Done(Result<http::Response, String>),
}

pub struct Browser {
    address: String,
    page: Option<Page>,
    page_url: String,
    placed: Option<(Vec<Placed>, Vec<i32>, i32, i32)>,
    history: Vec<String>,
    forward: Vec<String>,
    load: Arc<Mutex<Load>>,
    cancel: Arc<AtomicBool>,
    status: String,
    hover: Option<usize>,
    progress_at: u64,
}

const HOME: &str = "<h1>zero browser</h1><p>A very small web browser. Type an <b>http://</b> address above and press Enter. Pages that need encryption (https) do not work yet.</p>\
<h3>Try these</h3><ul><li><a href=\"http://info.cern.ch\">info.cern.ch</a>, the first web site</li><li><a href=\"http://example.com\">example.com</a></li><li><a href=\"http://neverssl.com\">neverssl.com</a></li><li><a href=\"http://textfiles.com\">textfiles.com</a></li></ul>\
<h3>Downloads</h3><p>Links to pictures (png, gif, bmp) and sounds (wav) are saved in Downloads and open in Media.</p>";

impl Browser {
    pub fn new(arg: Option<String>) -> Browser {
        let mut b = Browser {
            address: String::new(),
            page: None,
            page_url: String::from("about:home"),
            placed: None,
            history: Vec::new(),
            forward: Vec::new(),
            load: Arc::new(Mutex::new(Load::Idle)),
            cancel: Arc::new(AtomicBool::new(false)),
            status: String::new(),
            hover: None,
            progress_at: 0,
        };
        b.show("about:home", HOME);
        if let Some(a) = arg {
            b.navigate(&a, true);
        }
        b
    }

    fn show(&mut self, url: &str, html: &str) {
        self.page = Some(parse_html(html));
        self.page_url = String::from(url);
        self.address = if url == "about:home" { String::new() } else { String::from(url) };
        self.placed = None;
        self.hover = None;
    }

    fn navigate(&mut self, target: &str, push: bool) {
        let t = target.trim();
        if t.is_empty() {
            return;
        }
        let url = if t.contains("://") || t.starts_with("about:") { String::from(t) } else { alloc::format!("http://{}", t) };
        if push {
            self.history.push(self.page_url.clone());
            self.forward.clear();
        }
        if url == "about:home" {
            self.show("about:home", HOME);
            return;
        }
        self.address = url.clone();
        self.cancel.store(true, Ordering::SeqCst);
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel = cancel.clone();
        let slot = self.load.clone();
        *slot.lock() = Load::Loading(String::from("starting"), 0);
        self.progress_at = crate::time::uptime_ms();
        crate::net::start();
        crate::sched::spawn("fetch", 256, move || {
            let s2 = slot.clone();
            let c2 = cancel.clone();
            let r = http::get(&url, &mut |m, n| {
                if c2.load(Ordering::Relaxed) {
                    return false;
                }
                *s2.lock() = Load::Loading(String::from(m), n);
                true
            });
            if !cancel.load(Ordering::Relaxed) {
                *slot.lock() = Load::Done(r);
            }
        });
    }

    fn finish(&mut self, r: Result<http::Response, String>) {
        match r {
            Ok(resp) => {
                let url = resp.url.to_string();
                let lower = url.to_ascii_lowercase();
                let media = ["png", "gif", "bmp", "wav", "ppm"].iter().any(|e| lower.ends_with(&alloc::format!(".{}", e))) || resp.content_type.starts_with("image/") || resp.content_type.starts_with("audio/");
                if media {
                    let name = super::files::unique("/Downloads", lower.rsplit('/').next().filter(|n| !n.is_empty()).unwrap_or("download"));
                    let path = super::files::join("/Downloads", &name);
                    let _ = FILESYSTEM.lock().as_mut().map(|fs| fs.write_file(&path, resp.body.clone()));
                    crate::pfs::sync_if_dirty();
                    super::notify("Downloaded", &name);
                    super::open_file(&path);
                    self.status = alloc::format!("Saved {} to Downloads", name);
                    return;
                }
                let text = String::from_utf8_lossy(&resp.body).into_owned();
                let html = if resp.content_type.contains("html") || text.trim_start().starts_with('<') {
                    text
                } else {
                    alloc::format!("<pre>{}</pre>", text.replace('&', "&amp;").replace('<', "&lt;"))
                };
                self.show(&url, &html);
                self.status = if resp.status == 200 { String::new() } else { alloc::format!("HTTP {}", resp.status) };
            }
            Err(e) => {
                let a = self.address.clone();
                let html = alloc::format!("<h2>This page can't be opened</h2><p>{}</p><p>{}</p>", e.replace('<', "&lt;"), a.replace('<', "&lt;"));
                self.show(&a, &html);
            }
        }
    }

    fn back(&mut self) {
        if let Some(p) = self.history.pop() {
            self.forward.push(self.page_url.clone());
            self.navigate(&p, false);
        }
    }

    fn fwd(&mut self) {
        if let Some(p) = self.forward.pop() {
            self.history.push(self.page_url.clone());
            self.navigate(&p, false);
        }
    }

    fn resolve(&self, href: &str) -> String {
        match http::Url::parse(&self.page_url) {
            Ok(b) => b.join(href),
            Err(_) => String::from(href),
        }
    }
}

impl App for Browser {
    fn title(&self) -> String {
        match &self.page {
            Some(p) if !p.title.is_empty() => p.title.clone(),
            _ => String::from("Browser"),
        }
    }

    fn menus(&self) -> Vec<(&'static str, Vec<MenuItem>)> {
        let m = MenuItem::new;
        alloc::vec![("History", alloc::vec![m("Back"), m("Forward"), m("Reload"), MenuItem::sep(), m("Home")]), ("File", alloc::vec![m("Save Page to Downloads")])]
    }

    fn on_menu(&mut self, menu: usize, item: usize) {
        match (menu, item) {
            (0, 0) => self.back(),
            (0, 1) => self.fwd(),
            (0, 2) => {
                let u = self.page_url.clone();
                self.navigate(&u, false);
            }
            (0, 4) => self.navigate("about:home", true),
            (1, 0) => {
                if let Some(p) = &self.page {
                    let mut text = String::new();
                    for t in &p.toks {
                        match t {
                            Tok::Word { text: w, space_before, .. } => {
                                if *space_before {
                                    text.push(' ');
                                }
                                text.push_str(w);
                            }
                            Tok::Break | Tok::Gap => text.push('\n'),
                            _ => {}
                        }
                    }
                    let name = super::files::unique("/Downloads", "page.txt");
                    let _ = FILESYSTEM.lock().as_mut().map(|fs| fs.write_file(&super::files::join("/Downloads", &name), text.into_bytes()));
                    crate::pfs::sync_if_dirty();
                    super::notify("Saved", &name);
                }
            }
            _ => {}
        }
    }

    fn tick(&mut self) -> bool {
        matches!(*self.load.lock(), Load::Loading(..) | Load::Done(_))
    }

    fn frame(&mut self, ui: &mut Ui) {
        // results from the fetching thread
        let done = {
            let mut l = self.load.lock();
            match &*l {
                Load::Loading(m, n) => {
                    self.status = if *n > 0 { alloc::format!("{} ({} KB)", m, n / 1024) } else { alloc::format!("{}...", m) };
                    None
                }
                Load::Done(_) => match core::mem::replace(&mut *l, Load::Idle) {
                    Load::Done(r) => Some(r),
                    _ => None,
                },
                Load::Idle => None,
            }
        };
        if let Some(r) = done {
            self.status.clear();
            self.finish(r);
        }
        let loading = matches!(*self.load.lock(), Load::Loading(..));
        let th = ui.th;
        let (w, h) = (ui.w, ui.h);
        let paper = if th.dark { 0x1E1F23 } else { 0xFFFFFF };
        ui.fill(paper);
        let bar = 48;
        // toolbar
        ui.cv.rect(0, 0, w, bar, th.window);
        ui.cv.hline(0, bar - 1, w, th.separator, 255);
        if ui.button((10, 9, 30, 30), "<") {
            self.back();
        }
        if ui.button((44, 9, 30, 30), ">") {
            self.fwd();
        }
        if ui.button((78, 9, 30, 30), if loading { "x" } else { "r" }) {
            if loading {
                self.cancel.store(true, Ordering::SeqCst);
                *self.load.lock() = Load::Idle;
                self.status.clear();
            } else {
                let u = self.page_url.clone();
                self.navigate(&u, false);
            }
        }
        let fr = (118, 9, w - 128, 30);
        if ui.field(id("addr"), fr, &mut self.address, "Search or enter an http:// address") {
            let a = self.address.clone();
            ui.st.focus = 0;
            self.navigate(&a, true);
        }
        if loading {
            let t = ((ui.now - self.progress_at) % 1200) as i32;
            let pw = fr.2 / 4;
            let px = fr.0 + (fr.2 + pw) * t / 1200 - pw;
            ui.cv.push(fr, false);
            ui.cv.round(px, fr.1 + fr.3 - 3, pw, 3, 1, th.accent);
            ui.cv.pop();
            ui.animate(100);
        }
        for (k, m) in ui.keys().to_vec() {
            if ui.st.focus == id("addr") {
                continue;
            }
            match (k, m) {
                (Key::Char('l'), _) | (Key::Char('/'), _) => ui.st.focus = id("addr"),
                (Key::Char('\u{8}'), _) => self.back(),
                (Key::Char('r'), _) => {
                    let u = self.page_url.clone();
                    self.navigate(&u, false);
                }
                _ => {}
            }
        }
        // the page
        let view = (0, bar, w, h - bar - 24);
        let Some(page) = &self.page else { return };
        if self.placed.as_ref().map(|p| p.3 != w).unwrap_or(true) {
            let (a, b, c) = layout(page, w);
            self.placed = Some((a, b, c, w));
        }
        let (words, rules, total, _) = self.placed.as_ref().unwrap();
        let total = *total;
        let sid = id("page");
        let mut off = ui.scroll_of(sid);
        let step = view.3 - 60;
        for (k, _) in ui.keys().to_vec() {
            if ui.st.focus == id("addr") {
                continue;
            }
            match k {
                Key::Down => off += 40,
                Key::Up => off -= 40,
                Key::PageDown | Key::Char(' ') => off += step,
                Key::PageUp => off -= step,
                Key::Home => off = 0,
                Key::End => off = total,
                _ => {}
            }
        }
        ui.scroll_to(sid, off);
        let off = ui.scroll_begin(sid, view, total);
        let mx = 20;
        let mouse = ui.mouse();
        let mut hover = None;
        for r in rules.iter() {
            let y = bar + 14 + r - off;
            ui.cv.hline(mx, y, w - 2 * mx, th.separator, 255);
        }
        for p in words.iter() {
            let f = font_for(p.style);
            let y = bar + 14 + p.y - off;
            if y + 40 < bar || y > h {
                continue;
            }
            let x = mx + 20 + p.x;
            let over = p.link.is_some() && ui.inp.inside && mouse.0 >= x && mouse.0 < x + p.w && mouse.1 >= y && mouse.1 < y + f.height() as i32 + 2;
            if over {
                hover = p.link;
            }
            let c = if p.link.is_some() {
                if p.link == self.hover { gfx::lighten(th.accent, 60) } else { th.accent }
            } else if p.style.heading > 0 {
                th.text
            } else {
                gfx::mix(th.text, paper, 20)
            };
            ui.cv.text(x, y, &p.text, &f, c);
            if p.link.is_some() && p.link == self.hover {
                ui.cv.rect(x, y + f.height() as i32, p.w, 1, c);
            }
        }
        ui.scroll_end(sid, view, total);
        if hover != self.hover {
            self.hover = hover;
            ui.redraw_soon = true;
        }
        if hover.is_some() {
            ui.cursor = super::Cursor::Hand;
        }
        if let (Some(l), true) = (hover, ui.inp.pressed) {
            let href = page.links[l].clone();
            let target = self.resolve(&href);
            self.navigate(&target, true);
        }
        // status line
        ui.cv.rect(0, h - 24, w, 24, th.window);
        ui.cv.hline(0, h - 24, w, th.separator, 255);
        let status = match self.hover {
            Some(l) => self.resolve(&self.page.as_ref().map(|p| p.links[l].clone()).unwrap_or_default()),
            None => self.status.clone(),
        };
        let f = ui.font;
        ui.cv.text(10, h - 21, &gfx::ellipsize(&f, &status, w - 20), &f, th.text2);
    }
}
