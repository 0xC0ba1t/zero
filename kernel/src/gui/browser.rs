//! A very basic web browser: plain HTTP, headings, paragraphs, lists, links
//! and preformatted text. No images, scripts or styles.

use super::draw::{self, accent, BG, BG_DARK, BG_HOVER, BG_PANEL, BORDER, TEXT, TEXT_FAINT, TEXT_MUTED};
use super::widgets::{button, inside, AppUi};
use super::Event;
use crate::graphics::display::Display;
use crate::graphics::font::Font;
use crate::graphics::framebuffer::Framebuffer;
use crate::input::Key;
use crate::net::http;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;

// ---------------------------------------------------------------- HTML

#[derive(Clone, Copy, PartialEq, Eq, Default)]
struct Style {
    bold: bool,
    /// 0 for body text, 1 to 6 for headings.
    heading: u8,
    pre: bool,
}

#[derive(Clone)]
enum Tok {
    Word { text: String, space_before: bool, style: Style, link: Option<usize> },
    /// End the current line.
    Break,
    /// A blank line (once).
    Gap,
    Rule,
    /// Start of a list item (the bullet or number).
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
                "copy" => Some('c'),
                "mdash" | "ndash" => Some('-'),
                "hellip" => Some('.'),
                "laquo" => Some('<'),
                "raquo" => Some('>'),
                _ => {
                    if let Some(num) = name.strip_prefix('#') {
                        let v = if let Some(h) = num.strip_prefix('x').or_else(|| num.strip_prefix('X')) {
                            u32::from_str_radix(h, 16).ok()
                        } else {
                            num.parse().ok()
                        };
                        v.and_then(char::from_u32)
                    } else {
                        None
                    }
                }
            };
            if let Some(c) = rep {
                out.push(c);
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
        let after = &tag[at + name.len()..];
        let after_trim = after.trim_start();
        if before_ok && after_trim.starts_with('=') {
            let v = after_trim[1..].trim_start();
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
    let mut toks: Vec<Tok> = Vec::new();
    let mut links: Vec<String> = Vec::new();
    let mut title = String::new();
    let mut style = Style::default();
    let mut link: Option<usize> = None;
    let mut in_title = false;
    let mut skip_until: Option<&'static str> = None;
    let mut ordered: Vec<Option<u32>> = Vec::new();
    let mut pending_space = false;
    let bytes = html;
    let mut i = 0;
    let lower_all = html.to_ascii_lowercase();

    while i < bytes.len() {
        let rest = &bytes[i..];
        if let Some(end_tag) = skip_until {
            match lower_all[i..].find(end_tag) {
                Some(j) => {
                    i += j + end_tag.len();
                    skip_until = None;
                    // skip the rest of the closing tag
                    if let Some(k) = bytes[i..].find('>') {
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
                    if closing {
                        toks.push(Tok::Gap);
                    }
                }
                "p" | "blockquote" | "table" | "form" | "section" | "article" => {
                    toks.push(Tok::Gap);
                }
                "div" | "tr" | "header" | "footer" | "nav" | "main" | "aside" | "center" | "address" | "dl" | "dt" | "dd" | "fieldset" | "figure" | "caption" => {
                    toks.push(Tok::Break);
                }
                "br" => toks.push(Tok::Break),
                "hr" => {
                    toks.push(Tok::Break);
                    toks.push(Tok::Rule);
                }
                "ul" | "ol" => {
                    if closing {
                        ordered.pop();
                        toks.push(Tok::Indent(-1));
                        toks.push(Tok::Break);
                    } else {
                        ordered.push(if name == "ol" { Some(0) } else { None });
                        toks.push(Tok::Break);
                        toks.push(Tok::Indent(1));
                    }
                }
                "li" if !closing => {
                    toks.push(Tok::Break);
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
                "b" | "strong" | "em" | "i" | "th" => {
                    if name == "th" {
                        pending_space = true;
                    }
                    style.bold = !closing || style.heading > 0;
                }
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
                    toks.push(Tok::Word { text: alloc::format!("[{}]", alt), space_before: true, style, link });
                }
                _ => {}
            }
            continue;
        }
        // text up to the next tag
        let end = rest.find('<').unwrap_or(rest.len());
        let raw = &rest[..end];
        i += end;
        let text = decode_entities(raw);
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
        // trailing whitespace is remembered for the next word
        if text.ends_with(char::is_whitespace) && !text.trim().is_empty() {
            pending_space = true;
        }
    }
    Page { title, toks, links }
}

// ---------------------------------------------------------------- layout

struct Placed {
    x: i32,
    y: i32,
    text: String,
    style: Style,
    link: Option<usize>,
    width: i32,
}

struct Layout {
    words: Vec<Placed>,
    rules: Vec<i32>,
    height: i32,
}

fn font_for(style: Style) -> Font {
    let px = match style.heading {
        1 => 32,
        2 => 24,
        3 => 20,
        _ => 16,
    };
    draw::font(px, style.bold || style.heading > 0)
}

fn layout_page(page: &Page, width: i32) -> Layout {
    let mut words = Vec::new();
    let mut rules = Vec::new();
    let (mut x, mut y) = (0i32, 0i32);
    let mut line_h = 20;
    let mut indent = 0i32;
    let mut at_line_start = true;
    let mut blank_before = true;
    let margin = 12;
    let max_x = (width - margin * 2).max(60);

    fn end_line(x: &mut i32, y: &mut i32, line_h: &mut i32, at_start: &mut bool, blank: &mut bool) {
        if !*at_start {
            *y += *line_h + 2;
            *x = 0;
            *line_h = 20;
            *at_start = true;
            *blank = false;
        }
    }

    for tok in &page.toks {
        match tok {
            Tok::Break => end_line(&mut x, &mut y, &mut line_h, &mut at_line_start, &mut blank_before),
            Tok::Gap => {
                end_line(&mut x, &mut y, &mut line_h, &mut at_line_start, &mut blank_before);
                if !blank_before && y > 0 {
                    y += 10;
                    blank_before = true;
                }
            }
            Tok::Rule => {
                end_line(&mut x, &mut y, &mut line_h, &mut at_line_start, &mut blank_before);
                rules.push(y + 6);
                y += 14;
            }
            Tok::Indent(d) => indent = (indent + d * 28).max(0),
            Tok::Bullet(b) => {
                end_line(&mut x, &mut y, &mut line_h, &mut at_line_start, &mut blank_before);
                let st = Style::default();
                let f = font_for(st);
                words.push(Placed { x: indent - 4 - f.text_width(b) as i32, y, text: b.clone(), style: st, link: None, width: f.text_width(b) as i32 });
                at_line_start = false;
                x = 0;
                blank_before = false;
            }
            Tok::Word { text, space_before, style, link } => {
                let f = font_for(*style);
                let w = f.text_width(text) as i32;
                let space = if at_line_start || x == 0 { 0 } else if *space_before { f.width() as i32 } else { 0 };
                if !style.pre && !at_line_start && indent + x + space + w > max_x {
                    end_line(&mut x, &mut y, &mut line_h, &mut at_line_start, &mut blank_before);
                }
                let space = if at_line_start { 0 } else { space };
                let px = indent + x + space;
                words.push(Placed { x: px, y, text: text.clone(), style: *style, link: *link, width: w });
                x += space + w;
                line_h = line_h.max(f.height() as i32 + 4);
                at_line_start = false;
                blank_before = false;
            }
        }
    }
    end_line(&mut x, &mut y, &mut line_h, &mut at_line_start, &mut blank_before);
    Layout { words, rules, height: y + 20 }
}

// ---------------------------------------------------------------- the app

enum Load {
    Idle,
    Loading { msg: String, bytes: usize },
    Done(Result<http::Response, String>),
}

pub struct Browser {
    address: String,
    editing: bool,
    page: Option<Page>,
    page_url: String,
    layout: Option<Layout>,
    layout_width: i32,
    scroll: i32,
    history: Vec<String>,
    forward: Vec<String>,
    load: Arc<Mutex<Load>>,
    cancel: Arc<AtomicBool>,
    status: String,
    mouse: (i32, i32),
    hover_link: Option<usize>,
    dragging_bar: bool,
}

const TOOLBAR_H: i32 = 44;
const STATUS_H: i32 = 24;

const HOME: &str = "<h1>zero browser</h1>\
<p>A very small web browser. Type an <b>http://</b> address above and press Enter. \
Pages that need encryption (https) do not work yet.</p>\
<h3>Try these</h3>\
<ul>\
<li><a href=\"http://example.com\">example.com</a></li>\
<li><a href=\"http://info.cern.ch\">info.cern.ch</a>, the first web site ever</li>\
<li><a href=\"http://neverssl.com\">neverssl.com</a></li>\
<li><a href=\"http://textfiles.com\">textfiles.com</a></li>\
</ul>\
<h3>Keys</h3>\
<pre>Enter     go to the address\n\
Up/Down   scroll    PageUp/PageDown   scroll a page\n\
Backspace go back   Esc   stop loading\n\
R         reload</pre>";

impl Browser {
    pub fn new() -> Browser {
        let mut b = Browser {
            address: String::new(),
            editing: false,
            page: None,
            page_url: String::from("about:home"),
            layout: None,
            layout_width: 0,
            scroll: 0,
            history: Vec::new(),
            forward: Vec::new(),
            load: Arc::new(Mutex::new(Load::Idle)),
            cancel: Arc::new(AtomicBool::new(false)),
            status: String::new(),
            mouse: (0, 0),
            hover_link: None,
            dragging_bar: false,
        };
        b.show_html("about:home", HOME);
        b
    }

    fn show_html(&mut self, url: &str, html: &str) {
        self.page = Some(parse_html(html));
        self.page_url = String::from(url);
        self.address = if url == "about:home" { String::new() } else { String::from(url) };
        self.layout = None;
        self.scroll = 0;
        self.hover_link = None;
    }

    fn navigate(&mut self, target: &str, push_history: bool) {
        let target = target.trim();
        if target.is_empty() {
            return;
        }
        let url = if target.contains("://") || target.starts_with("about:") {
            String::from(target)
        } else {
            alloc::format!("http://{}", target)
        };
        if url == "about:home" {
            if push_history {
                self.history.push(self.page_url.clone());
                self.forward.clear();
            }
            self.show_html("about:home", HOME);
            return;
        }
        if push_history {
            self.history.push(self.page_url.clone());
            self.forward.clear();
        }
        self.address = url.clone();
        self.start_fetch(url);
    }

    fn start_fetch(&mut self, url: String) {
        self.cancel.store(true, Ordering::SeqCst);
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel = cancel.clone();
        let slot = self.load.clone();
        *slot.lock() = Load::Loading { msg: String::from("starting"), bytes: 0 };
        crate::net::start();
        crate::sched::spawn("fetch", 256, move || {
            let slot2 = slot.clone();
            let c2 = cancel.clone();
            let result = http::get(&url, &mut |msg, bytes| {
                if c2.load(Ordering::Relaxed) {
                    return false;
                }
                *slot2.lock() = Load::Loading { msg: String::from(msg), bytes };
                true
            });
            if !cancel.load(Ordering::Relaxed) {
                *slot.lock() = Load::Done(result);
            }
        });
    }

    fn go_back(&mut self) {
        if let Some(prev) = self.history.pop() {
            self.forward.push(self.page_url.clone());
            self.navigate(&prev, false);
        }
    }

    fn go_forward(&mut self) {
        if let Some(next) = self.forward.pop() {
            self.history.push(self.page_url.clone());
            self.navigate(&next, false);
        }
    }

    fn viewport(&self, w: i32, h: i32) -> (i32, i32) {
        (w, h - TOOLBAR_H - STATUS_H)
    }

    fn max_scroll(&self, h: i32) -> i32 {
        let (_, vh) = self.viewport(0, h);
        self.layout.as_ref().map(|l| (l.height - vh).max(0)).unwrap_or(0)
    }

    fn link_at(&self, x: i32, y: i32) -> Option<usize> {
        let layout = self.layout.as_ref()?;
        let top = TOOLBAR_H;
        for p in &layout.words {
            let f = font_for(p.style);
            let (wx, wy) = (12 + p.x, top + p.y - self.scroll);
            if p.link.is_some() && x >= wx && x < wx + p.width && y >= wy && y < wy + f.height() as i32 + 2 {
                return p.link;
            }
        }
        None
    }

    fn toolbar(&self, w: i32) -> [(i32, i32, i32, i32); 4] {
        [(8, 8, 28, 28), (40, 8, 28, 28), (72, 8, 28, 28), (108, 6, w - 118, 32)]
    }
}

impl AppUi for Browser {
    fn event(&mut self, ev: Event, w: i32, h: i32) -> bool {
        let bar = self.toolbar(w);
        match ev {
            Event::Key(key) => {
                if self.editing {
                    match key {
                        Key::Char('\n') => {
                            self.editing = false;
                            let target = self.address.clone();
                            self.navigate(&target, true);
                        }
                        Key::Char('\u{1b}') => self.editing = false,
                        Key::Char('\u{8}') => {
                            self.address.pop();
                        }
                        Key::Char(c) if !c.is_control() => self.address.push(c),
                        _ => {}
                    }
                    return true;
                }
                let (_, vh) = self.viewport(w, h);
                let max = self.max_scroll(h);
                match key {
                    Key::Down => self.scroll = (self.scroll + 40).min(max),
                    Key::Up => self.scroll = (self.scroll - 40).max(0),
                    Key::PageDown | Key::Char(' ') => self.scroll = (self.scroll + vh - 40).min(max),
                    Key::PageUp => self.scroll = (self.scroll - vh + 40).max(0),
                    Key::Home => self.scroll = 0,
                    Key::End => self.scroll = max,
                    Key::Char('\u{8}') => self.go_back(),
                    Key::Char('r') | Key::Char('R') => {
                        let u = self.page_url.clone();
                        self.navigate(&u, false);
                    }
                    Key::Char('l') | Key::Char('/') => self.editing = true,
                    Key::Char('\u{1b}') => {
                        self.cancel.store(true, Ordering::SeqCst);
                        *self.load.lock() = Load::Idle;
                        self.status.clear();
                    }
                    _ => {}
                }
                true
            }
            Event::MouseMove { x, y } => {
                self.mouse = (x, y);
                if self.dragging_bar {
                    let (_, vh) = self.viewport(w, h);
                    let total = self.layout.as_ref().map(|l| l.height).unwrap_or(1).max(1);
                    let track = vh.max(1);
                    let rel = (y - TOOLBAR_H).clamp(0, track);
                    self.scroll = (rel * total / track - vh / 2).clamp(0, self.max_scroll(h));
                    return true;
                }
                let hover = if y > TOOLBAR_H { self.link_at(x, y) } else { None };
                let changed = hover != self.hover_link;
                self.hover_link = hover;
                changed || inside(x, y, (0, 0, w, TOOLBAR_H))
            }
            Event::MouseUp { .. } => {
                self.dragging_bar = false;
                false
            }
            Event::MouseDown { x, y, button: 0 } => {
                if inside(x, y, bar[0]) {
                    self.go_back();
                } else if inside(x, y, bar[1]) {
                    self.go_forward();
                } else if inside(x, y, bar[2]) {
                    let loading = matches!(*self.load.lock(), Load::Loading { .. });
                    if loading {
                        self.cancel.store(true, Ordering::SeqCst);
                        *self.load.lock() = Load::Idle;
                    } else {
                        let u = self.page_url.clone();
                        self.navigate(&u, false);
                    }
                } else if inside(x, y, bar[3]) {
                    self.editing = true;
                } else if y > TOOLBAR_H && y < h - STATUS_H {
                    self.editing = false;
                    if x >= w - 14 {
                        self.dragging_bar = true;
                        return self.event(Event::MouseMove { x, y }, w, h);
                    }
                    if let Some(l) = self.link_at(x, y) {
                        let href = self.page.as_ref().map(|p| p.links[l].clone()).unwrap_or_default();
                        let base = http::Url::parse(&self.page_url).ok();
                        let target = match base {
                            Some(b) => b.join(&href),
                            None => href,
                        };
                        self.navigate(&target, true);
                    }
                }
                true
            }
            _ => false,
        }
    }

    fn tick(&mut self) -> bool {
        let mut load = self.load.lock();
        match &*load {
            Load::Loading { msg, bytes } => {
                let s = if *bytes > 0 { alloc::format!("{} ({} KB)", msg, bytes / 1024) } else { alloc::format!("{}...", msg) };
                let changed = s != self.status;
                self.status = s;
                changed
            }
            Load::Done(_) => {
                let done = core::mem::replace(&mut *load, Load::Idle);
                drop(load);
                if let Load::Done(result) = done {
                    match result {
                        Ok(resp) => {
                            let text = String::from_utf8_lossy(&resp.body).into_owned();
                            let url = resp.url.to_string();
                            let html = if resp.content_type.contains("html") || text.trim_start().starts_with('<') {
                                text
                            } else {
                                // plain text: keep the line breaks
                                alloc::format!("<pre>{}</pre>", text.replace('&', "&amp;").replace('<', "&lt;"))
                            };
                            self.show_html(&url, &html);
                            self.status = if resp.status == 200 { String::new() } else { alloc::format!("status {}", resp.status) };
                        }
                        Err(e) => {
                            let html = alloc::format!("<h2>Could not load the page</h2><p>{}</p><p>Address: {}</p>", e.replace('<', "&lt;"), self.address.replace('<', "&lt;"));
                            let a = self.address.clone();
                            self.show_html(&a, &html);
                            self.status = String::from("failed");
                        }
                    }
                }
                true
            }
            Load::Idle => {
                if !self.status.is_empty() && self.page.is_some() && self.status.ends_with("...") {
                    self.status.clear();
                    return true;
                }
                false
            }
        }
    }

    fn draw(&mut self, fb: &mut Framebuffer) {
        let (w, h) = (fb.width() as i32, fb.height() as i32);
        let (vw, vh) = self.viewport(w, h);
        // lay the page out for this width
        if self.layout.is_none() || self.layout_width != vw {
            if let Some(page) = &self.page {
                self.layout = Some(layout_page(page, vw - 14));
                self.layout_width = vw;
            }
        }
        self.scroll = self.scroll.clamp(0, self.max_scroll(h));

        draw::rect(fb, 0, 0, w, h, BG);
        // page
        fb.set_clip_rect((0, TOOLBAR_H, w, h - STATUS_H));
        if let Some(layout) = &self.layout {
            let f16 = draw::font(16, false);
            for r in &layout.rules {
                let y = TOOLBAR_H + r - self.scroll;
                draw::rect(fb, 12, y, vw - 40, 1, BORDER);
            }
            for p in &layout.words {
                let f = font_for(p.style);
                let y = TOOLBAR_H + p.y - self.scroll;
                if y + f.height() as i32 + 2 < TOOLBAR_H || y > h - STATUS_H {
                    continue;
                }
                let hovered = p.link.is_some() && p.link == self.hover_link;
                let color = if p.link.is_some() {
                    if hovered { 0xFFFFFF } else { accent() }
                } else if p.style.heading > 0 {
                    0xFFFFFF
                } else if p.style.bold {
                    0xEEF1F5
                } else {
                    TEXT
                };
                draw::text(fb, &f, 12 + p.x, y, &p.text, color);
                if p.link.is_some() {
                    draw::rect(fb, 12 + p.x, y + f.height() as i32, p.width, 1, color);
                }
            }
            let _ = f16;
            // scrollbar
            if layout.height > vh {
                let track = vh;
                let thumb = (vh * vh / layout.height).clamp(24, track);
                let ty = TOOLBAR_H + self.scroll * (track - thumb) / (layout.height - vh).max(1);
                draw::round(fb, w - 9, ty, 6, thumb, 3, 0x4B5160);
            }
        }
        fb.clear_clip();

        // toolbar
        draw::rect(fb, 0, 0, w, TOOLBAR_H, BG_DARK);
        draw::rect(fb, 0, TOOLBAR_H - 1, w, 1, BORDER);
        let f = draw::font(16, false);
        let bar = self.toolbar(w);
        let hov = |r: (i32, i32, i32, i32)| inside(self.mouse.0, self.mouse.1, r);
        button(fb, &f, bar[0], "<", hov(bar[0]), false);
        button(fb, &f, bar[1], ">", hov(bar[1]), false);
        let loading = matches!(*self.load.lock(), Load::Loading { .. });
        button(fb, &f, bar[2], if loading { "x" } else { "r" }, hov(bar[2]), false);
        let field = bar[3];
        draw::round(fb, field.0, field.1, field.2, field.3, 8, if self.editing { BG_HOVER } else { BG_PANEL });
        if self.editing {
            draw::round(fb, field.0 - 1, field.1 - 1, field.2 + 2, field.3 + 2, 9, accent());
            draw::round(fb, field.0, field.1, field.2, field.3, 8, BG_HOVER);
        }
        let shown = if self.address.is_empty() && !self.editing { String::from("Search or type an http:// address") } else { self.address.clone() };
        let color = if self.address.is_empty() && !self.editing { TEXT_FAINT } else { TEXT };
        let max_chars = ((field.2 - 20) / f.width() as i32).max(1) as usize;
        let tail: String = {
            let chars: Vec<char> = shown.chars().collect();
            if self.editing && chars.len() > max_chars - 1 {
                chars[chars.len() - (max_chars - 1)..].iter().collect()
            } else {
                chars.iter().take(max_chars).collect()
            }
        };
        let text_x = field.0 + 10;
        let end_x = draw::text(fb, &f, text_x, field.1 + (field.3 - f.height() as i32) / 2, &tail, color);
        if self.editing {
            draw::rect(fb, end_x + 1, field.1 + 6, 2, field.3 - 12, accent());
        }

        // status line
        draw::rect(fb, 0, h - STATUS_H, w, STATUS_H, BG_DARK);
        let status = if let Some(l) = self.hover_link {
            let href = self.page.as_ref().map(|p| p.links[l].clone()).unwrap_or_default();
            http::Url::parse(&self.page_url).map(|b| b.join(&href)).unwrap_or(href)
        } else if !self.status.is_empty() {
            self.status.clone()
        } else if let Some(p) = &self.page {
            p.title.clone()
        } else {
            String::new()
        };
        let sf = draw::font(15, false);
        draw::text(fb, &sf, 10, h - STATUS_H + 3, &draw::fit_text(&sf, &status, w - 20), TEXT_MUTED);
    }
}
