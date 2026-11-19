//! Something pixels can be drawn on.

/// A pixel surface. Colors are `0x00RRGGBB`.
pub trait Display {
    fn put_pixel(&mut self, x: usize, y: usize, color: u32);
    fn clear(&mut self);
    fn width(&self) -> usize;
    fn height(&self) -> usize;

    /// Fills a rectangle. Surfaces should override this with something faster
    /// than a pixel loop.
    fn fill_rect(&mut self, x: usize, y: usize, w: usize, h: usize, color: u32) {
        for yy in y..y.saturating_add(h) {
            for xx in x..x.saturating_add(w) {
                self.put_pixel(xx, yy, color);
            }
        }
    }

    /// Makes everything drawn so far visible. Buffered surfaces (like the
    /// framebuffer) only show their changes after this call.
    fn present(&mut self) {}
}
