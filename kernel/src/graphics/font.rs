//! Text rendering with the anti-aliased Noto Sans Mono bitmap font.
//!
//! Text is the only thing in the kernel that goes through a font. Everything
//! else (the boot screen, script graphics) is drawn as pixels.

use crate::graphics::framebuffer::Framebuffer;
use noto_sans_mono_bitmap::{get_raster, get_raster_width, FontWeight, RasterHeight};

#[derive(Clone, Copy)]
pub struct Font {
    size: RasterHeight,
    weight: FontWeight,
}

/// Sizes available, smallest first.
const SIZES: [RasterHeight; 4] = [
    RasterHeight::Size16,
    RasterHeight::Size20,
    RasterHeight::Size24,
    RasterHeight::Size32,
];

impl Font {
    pub const fn regular(size: RasterHeight) -> Font {
        Font { size, weight: FontWeight::Regular }
    }

    pub const fn bold(size: RasterHeight) -> Font {
        Font { size, weight: FontWeight::Bold }
    }

    /// The font the shell uses.
    pub const fn console() -> Font {
        Font::regular(RasterHeight::Size20)
    }

    /// The shell font for a screen that is `width` pixels wide: bigger
    /// screens get bigger text so it stays readable.
    pub const fn console_for_width(width: usize) -> Font {
        if width >= 3000 {
            Font::regular(RasterHeight::Size32)
        } else if width >= 2200 {
            Font::regular(RasterHeight::Size24)
        } else {
            Font::regular(RasterHeight::Size20)
        }
    }

    /// The largest regular font whose glyphs fit in a `w` x `h` cell.
    pub fn fit(w: usize, h: usize) -> Font {
        let mut best = Font::regular(SIZES[0]);
        for size in SIZES {
            let font = Font::regular(size);
            if font.width() <= w && font.height() <= h {
                best = font;
            }
        }
        best
    }

    /// Returns the same size in bold.
    pub const fn in_bold(self) -> Font {
        Font { size: self.size, weight: FontWeight::Bold }
    }

    pub fn is_bold(&self) -> bool {
        matches!(self.weight, FontWeight::Bold)
    }

    pub fn width(&self) -> usize {
        get_raster_width(self.weight, self.size)
    }

    pub fn height(&self) -> usize {
        self.size.val()
    }

    /// The coverage rows of `c` (a `?` for characters the font lacks).
    pub fn raster(&self, c: char) -> &'static [&'static [u8]] {
        match get_raster(c, self.weight, self.size).or_else(|| get_raster('?', self.weight, self.size)) {
            Some(glyph) => glyph.raster(),
            None => &[],
        }
    }

    /// Draws one character with its top-left corner at `(x, y)`.
    pub fn draw_char(&self, fb: &mut Framebuffer, x: usize, y: usize, c: char, fg: u32, bg: u32) {
        let glyph = match get_raster(c, self.weight, self.size) {
            Some(glyph) => glyph,
            None => match get_raster('?', self.weight, self.size) {
                Some(glyph) => glyph,
                None => return,
            },
        };
        fb.draw_alpha(x, y, glyph.raster(), fg, bg);
    }

    /// Draws a string on one line; returns the x position after the last character.
    pub fn draw_text(&self, fb: &mut Framebuffer, x: usize, y: usize, text: &str, fg: u32, bg: u32) -> usize {
        let w = self.width();
        let mut cx = x;
        for c in text.chars() {
            self.draw_char(fb, cx, y, c, fg, bg);
            cx += w;
        }
        cx
    }

    /// Width in pixels of `text` when drawn on one line.
    pub fn text_width(&self, text: &str) -> usize {
        text.chars().count() * self.width()
    }
}

#[test_case]
fn test_fit_picks_the_largest_font_that_fits() {
    assert_eq!(Font::fit(1000, 1000).height(), 32);
    assert_eq!(Font::fit(9, 16).height(), 16);
    assert!(Font::fit(24, 43).width() <= 24);
}
