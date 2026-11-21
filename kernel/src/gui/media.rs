//! Media: a photo viewer, a music player and an animation player in one.

use super::gfx::{self, Canvas};
use super::icons::{self, Icon};
use super::ui::{id, idn, App, MenuItem, Ui};
use crate::codecs;
use crate::fs::FILESYSTEM;
use crate::input::Key;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

/// An opaque picture (0x00RRGGBB).
pub struct Image {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u32>,
}

pub fn load_image(path: &str) -> Option<Image> {
    let data = FILESYSTEM.lock().as_ref()?.read_file(path).ok()?;
    let p = codecs::decode_picture(&data).ok()?;
    Some(flatten(p))
}

fn flatten(p: codecs::Picture) -> Image {
    let px = p.px.iter().map(|&c| gfx::blend(0x1E1E22, c & 0xFFFFFF, c >> 24)).collect();
    Image { w: p.w, h: p.h, px }
}

/// Puts the sample pictures, music and animations in place (once).
pub fn install_samples() {
    if crate::settings::get("samples3").is_some() {
        return;
    }
    let files: [(&str, &[u8]); 7] = [
        ("/Pictures/Mountains.png", include_bytes!("../../../assets/media/Mountains.png")),
        ("/Pictures/Aurora.png", include_bytes!("../../../assets/media/Aurora.png")),
        ("/Pictures/Bit.bmp", include_bytes!("../../../assets/media/Bit.bmp")),
        ("/Movies/Starfield.gif", include_bytes!("../../../assets/media/Starfield.gif")),
        ("/Movies/Bounce.gif", include_bytes!("../../../assets/media/Bounce.gif")),
        ("/Music/Chiptune.wav", include_bytes!("../../../assets/media/Chiptune.wav")),
        ("/Documents/Welcome.txt", include_bytes!("../../../assets/media/Welcome.txt")),
    ];
    if let Some(fs) = FILESYSTEM.lock().as_mut() {
        for (path, data) in files {
            let _ = fs.write_file(path, data.to_vec());
        }
        let _ = fs.write_file("/Desktop/Welcome.txt", include_bytes!("../../../assets/media/Welcome.txt").to_vec());
    }
    crate::settings::set("samples3", "1");
}

enum Doc {
    Empty,
    Picture(Image),
    Animation(codecs::Animation),
    Sound { data: Arc<Vec<i16>>, peaks: Vec<u8> },
    Error(String),
}

pub struct Media {
    path: Option<String>,
    doc: Doc,
    zoom: i32,
    pan: (i32, i32),
    drag: Option<((i32, i32), (i32, i32))>,
    frame: usize,
    frame_at: u64,
    playing: bool,
    slideshow: Option<u64>,
    library: Vec<String>,
    last_arg: Option<String>,
    rotate: u8,
}

impl Media {
    pub fn new(arg: Option<String>) -> Media {
        let mut m = Media {
            path: None,
            doc: Doc::Empty,
            zoom: 0,
            pan: (0, 0),
            drag: None,
            frame: 0,
            frame_at: 0,
            playing: true,
            slideshow: None,
            library: Vec::new(),
            last_arg: arg.clone(),
            rotate: 0,
        };
        m.scan();
        if let Some(p) = arg {
            m.open(&p);
        }
        m
    }

    fn scan(&mut self) {
        self.library.clear();
        let fs = FILESYSTEM.lock();
        let Some(fs) = fs.as_ref() else { return };
        for dir in ["/Pictures", "/Music", "/Movies", "/Downloads", "/Desktop"] {
            for e in fs.entries(dir).unwrap_or_default() {
                if !e.is_dir && matches!(super::apps::for_file(&e.name), super::apps::AppId::Media) {
                    self.library.push(super::files::join(dir, &e.name));
                }
            }
        }
    }

    fn open(&mut self, path: &str) {
        crate::audio::stream_stop();
        self.path = Some(String::from(path));
        self.zoom = 0;
        self.pan = (0, 0);
        self.frame = 0;
        self.rotate = 0;
        self.playing = true;
        self.frame_at = crate::time::uptime_ms();
        let data = FILESYSTEM.lock().as_ref().and_then(|fs| fs.read_file(path).ok());
        let Some(data) = data else {
            self.doc = Doc::Error(String::from("The file could not be read."));
            return;
        };
        let lower = path.to_ascii_lowercase();
        self.doc = if lower.ends_with(".wav") {
            match codecs::wav(&data) {
                Ok(s) => {
                    let pcm = codecs::to_48k_stereo(&s);
                    // a waveform overview: 200 peaks
                    let n = 200;
                    let chunk = (pcm.len() / n).max(1);
                    let peaks = (0..n).map(|k| pcm.iter().skip(k * chunk).take(chunk).step_by(7).map(|v| v.unsigned_abs()).max().unwrap_or(0) as u32 * 255 / 32768).map(|v| v.min(255) as u8).collect();
                    let data = Arc::new(pcm);
                    crate::audio::play_stream(data.clone());
                    Doc::Sound { data, peaks }
                }
                Err(e) => Doc::Error(e),
            }
        } else if lower.ends_with(".gif") {
            match codecs::gif(&data) {
                Ok(a) if a.frames.len() > 1 => Doc::Animation(a),
                Ok(a) => {
                    let (px, _) = a.frames.into_iter().next().unwrap();
                    Doc::Picture(flatten(codecs::Picture { w: a.w, h: a.h, px }))
                }
                Err(e) => Doc::Error(e),
            }
        } else {
            match codecs::decode_picture(&data) {
                Ok(p) => Doc::Picture(flatten(p)),
                Err(e) => Doc::Error(e),
            }
        };
    }

    fn neighbour(&mut self, delta: i32) {
        let Some(p) = self.path.clone() else { return };
        let pics: Vec<String> = self.library.iter().filter(|q| !q.to_ascii_lowercase().ends_with(".wav")).cloned().collect();
        if let Some(i) = pics.iter().position(|q| *q == p) {
            let n = pics.len() as i32;
            let j = ((i as i32 + delta).rem_euclid(n)) as usize;
            let next = pics[j].clone();
            self.open(&next);
        }
    }
}

impl Drop for Media {
    fn drop(&mut self) {
        if let Doc::Sound { .. } = self.doc {
            crate::audio::stream_stop();
        }
    }
}

const SIDE: i32 = 200;
const BAR: i32 = 48;

fn fmt_time(samples: usize) -> String {
    let s = samples / 2 / 48000;
    alloc::format!("{}:{:02}", s / 60, s % 60)
}

impl App for Media {
    fn title(&self) -> String {
        match &self.path {
            Some(p) => String::from(super::files::base(p)),
            None => String::from("Media"),
        }
    }

    fn menus(&self) -> Vec<(&'static str, Vec<MenuItem>)> {
        let m = MenuItem::new;
        alloc::vec![("View", alloc::vec![m("Zoom In"), m("Zoom Out"), m("Fit to Window"), m("Actual Size"), MenuItem::sep(), m("Rotate"), m("Slideshow"), MenuItem::sep(), m("Set as Wallpaper")])]
    }

    fn on_menu(&mut self, _menu: usize, item: usize) {
        match item {
            0 => self.zoom = (if self.zoom == 0 { 100 } else { self.zoom }) * 5 / 4,
            1 => self.zoom = ((if self.zoom == 0 { 100 } else { self.zoom }) * 4 / 5).max(10),
            2 => self.zoom = 0,
            3 => self.zoom = 100,
            5 => self.rotate = (self.rotate + 1) % 4,
            6 => self.slideshow = if self.slideshow.is_some() { None } else { Some(crate::time::uptime_ms()) },
            8 => {
                if let (Some(p), Doc::Picture(_)) = (&self.path, &self.doc) {
                    *super::desktop::WALL_IMAGE.lock() = Some(p.clone());
                    crate::settings::set("wall_image", p);
                    super::desktop::WALL_GEN.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                    super::notify("Wallpaper", "Your desktop picture was changed.");
                }
            }
            _ => {}
        }
    }

    fn tick(&mut self) -> bool {
        match &self.doc {
            Doc::Animation(_) => self.playing,
            Doc::Sound { .. } => true,
            _ => self.slideshow.is_some(),
        }
    }

    fn frame(&mut self, ui: &mut Ui) {
        if let Some(a) = super::app_arg() {
            if Some(&a) != self.last_arg.as_ref() {
                self.last_arg = Some(a.clone());
                self.scan();
                self.open(&a);
            }
        }
        let th = ui.th;
        let (w, h) = (ui.w, ui.h);
        let stage = if th.dark { 0x141417 } else { 0x2A2A2E };
        ui.fill(th.window);

        // library
        ui.cv.rect(0, 0, SIDE, h, th.sidebar);
        ui.cv.vline(SIDE, 0, h, th.separator, 255);
        let small = gfx::font(16, true);
        ui.cv.text(16, 14, "Library", &small, th.text3);
        let lib = self.library.clone();
        let sid = id("libscroll");
        let lr = (0, 38, SIDE, h - 38);
        let off = ui.scroll_begin(sid, lr, lib.len() as i32 * 34 + 10);
        for (k, p) in lib.iter().enumerate() {
            let r = (8, 40 + k as i32 * 34 - off, SIDE - 16, 32);
            let sel = self.path.as_deref() == Some(p.as_str());
            if ui.flat(idn("lib", k), r, sel).clicked {
                let p = p.clone();
                self.open(&p);
            }
            let name = super::files::base(p);
            icons::draw(&mut ui.cv, super::apps::file_icon(name, false), r.0 + 6, r.1 + 5, 22);
            let f = ui.font;
            ui.cv.text(r.0 + 34, r.1 + 7, &gfx::ellipsize(&f, name, r.2 - 40), &f, if sel { th.on_accent } else { th.text });
        }
        ui.scroll_end(sid, lr, lib.len() as i32 * 34 + 10);

        // toolbar
        let main = (SIDE + 1, BAR, w - SIDE - 1, h - BAR);
        ui.cv.rect(SIDE + 1, 0, w - SIDE, BAR, th.window);
        ui.cv.hline(SIDE + 1, BAR - 1, w, th.separator, 255);
        let tx = SIDE + 12;
        let is_pic = matches!(self.doc, Doc::Picture(_) | Doc::Animation(_));
        if is_pic {
            if ui.button((tx, 9, 30, 30), "<") {
                self.neighbour(-1);
            }
            if ui.button((tx + 34, 9, 30, 30), ">") {
                self.neighbour(1);
            }
            if ui.button((w - 252, 9, 34, 30), "-") {
                self.zoom = ((if self.zoom == 0 { 100 } else { self.zoom }) * 4 / 5).max(10);
            }
            if ui.button((w - 214, 9, 50, 30), "Fit") {
                self.zoom = 0;
                self.pan = (0, 0);
            }
            if ui.button((w - 160, 9, 34, 30), "+") {
                self.zoom = ((if self.zoom == 0 { 100 } else { self.zoom }) * 5 / 4).min(1600);
            }
            let label = if self.slideshow.is_some() { "Stop" } else { "Slideshow" };
            if ui.button((w - 120, 9, 108, 30), label) {
                self.slideshow = if self.slideshow.is_some() { None } else { Some(ui.now) };
            }
        }
        let title = self.title();
        let bf = ui.bold;
        ui.cv.text(if is_pic { tx + 76 } else { tx }, 15, &gfx::ellipsize(&bf, &title, 260), &bf, th.text);

        // keys
        for (k, _) in ui.keys().to_vec() {
            match k {
                Key::Left => self.neighbour(-1),
                Key::Right => self.neighbour(1),
                Key::Char(' ') => {
                    if let Doc::Sound { .. } = self.doc {
                        let playing = crate::audio::stream_state().map(|s| s.2).unwrap_or(false);
                        crate::audio::stream_set(None, Some(!playing));
                    } else {
                        self.playing = !self.playing;
                    }
                }
                Key::Char('+') | Key::Char('=') => self.zoom = ((if self.zoom == 0 { 100 } else { self.zoom }) * 5 / 4).min(1600),
                Key::Char('-') => self.zoom = ((if self.zoom == 0 { 100 } else { self.zoom }) * 4 / 5).max(10),
                Key::Char('0') => self.zoom = 0,
                _ => {}
            }
        }
        if let Some(t) = self.slideshow {
            if ui.now > t + 3500 {
                self.slideshow = Some(ui.now);
                self.neighbour(1);
            }
            ui.animate(100);
        }

        ui.cv.push(main, false);
        match &self.doc {
            Doc::Empty => {
                ui.cv.rect(main.0, main.1, main.2, main.3, th.window);
                icons::draw(&mut ui.cv, Icon::Media, main.0 + main.2 / 2 - 40, main.1 + main.3 / 2 - 90, 80);
                let f = ui.big;
                ui.cv.text_center(main.0 + main.2 / 2, main.1 + main.3 / 2 + 10, "Pick something from your library", &f, th.text2);
            }
            Doc::Error(e) => {
                let f = ui.big;
                ui.cv.text_center(main.0 + main.2 / 2, main.1 + main.3 / 2 - 20, "This file can't be shown", &f, th.text);
                let e = e.clone();
                ui.cv.text_center(main.0 + main.2 / 2, main.1 + main.3 / 2 + 14, &e, &ui.font.clone(), th.text2);
            }
            Doc::Picture(_) | Doc::Animation(_) => {
                ui.cv.rect(main.0, main.1, main.2, main.3, stage);
                let (iw, ih, px): (usize, usize, &[u32]) = match &self.doc {
                    Doc::Picture(img) => (img.w, img.h, &img.px),
                    Doc::Animation(a) => (a.w, a.h, &a.frames[self.frame.min(a.frames.len() - 1)].0),
                    _ => unreachable!(),
                };
                // rotation makes a rotated copy (rarely used, so it is simple)
                let rotated: Vec<u32>;
                let (iw, ih, px) = if self.rotate % 2 == 1 || self.rotate == 2 {
                    let (rw, rh) = if self.rotate % 2 == 1 { (ih, iw) } else { (iw, ih) };
                    let mut out = alloc::vec![0u32; rw * rh];
                    for y in 0..ih {
                        for x in 0..iw {
                            let (nx, ny) = match self.rotate {
                                1 => (ih - 1 - y, x),
                                2 => (iw - 1 - x, ih - 1 - y),
                                _ => (y, iw - 1 - x),
                            };
                            out[ny * rw + nx] = px[y * iw + x];
                        }
                    }
                    rotated = out;
                    (rw, rh, &rotated[..])
                } else {
                    (iw, ih, px)
                };
                let area = (main.0 + 16, main.1 + 16, main.2 - 32, main.3 - 32 - if matches!(self.doc, Doc::Animation(_)) { 50 } else { 0 });
                let fit = ((area.2 as i64 * 1000 / iw.max(1) as i64).min(area.3 as i64 * 1000 / ih.max(1) as i64)).min(if self.zoom == 0 { 4000 } else { 100000 }) as i32;
                let scale = if self.zoom == 0 { fit.min(2000) } else { self.zoom * 10 };
                let (dw, dh) = ((iw as i64 * scale as i64 / 1000) as i32, (ih as i64 * scale as i64 / 1000) as i32);
                let (dx, dy) = (area.0 + (area.2 - dw) / 2 + self.pan.0, area.1 + (area.3 - dh) / 2 + self.pan.1);
                ui.cv.shadow(dx, dy + 6, dw, dh, 2, 24, 140);
                ui.cv.blit_scaled((dx, dy, dw, dh), px, iw, ih, scale < 1000, false);
                // dragging pans a zoomed picture; the wheel zooms
                let resp = ui.interact(id("stage"), main);
                if resp.pressed {
                    self.drag = Some((ui.mouse(), self.pan));
                }
                if let Some((m0, p0)) = self.drag {
                    if ui.inp.down {
                        let m = ui.mouse();
                        self.pan = (p0.0 + m.0 - m0.0, p0.1 + m.1 - m0.1);
                    } else {
                        self.drag = None;
                    }
                }
                if resp.hover && ui.inp.wheel != 0 {
                    let cur = if self.zoom == 0 { scale / 10 } else { self.zoom };
                    self.zoom = if ui.inp.wheel < 0 { cur * 5 / 4 } else { cur * 4 / 5 }.clamp(5, 1600);
                }
                if resp.double {
                    self.zoom = 0;
                    self.pan = (0, 0);
                }
                let label = alloc::format!("{} x {}   {}%", iw, ih, scale / 10);
                let f = ui.font;
                let lw = gfx::text_width(&f, &label) + 20;
                ui.cv.round_a(main.0 + main.2 - lw - 12, main.1 + 10, lw, 26, 13, 0x000000, 140);
                ui.cv.text(main.0 + main.2 - lw - 2, main.1 + 14, &label, &f, 0xFFFFFF);
                // animation controls
                if let Doc::Animation(a) = &self.doc {
                    let n = a.frames.len();
                    if self.playing && ui.now >= self.frame_at + a.frames[self.frame.min(n - 1)].1 as u64 {
                        self.frame = (self.frame + 1) % n;
                        self.frame_at = ui.now;
                    }
                    let by = main.1 + main.3 - 46;
                    if ui.button((main.0 + 16, by, 74, 32), if self.playing { "Pause" } else { "Play" }) {
                        self.playing = !self.playing;
                    }
                    let mut f = self.frame as i32;
                    if ui.slider(id("frames"), (main.0 + 106, by + 4, main.2 - 220, 24), &mut f, 0, n as i32 - 1) {
                        self.frame = f.clamp(0, n as i32 - 1) as usize;
                        self.playing = false;
                    }
                    let lbl = alloc::format!("{} / {}", self.frame + 1, n);
                    ui.cv.text(main.0 + main.2 - 96, by + 7, &lbl, &ui.font.clone(), 0xFFFFFF);
                }
            }
            Doc::Sound { data, peaks } => {
                let (data_len, peaks) = (data.len(), peaks.clone());
                ui.cv.rect(main.0, main.1, main.2, main.3, th.window);
                let cx = main.0 + main.2 / 2;
                // album art
                let art = 180;
                let (ax, ay) = (cx - art / 2, main.1 + 30);
                ui.cv.shadow(ax, ay + 8, art, art, 18, 28, 120);
                ui.cv.gradient(ax, ay, art, art, 18, 0xFF6B9A, 0x6A3DE8);
                let state = crate::audio::stream_state();
                let (pos, len, playing) = state.unwrap_or((0, data_len, false));
                // a note that pulses with the music
                let beat = if playing { ((ui.now / 120) % 6) as i32 } else { 0 };
                ui.cv.circle(ax + 70, ay + 120, 18 + beat / 2, 0xFFFFFF);
                ui.cv.circle(ax + 118, ay + 108, 18 + beat / 2, 0xFFFFFF);
                ui.cv.rect(ax + 84, ay + 50, 6, 70, 0xFFFFFF);
                ui.cv.rect(ax + 132, ay + 40, 6, 70, 0xFFFFFF);
                ui.cv.thick_line(ax + 87, ay + 52, ax + 135, ay + 42, 8, 0xFFFFFF);
                let title = self.title();
                let f = ui.big;
                ui.cv.text_center(cx, ay + art + 22, &title, &f, th.text);
                ui.cv.text_center(cx, ay + art + 52, "WAV audio", &ui.font.clone(), th.text2);
                // waveform with progress
                let wave = (main.0 + 40, ay + art + 84, main.2 - 80, 60);
                let n = peaks.len().max(1) as i32;
                let done_x = wave.0 + (wave.2 as i64 * pos as i64 / len.max(1) as i64) as i32;
                for (k, p) in peaks.iter().enumerate() {
                    let x = wave.0 + k as i32 * wave.2 / n;
                    let hh = (*p as i32 * wave.3 / 255).max(2);
                    let c = if x < done_x { th.accent } else { gfx::mix(th.window, th.text, 70) };
                    ui.cv.round(x, wave.1 + (wave.3 - hh) / 2, (wave.2 / n - 1).max(1), hh, 1, c);
                }
                let resp = ui.interact(id("wave"), wave);
                if resp.pressed || (ui.st.active == id("wave") && ui.inp.down) {
                    let mx = ui.mouse().0;
                    let p = ((mx - wave.0).clamp(0, wave.2) as i64 * len as i64 / wave.2.max(1) as i64) as usize;
                    crate::audio::stream_set(Some(p), None);
                }
                let f = ui.font;
                ui.cv.text(wave.0, wave.1 + wave.3 + 8, &fmt_time(pos), &f, th.text2);
                ui.cv.text_right(wave.0 + wave.2, wave.1 + wave.3 + 8, &fmt_time(len), &f, th.text2);
                let by = wave.1 + wave.3 + 40;
                if ui.button((cx - 140, by, 80, 34), "-10s") {
                    crate::audio::stream_set(Some(pos.saturating_sub(48000 * 20)), None);
                }
                if ui.primary((cx - 50, by, 100, 34), if playing { "Pause" } else { "Play" }) {
                    crate::audio::stream_set(None, Some(!playing));
                }
                if ui.button((cx + 60, by, 80, 34), "+10s") {
                    crate::audio::stream_set(Some(pos + 48000 * 20), None);
                }
                let mut vol = crate::audio::VOLUME.load(core::sync::atomic::Ordering::Relaxed) as i32;
                ui.cv.text(cx - 140, by + 52, "Volume", &f, th.text2);
                if ui.slider(id("vol"), (cx - 70, by + 50, 210, 24), &mut vol, 0, 100) {
                    crate::audio::VOLUME.store(vol as u32, core::sync::atomic::Ordering::Relaxed);
                }
                if !crate::audio::available() {
                    ui.cv.text_center(cx, by + 90, "No sound card found: start QEMU with -device AC97", &f, th.text3);
                }
            }
        }
        ui.cv.pop();
        let _ = Canvas::new;
    }
}
