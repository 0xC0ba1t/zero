use crate::graphics::display::Display;
use bootloader_api::info::FrameBufferInfo;
use spin::Mutex;

pub static FRAMEBUFFER: Mutex<Option<Framebuffer>> = Mutex::new(None);

pub struct Framebuffer {
    info: FrameBufferInfo,
    buffer: &'static mut [u8],
}

impl Framebuffer {
    pub fn new(info: FrameBufferInfo, buffer: &'static mut [u8]) -> Self {
        Self { info, buffer }
    }
}

impl Display for Framebuffer {
    fn put_pixel(&mut self, x: usize, y: usize, color: u32) {
        if x < self.info.width && y < self.info.height {
            let pixel_offset = y * self.info.stride + x;
            let color_bytes = color.to_le_bytes(); // Assuming ARGB/RGBA format
            let byte_offset = pixel_offset * 4;
            // Ensure we don't go out of bounds of the buffer
            if byte_offset + 4 <= self.buffer.len() {
                self.buffer[byte_offset..byte_offset + 4].copy_from_slice(&color_bytes);
            }
        }
    }

    fn clear(&mut self) {
        self.buffer.fill(0);
    }

    fn width(&self) -> usize {
        self.info.width
    }

    fn height(&self) -> usize {
        self.info.height
    }
}
