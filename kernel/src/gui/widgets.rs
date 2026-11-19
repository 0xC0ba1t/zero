//! A tiny toolkit for the desktop's own applications (Files, Settings):
//! an event loop and a few controls.

use super::draw::{self, accent, BG_HOVER, BG_PANEL, BORDER, TEXT, TEXT_MUTED};
use super::Event;
use crate::graphics::display::Display;
use crate::graphics::framebuffer::{self, Framebuffer};

/// An application with its own window contents.
pub trait AppUi {
    /// Handles one event; returns true if the window must be repainted.
    fn event(&mut self, ev: Event, w: i32, h: i32) -> bool;
    /// Called about every 50 ms; returns true if the window must be repainted.
    fn tick(&mut self) -> bool {
        false
    }
    /// Paints the whole window.
    fn draw(&mut self, fb: &mut Framebuffer);
}

/// Runs an application until its window is closed.
pub fn run(app: &mut dyn AppUi) {
    let Some(ctx) = crate::sched::context() else { return };
    let mut dirty = true;
    let mut last_tick = 0;
    loop {
        if ctx.is_closed() {
            break;
        }
        let (w, h) = framebuffer::with(|fb| (fb.width() as i32, fb.height() as i32)).unwrap_or((0, 0));
        loop {
            let ev = ctx.events.lock().pop_front();
            match ev {
                Some(Event::Resize { .. }) => dirty = true,
                Some(ev) => dirty |= app.event(ev, w, h),
                None => break,
            }
        }
        let now = crate::time::uptime_ms();
        if now >= last_tick + 50 {
            last_tick = now;
            dirty |= app.tick();
        }
        if dirty {
            framebuffer::with(|fb| app.draw(fb));
            dirty = false;
            crate::sched::note_progress();
        }
        crate::sched::yield_idle();
    }
}

pub fn inside(x: i32, y: i32, r: (i32, i32, i32, i32)) -> bool {
    x >= r.0 && y >= r.1 && x < r.0 + r.2 && y < r.1 + r.3
}

/// A push button.
pub fn button(fb: &mut Framebuffer, font: &crate::graphics::font::Font, r: (i32, i32, i32, i32), label: &str, hover: bool, primary: bool) {
    let bg = if primary {
        accent()
    } else if hover {
        BG_HOVER
    } else {
        BG_PANEL
    };
    draw::round(fb, r.0, r.1, r.2, r.3, 6, bg);
    if !primary {
        // a thin border: draw the outline slightly inset so the corners stay round
        draw::tint(fb, r.0 + 3, r.1, r.2 - 6, 1, BORDER, 255);
        draw::tint(fb, r.0 + 3, r.1 + r.3 - 1, r.2 - 6, 1, BORDER, 255);
    }
    let color = if primary { 0x14171C } else { TEXT };
    draw::text_centered(fb, font, r.0 + r.2 / 2, r.1 + (r.3 - font.height() as i32) / 2, label, color);
}

/// A toggle switch (iOS style).
pub fn switch(fb: &mut Framebuffer, x: i32, y: i32, on: bool) {
    let (w, h) = (40, 22);
    draw::round(fb, x, y, w, h, h / 2, if on { accent() } else { 0x4B5160 });
    let knob = if on { x + w - h + 2 } else { x + 2 };
    draw::disc(fb, knob + (h - 4) / 2, y + h / 2, (h - 4) / 2, 0xFFFFFF);
}

/// A slider track with a knob; `value` is 0..=100.
pub fn slider(fb: &mut Framebuffer, x: i32, y: i32, w: i32, value: i32) {
    draw::round(fb, x, y + 8, w, 6, 3, 0x4B5160);
    let filled = w * value.clamp(0, 100) / 100;
    draw::round(fb, x, y + 8, filled.max(6), 6, 3, accent());
    draw::disc(fb, x + filled, y + 11, 9, 0xF4F6FA);
}

pub fn label(fb: &mut Framebuffer, font: &crate::graphics::font::Font, x: i32, y: i32, s: &str, muted: bool) {
    draw::text(fb, font, x, y, s, if muted { TEXT_MUTED } else { TEXT });
}
