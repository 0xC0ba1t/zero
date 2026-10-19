use crate::graphics::display::Display;
use crate::graphics::cube::Cube;
use crate::graphics::framebuffer::Framebuffer;

pub fn run_demo(display: &mut Framebuffer) {
    let mut cube = Cube::new();

    loop {
        display.clear();
        cube.render(display);
        cube.rotation += 0.05;

        // Check for exit condition (e.g., any key)
        if crate::input::try_read_key().is_some() {
            break;
        }

        // Small delay to control animation speed
        for _ in 0..1000000 { core::hint::spin_loop(); }
    }
}
