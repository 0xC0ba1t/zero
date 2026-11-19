//! ZeroScript programs as desktop apps: the host that gives a script a
//! window, drawing, the mouse and keyboard, sound, Bit and saved data.
//!
//! Drawing goes to a back buffer; `frame()` shows it (so nothing flickers)
//! and waits for the next frame, about 60 a second.

use super::gfx::{self, Canvas};
use crate::graphics::display::Display;
use super::theme;
use super::Event;
use crate::input::Key;
use crate::zs::{self, Value};
use alloc::collections::BTreeMap;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::Ordering;

pub const BUILTIN: [(&str, &str); 7] = [
    ("snake", include_str!("../../../programs/gui/snake.zs")),
    ("pong", include_str!("../../../programs/gui/pong.zs")),
    ("invaders", include_str!("../../../programs/gui/invaders.zs")),
    ("tictactoe", include_str!("../../../programs/gui/tictactoe.zs")),
    ("connect4", include_str!("../../../programs/gui/connect4.zs")),
    ("calculator", include_str!("../../../programs/gui/calculator.zs")),
    ("bounce", include_str!("../../../programs/gui/bounce.zs")),
];

pub fn run_builtin(name: &str) {
    match BUILTIN.iter().find(|(n, _)| *n == name) {
        Some((n, src)) => run_source(n, src),
        None => show_error(name, "no such program"),
    }
}

pub fn run_file(path: &str) {
    let data = crate::fs::FILESYSTEM.lock().as_ref().and_then(|fs| fs.read_file(path).ok());
    match data {
        Some(d) => {
            let src = String::from_utf8_lossy(&d).into_owned();
            run_source(super::files::base(path), &src);
        }
        None => show_error(path, "the file could not be read"),
    }
}

struct GuiHost {
    w: usize,
    h: usize,
    buf: Vec<u32>,
    mouse: (i32, i32),
    down: bool,
    clicked: bool,
    rclicked: bool,
    wheel: i32,
    keys: Vec<(Key, u8)>,
    held_keys: Vec<i64>,
    last_frame: u64,
    images: BTreeMap<String, Option<(Vec<u32>, usize, usize)>>,
    name: String,
    printed: Vec<String>,
    active_button: Option<(i32, i32)>,
}

fn key_code(k: Key) -> i64 {
    match k {
        Key::Char(c) => c as i64,
        Key::Up => 256,
        Key::Down => 257,
        Key::Left => 258,
        Key::Right => 259,
        Key::PageUp => 260,
        Key::PageDown => 261,
        Key::Home => 262,
        Key::End => 263,
        Key::Delete => 264,
        Key::F(n) => 300 + n as i64,
    }
}

impl GuiHost {
    fn canvas(&mut self) -> Canvas {
        Canvas::new(&mut self.buf, self.w, self.h)
    }

    /// Shows the back buffer and collects input until the next frame.
    fn frame(&mut self) -> bool {
        let Some(ctx) = crate::sched::context() else { return false };
        {
            let mut s = ctx.surface.lock();
            let (sw, sh) = (s.width(), s.height());
            let dst = s.pixels32();
            for y in 0..sh.min(self.h) {
                let n = sw.min(self.w);
                dst[y * sw..y * sw + n].copy_from_slice(&self.buf[y * self.w..y * self.w + n]);
            }
            s.damage_rows(0, sh);
        }
        self.clicked = false;
        self.rclicked = false;
        self.wheel = 0;
        self.keys.clear();
        // wait for the next frame, letting everything else run
        let due = self.last_frame + 16;
        loop {
            if ctx.is_closed() {
                return false;
            }
            loop {
                let ev = ctx.events.lock().pop_front();
                let Some(ev) = ev else { break };
                match ev {
                    Event::MouseMove { x, y } => self.mouse = (x, y),
                    Event::MouseDown { x, y, button, .. } => {
                        self.mouse = (x, y);
                        if button == 0 {
                            self.down = true;
                            self.clicked = true;
                        } else if button == 1 {
                            self.rclicked = true;
                        }
                    }
                    Event::MouseUp { x, y, button } => {
                        self.mouse = (x, y);
                        if button == 0 {
                            self.down = false;
                        }
                    }
                    Event::Wheel { dy } => self.wheel += dy,
                    Event::Key(k, m) => self.keys.push((k, m)),
                    Event::Resize { w, h } => {
                        self.w = w.max(1);
                        self.h = h.max(1);
                        self.buf = alloc::vec![theme::current().window; self.w * self.h];
                    }
                    Event::Focus(false) => self.down = false,
                    _ => {}
                }
            }
            if crate::time::uptime_ms() >= due {
                break;
            }
            crate::sched::yield_idle();
        }
        self.last_frame = crate::time::uptime_ms();
        true
    }

    fn color(v: &Value) -> u32 {
        match v {
            Value::Str(s) => named_color(s),
            other => (other.int() as u32) & 0xFFFFFF,
        }
    }
}

fn named_color(s: &str) -> u32 {
    let th = theme::current();
    match s {
        "accent" => th.accent,
        "bg" | "window" => th.window,
        "fg" | "text" => th.text,
        "muted" => th.text2,
        "faint" => th.text3,
        "panel" | "control" => th.control,
        "sidebar" => th.sidebar,
        "line" | "separator" => th.separator,
        "white" => 0xFFFFFF,
        "black" => 0x000000,
        "red" => 0xFF453A,
        "green" => 0x30D158,
        "blue" => 0x0A84FF,
        "yellow" => 0xFFD60A,
        "orange" => 0xFF9F0A,
        "purple" => 0xBF5AF2,
        "pink" => 0xFF375F,
        "gray" | "grey" => 0x8E8E93,
        other => u32::from_str_radix(other.trim_start_matches('#'), 16).unwrap_or(0xFFFFFF),
    }
}

impl zs::Host for GuiHost {
    fn print(&mut self, text: &str) {
        self.printed.push(String::from(text));
        if self.printed.len() > 200 {
            self.printed.remove(0);
        }
    }

    fn should_stop(&mut self) -> bool {
        crate::sched::window_closed()
    }

    fn call(&mut self, name: &str, args: &[Value]) -> Option<Result<Value, String>> {
        let n = |k: usize| args.get(k).map(|v| v.int()).unwrap_or(0) as i32;
        let c = |k: usize| args.get(k).map(GuiHost::color).unwrap_or(0xFFFFFF);
        let s = |k: usize| args.get(k).map(|v| v.text()).unwrap_or_default();
        let v = match name {
            "window" => {
                if let Some(ctx) = crate::sched::context() {
                    *ctx.want_size.lock() = Some((n(0).max(160) as usize, n(1).max(120) as usize));
                    if args.len() > 2 {
                        *ctx.title.lock() = s(2);
                    }
                }
                Value::Nil
            }
            "title" => {
                if let Some(ctx) = crate::sched::context() {
                    *ctx.title.lock() = s(0);
                }
                Value::Nil
            }
            "frame" => Value::Int(self.frame() as i64),
            "sleep" => {
                let end = crate::time::uptime_ms() + n(0).max(0) as u64;
                while crate::time::uptime_ms() < end {
                    if !self.frame() {
                        return Some(Ok(Value::Int(0)));
                    }
                }
                Value::Int(1)
            }
            "width" => Value::Int(self.w as i64),
            "height" => Value::Int(self.h as i64),
            "mousex" => Value::Int(self.mouse.0 as i64),
            "mousey" => Value::Int(self.mouse.1 as i64),
            "mousedown" => Value::Int(self.down as i64),
            "clicked" => Value::Int(self.clicked as i64),
            "rclicked" => Value::Int(self.rclicked as i64),
            "wheel" => Value::Int(self.wheel as i64),
            "key" | "getkey" => {
                if self.keys.is_empty() {
                    Value::Int(0)
                } else {
                    Value::Int(key_code(self.keys.remove(0).0))
                }
            }
            "keydown" | "down" => {
                let focused = crate::sched::context().map(|c| c.focused.load(Ordering::Relaxed)).unwrap_or(false);
                let code = match args.first() {
                    Some(Value::Str(t)) => t.chars().next().map(|c| c as i64).unwrap_or(0),
                    Some(v) => v.int(),
                    None => 0,
                };
                Value::Int((focused && crate::input::key_down(code as i32)) as i64)
            }
            "rgb" => Value::Int(gfx::rgb(n(0).clamp(0, 255) as u32, n(1).clamp(0, 255) as u32, n(2).clamp(0, 255) as u32) as i64),
            "mix" => Value::Int(gfx::mix(c(0), c(1), n(2).clamp(0, 255) as u32) as i64),
            "theme" => Value::Int(named_color(&s(0)) as i64),
            "dark" => Value::Int(theme::is_dark() as i64),
            "clear" => {
                let col = if args.is_empty() { theme::current().window } else { c(0) };
                self.buf.fill(col);
                Value::Nil
            }
            "rect" => {
                let col = c(4);
                self.canvas().rect(n(0), n(1), n(2), n(3), col);
                Value::Nil
            }
            "fade" => {
                let (col, a) = (c(4), n(5).clamp(0, 255) as u32);
                self.canvas().rect_a(n(0), n(1), n(2), n(3), col, a);
                Value::Nil
            }
            "rrect" => {
                let col = c(5);
                self.canvas().round(n(0), n(1), n(2), n(3), n(4), col);
                Value::Nil
            }
            "outline" => {
                let col = c(5);
                self.canvas().round_outline(n(0), n(1), n(2), n(3), n(4), col, 255);
                Value::Nil
            }
            "circle" => {
                let col = c(3);
                self.canvas().circle(n(0), n(1), n(2), col);
                Value::Nil
            }
            "line" => {
                let col = c(4);
                let width = if args.len() > 5 { n(5) } else { 1 };
                if width <= 1 {
                    self.canvas().line(n(0), n(1), n(2), n(3), col);
                } else {
                    self.canvas().thick_line(n(0), n(1), n(2), n(3), width, col);
                }
                Value::Nil
            }
            "gradient" => {
                let (c1, c2) = (c(4), c(5));
                let r = if args.len() > 6 { n(6) } else { 0 };
                self.canvas().gradient(n(0), n(1), n(2), n(3), r, c1, c2);
                Value::Nil
            }
            "shadow" => {
                self.canvas().shadow(n(0), n(1), n(2), n(3), n(4), n(5).max(1), 120);
                Value::Nil
            }
            "text" | "center" | "textr" => {
                let size = if args.len() > 4 { n(4) } else { 16 };
                let bold = args.len() > 5 && args[5].truthy();
                let f = gfx::font(size, bold);
                let col = if args.len() > 3 { c(3) } else { theme::current().text };
                let text = s(2);
                let mut cv = self.canvas();
                match name {
                    "center" => cv.text_center(n(0), n(1), &text, &f, col),
                    "textr" => cv.text_right(n(0), n(1), &text, &f, col),
                    _ => {
                        cv.text(n(0), n(1), &text, &f, col);
                    }
                }
                Value::Nil
            }
            "textw" => {
                let size = if args.len() > 1 { n(1) } else { 16 };
                let bold = args.len() > 2 && args[2].truthy();
                Value::Int(gfx::text_width(&gfx::font(size, bold), &s(0)) as i64)
            }
            "button" => {
                let r = (n(0), n(1), n(2), n(3));
                let primary = args.len() > 5 && args[5].truthy();
                let th = theme::current();
                let hover = self.mouse.0 >= r.0 && self.mouse.1 >= r.1 && self.mouse.0 < r.0 + r.2 && self.mouse.1 < r.1 + r.3;
                let pressed = hover && self.down;
                let (bg, fg) = if primary {
                    (if pressed { gfx::darken(th.accent, 40) } else if hover { gfx::lighten(th.accent, 25) } else { th.accent }, th.on_accent)
                } else {
                    (if pressed { gfx::darken(th.control_hover, 25) } else if hover { th.control_hover } else { th.control }, th.text)
                };
                let f = gfx::font(if r.3 >= 44 { 20 } else { 16 }, false);
                let label = s(4);
                {
                    let mut cv = self.canvas();
                    cv.shadow(r.0, r.1 + 1, r.2, r.3, 8, 3, 50);
                    if primary {
                        cv.round(r.0, r.1, r.2, r.3, 8, bg);
                    } else {
                        cv.round_border(r.0, r.1, r.2, r.3, 8, bg, th.control_border);
                    }
                    cv.text_center(r.0 + r.2 / 2, r.1 + (r.3 - f.height() as i32) / 2 - 1, &label, &f, fg);
                }
                if hover {
                    if let Some(ctx) = crate::sched::context() {
                        ctx.cursor.store(super::Cursor::Hand as u8, Ordering::Relaxed);
                    }
                }
                Value::Int((hover && self.clicked) as i64)
            }
            "image" => {
                let path = s(0);
                if !self.images.contains_key(&path) {
                    let img = super::media::load_image(&path).map(|i| (i.px, i.w, i.h));
                    self.images.insert(path.clone(), img);
                }
                if let Some(Some((px, iw, ih))) = self.images.get(&path) {
                    let (dw, dh) = if args.len() > 4 { (n(3), n(4)) } else { (*iw as i32, *ih as i32) };
                    let (px, iw, ih) = (px.clone(), *iw, *ih);
                    self.canvas().blit_scaled((n(1), n(2), dw, dh), &px, iw, ih, true, false);
                    Value::Int(1)
                } else {
                    Value::Int(0)
                }
            }
            "sound" => {
                use crate::audio::{effect, Effect};
                effect(match s(0).as_str() {
                    "score" => Effect::Score,
                    "hurt" => Effect::Hurt,
                    "win" => Effect::Win,
                    "lose" => Effect::Lose,
                    "danger" => Effect::Danger,
                    "open" => Effect::Open,
                    "close" => Effect::Close,
                    _ => Effect::Click,
                });
                Value::Nil
            }
            "emote" => {
                use crate::graphics::buddy::{react, Event as B};
                let code = n(0);
                react(match code {
                    1 => B::Score,
                    2 => B::Hurt,
                    3 => B::Win,
                    4 => B::Danger,
                    _ => B::Lose,
                });
                use crate::audio::{effect, Effect};
                effect(match code {
                    1 => Effect::Score,
                    2 => Effect::Hurt,
                    3 => Effect::Win,
                    4 => Effect::Danger,
                    _ => Effect::Lose,
                });
                Value::Nil
            }
            "say" => {
                super::request(super::Request::BuddySay(s(0)));
                Value::Nil
            }
            "notify" => {
                super::notify(&s(0), &s(1));
                Value::Nil
            }
            "save" => {
                crate::settings::set(&alloc::format!("zs_{}_{}", self.name, s(0)), &s(1));
                Value::Nil
            }
            "load" => match crate::settings::get(&alloc::format!("zs_{}_{}", self.name, s(0))) {
                Some(v) if !v.is_empty() => {
                    if v.parse::<i64>().is_ok() && args.get(1).map(|d| matches!(d, Value::Int(_))).unwrap_or(true) {
                        Value::Int(v.parse().unwrap_or(0))
                    } else {
                        Value::str(&v)
                    }
                }
                _ => args.get(1).cloned().unwrap_or(Value::Nil),
            },
            "time" => {
                let (h, m, sec, ..) = super::desktop::read_rtc();
                Value::str(&alloc::format!("{:02}:{:02}:{:02}", h, m, sec))
            }
            "cursor" => {
                if let Some(ctx) = crate::sched::context() {
                    let k = match s(0).as_str() {
                        "hand" => super::Cursor::Hand,
                        "text" => super::Cursor::Text,
                        _ => super::Cursor::Arrow,
                    };
                    ctx.cursor.store(k as u8, Ordering::Relaxed);
                }
                Value::Nil
            }
            _ => return None,
        };
        let _ = self.active_button;
        Some(Ok(v))
    }
}

fn run_source(name: &str, src: &str) {
    let Some(ctx) = crate::sched::context() else { return };
    let (w, h) = {
        let s = ctx.surface.lock();
        (s.width(), s.height())
    };
    *ctx.title.lock() = String::from(match name {
        "snake" => "Snake",
        "pong" => "Pong",
        "invaders" => "Space Invaders",
        "tictactoe" => "Tic Tac Toe",
        "connect4" => "Connect Four",
        "calculator" => "Calculator",
        "bounce" => "Bounce",
        other => other,
    });
    let mut host = GuiHost {
        w,
        h,
        buf: alloc::vec![theme::current().window; w * h],
        mouse: (0, 0),
        down: false,
        clicked: false,
        rclicked: false,
        wheel: 0,
        keys: Vec::new(),
        held_keys: Vec::new(),
        last_frame: crate::time::uptime_ms(),
        images: BTreeMap::new(),
        name: String::from(name),
        printed: Vec::new(),
        active_button: None,
    };
    let result = zs::compile(src).and_then(|p| zs::run(&p, &mut host));
    match result {
        Ok(()) => {
            // a program that only printed: show what it said
            if !host.printed.is_empty() && !ctx.is_closed() {
                let text = host.printed.join("\n");
                show_text(name, &text);
            }
        }
        Err(e) => {
            if e.message != "stopped" && e.message != "exit" && !ctx.is_closed() {
                show_error(name, &alloc::format!("{}", e));
            }
        }
    }
    let _ = Rc::<u8>::new;
}

/// Shows text in the window until it is closed.
fn show_text(title: &str, text: &str) {
    show_page(title, text, false);
}

/// Shows an error in the window until it is closed.
pub fn show_error(title: &str, message: &str) {
    show_page(title, message, true);
}

fn show_page(title: &str, message: &str, error: bool) {
    let Some(ctx) = crate::sched::context() else { return };
    let mut dirty = true;
    while !ctx.is_closed() {
        loop {
            let ev = ctx.events.lock().pop_front();
            match ev {
                Some(Event::Resize { .. }) => dirty = true,
                Some(_) => {}
                None => break,
            }
        }
        if dirty {
            dirty = false;
            let th = theme::current();
            let mut s = ctx.surface.lock();
            let (w, h) = (s.width(), s.height());
            {
                let mut cv = Canvas::new(s.pixels32(), w, h);
                cv.fill(th.window);
                let tf = gfx::font(24, true);
                let f = gfx::font(16, false);
                if error {
                    cv.circle(36, 40, 14, 0xFF453A);
                    cv.rect(35, 31, 3, 11, 0xFFFFFF);
                    cv.rect(35, 45, 3, 3, 0xFFFFFF);
                }
                cv.text(if error { 62 } else { 20 }, 26, title, &tf, th.text);
                let mut y = 72;
                for line in message.split('\n') {
                    for l in gfx::wrap(&f, line, w as i32 - 40) {
                        cv.text(20, y, &l, &f, if error { th.text2 } else { th.text });
                        y += 22;
                    }
                }
            }
            s.damage_rows(0, h);
        }
        crate::sched::yield_idle();
    }
}
