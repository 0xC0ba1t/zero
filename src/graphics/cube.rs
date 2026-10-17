use alloc::vec::Vec;
use crate::graphics::display::Display;
use crate::graphics::math::{Mat4, Vec3};
use crate::graphics::rasterizer::Rasterizer;

pub struct Cube {
    pub position: Vec3,
    pub rotation: f32,
}

impl Cube {
    pub fn new() -> Self {
        Self {
            position: Vec3::new(0.0, 0.0, -5.0),
            rotation: 0.0,
        }
    }

    pub fn render(&self, display: &mut dyn Display) {
        let mut rasterizer = Rasterizer::new(display);

        // Define cube vertices
        let vertices = [
            Vec3::new(-1.0, -1.0,  1.0), Vec3::new( 1.0, -1.0,  1.0),
            Vec3::new( 1.0,  1.0,  1.0), Vec3::new(-1.0,  1.0,  1.0),
            Vec3::new(-1.0, -1.0, -1.0), Vec3::new( 1.0, -1.0, -1.0),
            Vec3::new( 1.0,  1.0, -1.0), Vec3::new(-1.0,  1.0, -1.0),
        ];

        // Define edges (pairs of vertex indices)
        let edges = [
            (0, 1), (1, 2), (2, 3), (3, 0),
            (4, 5), (5, 6), (6, 7), (7, 4),
            (0, 4), (1, 5), (2, 6), (3, 7),
        ];

        // Simple rotation matrix
        let rotation_matrix = Mat4::from_rotation_y(self.rotation) * Mat4::from_rotation_x(self.rotation * 0.5);
        let projection_matrix = Mat4::perspective_rh(1.0, 1.33, 0.1, 100.0);

        let transform = projection_matrix * Mat4::from_translation(self.position) * rotation_matrix;

        let mut projected_vertices: Vec<_> = Vec::with_capacity(vertices.len());
        for v in vertices {
            let p = transform.project_point3(v);
            // Map from [-1, 1] to screen coordinates
            let x = ((p.x + 1.0) * 0.5 * display.width() as f32) as i32;
            let y = ((1.0 - p.y) * 0.5 * display.height() as f32) as i32;
            projected_vertices.push((x, y));
        }

        for &(i, j) in &edges {
            rasterizer.draw_line(
                projected_vertices[i].0, projected_vertices[i].1,
                projected_vertices[j].0, projected_vertices[j].1,
                0xFFFFFF,
            );
        }
    }
}
