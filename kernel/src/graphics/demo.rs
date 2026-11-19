use crate::graphics::cube::Cube;
use crate::graphics::display::Display;
use crate::graphics::framebuffer::Framebuffer;
use crate::graphics::vga::BACKGROUND;

/// Spins the cube until a key is pressed.
pub fn run_demo(display: &mut Framebuffer) {
    let mut cube = Cube::new();
    let mut last = crate::time::ticks();

    loop {
        display.clear_to(BACKGROUND);
        cube.render(display);
        display.present();
        cube.rotation += 0.05;

        if crate::input::try_read_key().is_some() {
            break;
        }

        // wait for the next timer tick (10 ms) instead of burning the CPU
        while crate::time::ticks() == last {
            x86_64::instructions::hlt();
        }
        last = crate::time::ticks();
    }
}
