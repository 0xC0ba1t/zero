pub trait Display {
    fn put_pixel(&mut self, x: usize, y: usize, color: u32);
    fn clear(&mut self);
    fn width(&self) -> usize;
    fn height(&self) -> usize;
}
