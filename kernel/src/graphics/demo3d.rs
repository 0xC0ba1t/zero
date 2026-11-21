//! A small software 3D renderer and a set of demos.
//!
//! Only vertex work uses floating point (there is no FPU in the kernel's
//! target, so every float operation is emulated and costs real time); the
//! per-pixel work is integer: anti-aliased lines use Wu's algorithm in fixed
//! point and triangles are filled one scanline span at a time.

use crate::graphics::display::Display;
use crate::graphics::framebuffer::{self, Framebuffer};
use crate::graphics::math::{Mat4, Vec3, Vec4};
use crate::graphics::ui::{self, lerp};
use crate::input::{self, Key};
use alloc::format;
use alloc::vec;
use alloc::vec::Vec;
use libm::{cosf, sinf};

const BG: u32 = ui::BG_EDITOR;
const TAU: f32 = 6.2831855;

// ---------------------------------------------------------------- canvas

/// A rectangle of the screen that 3D drawing is clipped to.
pub struct Canvas<'a> {
    pub fb: &'a mut Framebuffer,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Canvas<'_> {
    #[inline]
    fn plot(&mut self, px: i64, py: i64, color: u32, alpha: u32) {
        if px >= self.x as i64 && px < (self.x + self.w) as i64 && py >= self.y as i64 && py < (self.y + self.h) as i64 {
            self.fb.blend_pixel(px as usize, py as usize, color, alpha);
        }
    }

    /// A square dot centred on `(x, y)`.
    pub fn dot(&mut self, x: i32, y: i32, size: i32, color: u32) {
        let half = size / 2;
        let (x0, y0) = ((x - half).max(self.x), (y - half).max(self.y));
        let (x1, y1) = ((x - half + size).min(self.x + self.w), (y - half + size).min(self.y + self.h));
        if x1 > x0 && y1 > y0 {
            self.fb.fill_rect(x0 as usize, y0 as usize, (x1 - x0) as usize, (y1 - y0) as usize, color);
        }
    }

    /// An anti-aliased line (Wu's algorithm, 24.8 fixed point).
    pub fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, color: u32) {
        let limit = 20000.0;
        if !(x0.abs() < limit && y0.abs() < limit && x1.abs() < limit && y1.abs() < limit) {
            return;
        }
        let (mut ax, mut ay) = ((x0 * 256.0) as i64, (y0 * 256.0) as i64);
        let (mut bx, mut by) = ((x1 * 256.0) as i64, (y1 * 256.0) as i64);
        let steep = (by - ay).abs() > (bx - ax).abs();
        if steep {
            core::mem::swap(&mut ax, &mut ay);
            core::mem::swap(&mut bx, &mut by);
        }
        if ax > bx {
            core::mem::swap(&mut ax, &mut bx);
            core::mem::swap(&mut ay, &mut by);
        }
        let dx = bx - ax;
        let dy = by - ay;
        let gradient = if dx == 0 { 256 } else { dy * 256 / dx };

        let put = |cv: &mut Self, x: i64, y: i64, a: i64| {
            let a = a.clamp(0, 255) as u32;
            if steep {
                cv.plot(y, x, color, a);
            } else {
                cv.plot(x, y, color, a);
            }
        };

        // first end point
        let xend = (ax + 128) & !255;
        let yend = ay + gradient * (xend - ax) / 256;
        let xgap = 255 - ((ax + 128) & 255);
        let (xp1, yp1) = (xend >> 8, yend >> 8);
        put(self, xp1, yp1, (255 - (yend & 255)) * xgap / 255);
        put(self, xp1, yp1 + 1, (yend & 255) * xgap / 255);
        let mut inter = yend + gradient;

        // second end point
        let xend2 = (bx + 128) & !255;
        let yend2 = by + gradient * (xend2 - bx) / 256;
        let xgap2 = (bx + 128) & 255;
        let (xp2, yp2) = (xend2 >> 8, yend2 >> 8);
        put(self, xp2, yp2, (255 - (yend2 & 255)) * xgap2 / 255);
        put(self, xp2, yp2 + 1, (yend2 & 255) * xgap2 / 255);

        for x in xp1 + 1..xp2 {
            put(self, x, inter >> 8, 255 - (inter & 255));
            put(self, x, (inter >> 8) + 1, inter & 255);
            inter += gradient;
        }
    }

    /// A filled triangle, one horizontal span at a time.
    pub fn tri(&mut self, p: [(i32, i32); 3], color: u32) {
        let mut v = p;
        v.sort_by_key(|q| q.1);
        let [(x0, y0), (x1, y1), (x2, y2)] = [
            (v[0].0 as i64, v[0].1 as i64),
            (v[1].0 as i64, v[1].1 as i64),
            (v[2].0 as i64, v[2].1 as i64),
        ];
        if y0 == y2 || x0.abs() > 20000 || x1.abs() > 20000 || x2.abs() > 20000 {
            return;
        }
        let top = y0.max(self.y as i64);
        let bottom = y2.min((self.y + self.h - 1) as i64);
        for y in top..=bottom {
            let xa = x0 + (x2 - x0) * (y - y0) / (y2 - y0);
            let xb = if y < y1 {
                if y1 == y0 { x1 } else { x0 + (x1 - x0) * (y - y0) / (y1 - y0) }
            } else if y2 == y1 {
                x1
            } else {
                x1 + (x2 - x1) * (y - y1) / (y2 - y1)
            };
            let left = xa.min(xb).max(self.x as i64);
            let right = xa.max(xb).min((self.x + self.w - 1) as i64);
            if right >= left {
                self.fb.fill_rect(left as usize, y as usize, (right - left + 1) as usize, 1, color);
            }
        }
    }
}

// ---------------------------------------------------------------- camera and meshes

pub struct Camera {
    view_proj: Mat4,
    eye: Vec3,
    cx: f32,
    cy: f32,
    hw: f32,
    hh: f32,
}

impl Camera {
    /// A camera looking at the origin from `distance`, a little above it.
    pub fn new(cv: &Canvas, distance: f32, height: f32) -> Camera {
        let (w, h) = (cv.w as f32, cv.h as f32);
        let eye = Vec3::new(0.0, height, distance);
        let proj = Mat4::perspective_rh(0.9, w / h, 0.1, 100.0);
        let view = Mat4::look_at_rh(eye, Vec3::ZERO, Vec3::Y);
        Camera {
            view_proj: proj * view,
            eye,
            cx: cv.x as f32 + w / 2.0,
            cy: cv.y as f32 + h / 2.0,
            hw: w / 2.0,
            hh: h / 2.0,
        }
    }

    /// Screen position and depth of a world point, if it is in front of the camera.
    pub fn project(&self, v: Vec3) -> Option<(f32, f32, f32)> {
        let p = self.view_proj * Vec4::new(v.x, v.y, v.z, 1.0);
        if p.w < 0.05 {
            return None;
        }
        let inv = 1.0 / p.w;
        Some((self.cx + p.x * inv * self.hw, self.cy - p.y * inv * self.hh, p.w))
    }
}

pub struct Mesh {
    verts: Vec<Vec3>,
    tris: Vec<[usize; 3]>,
    edges: Vec<(usize, usize)>,
}

/// Orders a triangle's corners so it faces away from `center`.
fn outward(verts: &[Vec3], t: [usize; 3], center: Vec3) -> [usize; 3] {
    let (a, b, c) = (verts[t[0]], verts[t[1]], verts[t[2]]);
    let n = (b - a).cross(c - a);
    let mid = (a + b + c) / 3.0;
    if n.dot(mid - center) < 0.0 {
        [t[0], t[2], t[1]]
    } else {
        t
    }
}

fn cube(size: f32) -> Mesh {
    let s = size;
    let verts = vec![
        Vec3::new(-s, -s, -s), Vec3::new(s, -s, -s), Vec3::new(s, s, -s), Vec3::new(-s, s, -s),
        Vec3::new(-s, -s, s), Vec3::new(s, -s, s), Vec3::new(s, s, s), Vec3::new(-s, s, s),
    ];
    let quads = [[0, 1, 2, 3], [4, 5, 6, 7], [0, 1, 5, 4], [3, 2, 6, 7], [0, 3, 7, 4], [1, 2, 6, 5]];
    let mut tris = Vec::new();
    for q in quads {
        tris.push(outward(&verts, [q[0], q[1], q[2]], Vec3::ZERO));
        tris.push(outward(&verts, [q[0], q[2], q[3]], Vec3::ZERO));
    }
    let edges = vec![(0, 1), (1, 2), (2, 3), (3, 0), (4, 5), (5, 6), (6, 7), (7, 4), (0, 4), (1, 5), (2, 6), (3, 7)];
    Mesh { verts, tris, edges }
}

fn torus(major: f32, minor: f32, segs: usize, sides: usize) -> Mesh {
    let mut verts = Vec::with_capacity(segs * sides);
    for i in 0..segs {
        let u = TAU * i as f32 / segs as f32;
        for j in 0..sides {
            let v = TAU * j as f32 / sides as f32;
            let ring = major + minor * cosf(v);
            verts.push(Vec3::new(ring * cosf(u), minor * sinf(v), ring * sinf(u)));
        }
    }
    let at = |i: usize, j: usize| (i % segs) * sides + (j % sides);
    let (mut tris, mut edges) = (Vec::new(), Vec::new());
    for i in 0..segs {
        for j in 0..sides {
            edges.push((at(i, j), at(i + 1, j)));
            edges.push((at(i, j), at(i, j + 1)));
            for t in [[at(i, j), at(i + 1, j), at(i + 1, j + 1)], [at(i, j), at(i + 1, j + 1), at(i, j + 1)]] {
                // the middle of the tube nearest this triangle
                let mid = (verts[t[0]] + verts[t[1]] + verts[t[2]]) / 3.0;
                let flat = Vec3::new(mid.x, 0.0, mid.z);
                let centre = flat / flat.length().max(0.0001) * major;
                tris.push(outward(&verts, t, centre));
            }
        }
    }
    Mesh { verts, tris, edges }
}

fn sphere(radius: f32, lat: usize, lon: usize) -> Mesh {
    let mut verts = Vec::new();
    for k in 0..=lat {
        let theta = TAU / 2.0 * k as f32 / lat as f32;
        for m in 0..lon {
            let phi = TAU * m as f32 / lon as f32;
            verts.push(Vec3::new(radius * sinf(theta) * cosf(phi), radius * cosf(theta), radius * sinf(theta) * sinf(phi)));
        }
    }
    let at = |k: usize, m: usize| k * lon + (m % lon);
    let (mut tris, mut edges) = (Vec::new(), Vec::new());
    for k in 0..=lat {
        for m in 0..lon {
            if k < lat {
                edges.push((at(k, m), at(k + 1, m)));
                for t in [[at(k, m), at(k + 1, m), at(k + 1, m + 1)], [at(k, m), at(k + 1, m + 1), at(k, m + 1)]] {
                    tris.push(outward(&verts, t, Vec3::ZERO));
                }
            }
            edges.push((at(k, m), at(k, m + 1)));
        }
    }
    Mesh { verts, tris, edges }
}

fn shade(color: u32, k: f32) -> u32 {
    let k = k.clamp(0.0, 1.0);
    let ch = |shift: u32| ((((color >> shift) & 255) as f32 * k) as u32).min(255) << shift;
    ch(16) | ch(8) | ch(0)
}

/// Fades a color towards the background with distance.
fn fade(color: u32, depth: f32, near: f32, far: f32) -> u32 {
    let t = ((depth - near) / (far - near)).clamp(0.0, 1.0);
    lerp(color, BG, (t * 190.0) as u32)
}

/// Draws a mesh as anti-aliased wire lines.
fn draw_wire(cv: &mut Canvas, cam: &Camera, mesh: &Mesh, model: &Mat4, color: u32, near: f32, far: f32) {
    let pts: Vec<Option<(f32, f32, f32)>> = mesh.verts.iter().map(|&v| cam.project(model.transform_point3(v))).collect();
    for &(a, b) in &mesh.edges {
        if let (Some(pa), Some(pb)) = (pts[a], pts[b]) {
            let c = fade(color, (pa.2 + pb.2) / 2.0, near, far);
            cv.line(pa.0, pa.1, pb.0, pb.1, c);
        }
    }
}

/// Draws a mesh as flat-shaded triangles, far ones first.
fn draw_solid(cv: &mut Canvas, cam: &Camera, mesh: &Mesh, model: &Mat4, base: u32, light: Vec3, outline: bool) {
    let world: Vec<Vec3> = mesh.verts.iter().map(|&v| model.transform_point3(v)).collect();
    let pts: Vec<Option<(f32, f32, f32)>> = world.iter().map(|&v| cam.project(v)).collect();
    let mut faces: Vec<(i32, [(i32, i32); 3], u32)> = Vec::new();
    for t in &mesh.tris {
        let (a, b, c) = (world[t[0]], world[t[1]], world[t[2]]);
        let n = (b - a).cross(c - a);
        let len = n.length();
        if len < 1e-6 {
            continue;
        }
        let n = n / len;
        let mid = (a + b + c) / 3.0;
        if n.dot(cam.eye - mid) <= 0.0 {
            continue; // facing away
        }
        if let (Some(pa), Some(pb), Some(pc)) = (pts[t[0]], pts[t[1]], pts[t[2]]) {
            let k = 0.3 + 0.7 * n.dot(light).max(0.0);
            let depth = ((pa.2 + pb.2 + pc.2) * 100.0) as i32;
            faces.push((depth, [(pa.0 as i32, pa.1 as i32), (pb.0 as i32, pb.1 as i32), (pc.0 as i32, pc.1 as i32)], shade(base, k)));
        }
    }
    faces.sort_unstable_by(|a, b| b.0.cmp(&a.0));
    for (_, p, color) in faces {
        cv.tri(p, color);
        // a thin line round the edge hides the jagged border of the fill
        if outline {
            for (i, j) in [(0, 1), (1, 2), (2, 0)] {
                cv.line(p[i].0 as f32, p[i].1 as f32, p[j].0 as f32, p[j].1 as f32, color);
            }
        }
    }
}

// ---------------------------------------------------------------- demos

/// Everything a demo gets to work with each frame.
pub struct Ctx<'a, 'b> {
    pub cv: Canvas<'a>,
    /// Animation time in seconds (scaled by the speed setting).
    pub t: f32,
    /// Seconds since the previous frame (scaled by the speed setting).
    pub dt: f32,
    pub zoom: f32,
    pub st: &'b mut State,
}

/// Things a demo wants to remember between frames.
pub struct State {
    rng: u32,
    stars: Vec<[f32; 3]>,
    trail: Vec<Vec3>,
    point: Vec3,
    meshes: Vec<Mesh>,
}

impl State {
    fn new() -> State {
        State { rng: 0x2545_F491 ^ crate::time::ticks() as u32, stars: Vec::new(), trail: Vec::new(), point: Vec3::new(0.1, 0.0, 0.0), meshes: Vec::new() }
    }

    /// A random number between 0 and 1.
    fn rand(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / 16_777_216.0
    }
}

pub struct Demo {
    pub name: &'static str,
    pub about: &'static str,
    run: fn(&mut Ctx),
}

pub static DEMOS: &[Demo] = &[
    Demo { name: "cube", about: "two wireframe cubes spinning against each other", run: demo_cube },
    Demo { name: "solid", about: "a flat-shaded cube lit from one side", run: demo_solid },
    Demo { name: "torus", about: "a wireframe torus, fading with distance", run: demo_torus },
    Demo { name: "donut", about: "a shaded solid torus", run: demo_donut },
    Demo { name: "sphere", about: "a wireframe globe with a shaded moon", run: demo_sphere },
    Demo { name: "helix", about: "a DNA double helix", run: demo_helix },
    Demo { name: "starfield", about: "flying through stars", run: demo_starfield },
    Demo { name: "terrain", about: "a wireframe landscape flythrough", run: demo_terrain },
    Demo { name: "lorenz", about: "the Lorenz strange attractor", run: demo_lorenz },
];

fn spin(t: f32, speed: f32) -> Mat4 {
    Mat4::from_rotation_y(t * speed) * Mat4::from_rotation_x(t * speed * 0.6)
}

fn demo_cube(cx: &mut Ctx) {
    let cam = Camera::new(&cx.cv, 7.5 / cx.zoom, 1.6);
    if cx.st.meshes.is_empty() {
        cx.st.meshes.push(cube(1.0));
        cx.st.meshes.push(cube(0.55));
    }
    let big = spin(cx.t, 0.8);
    let small = spin(cx.t, -1.3);
    draw_wire(&mut cx.cv, &cam, &cx.st.meshes[0], &big, ui::ACCENT, 3.0, 8.0);
    draw_wire(&mut cx.cv, &cam, &cx.st.meshes[1], &small, ui::PURPLE, 3.0, 8.0);
    for &v in &cx.st.meshes[0].verts {
        if let Some(p) = cam.project(big.transform_point3(v)) {
            cx.cv.dot(p.0 as i32, p.1 as i32, 6, 0xDCE0E5);
        }
    }
}

fn demo_solid(cx: &mut Ctx) {
    let cam = Camera::new(&cx.cv, 7.0 / cx.zoom, 1.8);
    if cx.st.meshes.is_empty() {
        cx.st.meshes.push(cube(1.0));
    }
    let model = spin(cx.t, 0.7);
    let light = Vec3::new(0.5, 0.8, 0.6).normalize();
    draw_solid(&mut cx.cv, &cam, &cx.st.meshes[0], &model, ui::ACCENT, light, true);
}

fn demo_torus(cx: &mut Ctx) {
    let cam = Camera::new(&cx.cv, 4.6 / cx.zoom, 1.6);
    if cx.st.meshes.is_empty() {
        cx.st.meshes.push(torus(1.2, 0.45, 32, 14));
    }
    let model = Mat4::from_rotation_x(0.9 + sinf(cx.t * 0.4) * 0.3) * Mat4::from_rotation_y(cx.t * 0.7);
    draw_wire(&mut cx.cv, &cam, &cx.st.meshes[0], &model, ui::CYAN, 3.0, 6.5);
}

fn demo_donut(cx: &mut Ctx) {
    let cam = Camera::new(&cx.cv, 4.6 / cx.zoom, 1.0);
    if cx.st.meshes.is_empty() {
        cx.st.meshes.push(torus(1.2, 0.5, 36, 18));
    }
    // tilted so the hole shows, spinning about its own axis, lit by a light that orbits
    let model = Mat4::from_rotation_x(0.95 + 0.35 * sinf(cx.t * 0.6)) * Mat4::from_rotation_z(0.4 * sinf(cx.t * 0.4)) * Mat4::from_rotation_y(cx.t * 0.8);
    let light = Vec3::new(sinf(cx.t * 0.9) * 0.9, 0.6, 0.9).normalize();
    draw_solid(&mut cx.cv, &cam, &cx.st.meshes[0], &model, ui::ORANGE, light, false);
}

fn demo_sphere(cx: &mut Ctx) {
    let cam = Camera::new(&cx.cv, 5.5 / cx.zoom, 1.0);
    if cx.st.meshes.is_empty() {
        cx.st.meshes.push(sphere(1.4, 14, 28));
        cx.st.meshes.push(sphere(0.35, 10, 16));
    }
    let tilt = Mat4::from_rotation_z(0.4) * Mat4::from_rotation_y(cx.t * 0.5);
    draw_wire(&mut cx.cv, &cam, &cx.st.meshes[0], &tilt, ui::GREEN, 3.5, 7.5);
    // a moon on a tilted orbit
    let orbit = Mat4::from_rotation_z(0.4) * Mat4::from_rotation_y(cx.t * 1.1) * Mat4::from_translation(Vec3::new(2.5, 0.0, 0.0));
    let light = Vec3::new(0.3, 0.6, 0.8).normalize();
    draw_solid(&mut cx.cv, &cam, &cx.st.meshes[1], &orbit, ui::YELLOW, light, false);
}

fn demo_helix(cx: &mut Ctx) {
    let cam = Camera::new(&cx.cv, 6.0 / cx.zoom, 0.5);
    let model = Mat4::from_rotation_y(cx.t * 0.4) * Mat4::from_rotation_z(0.25);
    let mut prev: Option<[(f32, f32, f32); 2]> = None;
    for i in 0..60 {
        let y = -2.4 + i as f32 * 0.08;
        let a = i as f32 * 0.3 + cx.t * 1.5;
        let p1 = cam.project(model.transform_point3(Vec3::new(0.8 * cosf(a), y, 0.8 * sinf(a))));
        let p2 = cam.project(model.transform_point3(Vec3::new(-0.8 * cosf(a), y, -0.8 * sinf(a))));
        if let (Some(p1), Some(p2)) = (p1, p2) {
            if i % 3 == 0 {
                cx.cv.line(p1.0, p1.1, p2.0, p2.1, ui::TEXT_FAINT);
            }
            if let Some(prev) = prev {
                cx.cv.line(prev[0].0, prev[0].1, p1.0, p1.1, fade(ui::ACCENT, p1.2, 4.5, 7.5));
                cx.cv.line(prev[1].0, prev[1].1, p2.0, p2.1, fade(ui::ORANGE, p2.2, 4.5, 7.5));
            }
            cx.cv.dot(p1.0 as i32, p1.1 as i32, 7, fade(ui::ACCENT, p1.2, 4.5, 7.5));
            cx.cv.dot(p2.0 as i32, p2.1 as i32, 7, fade(ui::ORANGE, p2.2, 4.5, 7.5));
            prev = Some([p1, p2]);
        } else {
            prev = None;
        }
    }
}

fn demo_starfield(cx: &mut Ctx) {
    if cx.st.stars.is_empty() {
        for _ in 0..600 {
            let s = [cx.st.rand() * 2.0 - 1.0, cx.st.rand() * 2.0 - 1.0, 0.05 + cx.st.rand() * 0.95];
            cx.st.stars.push(s);
        }
    }
    let (w, h) = (cx.cv.w as f32, cx.cv.h as f32);
    let (mid_x, mid_y) = (cx.cv.x as f32 + w / 2.0, cx.cv.y as f32 + h / 2.0);
    let scale = h.max(w * 0.5) * 0.5 * cx.zoom;
    let dz = cx.dt * 0.6;
    for i in 0..cx.st.stars.len() {
        let [x, y, z] = cx.st.stars[i];
        let nz = z - dz;
        if nz < 0.03 {
            cx.st.stars[i] = [cx.st.rand() * 2.0 - 1.0, cx.st.rand() * 2.0 - 1.0, 1.0];
            continue;
        }
        cx.st.stars[i][2] = nz;
        let back = nz + dz * 3.0;
        let (x0, y0) = (mid_x + x / back * scale * 0.5, mid_y + y / back * scale * 0.5);
        let (x1, y1) = (mid_x + x / nz * scale * 0.5, mid_y + y / nz * scale * 0.5);
        let bright = ((1.0 - nz) * 255.0) as u32;
        let color = lerp(BG, 0xF1F3F6, bright);
        cx.cv.line(x0, y0, x1, y1, color);
        if nz < 0.25 {
            cx.cv.dot(x1 as i32, y1 as i32, 3, color);
        }
    }
}

fn demo_terrain(cx: &mut Ctx) {
    const N: usize = 30;
    let cam = {
        let (w, h) = (cx.cv.w as f32, cx.cv.h as f32);
        let eye = Vec3::new(0.0, 1.5, 2.4 / cx.zoom);
        let proj = Mat4::perspective_rh(1.0, w / h, 0.1, 100.0);
        let view = Mat4::look_at_rh(eye, Vec3::new(0.0, 0.2, -3.0), Vec3::Y);
        Camera { view_proj: proj * view, eye, cx: cx.cv.x as f32 + w / 2.0, cy: cx.cv.y as f32 + h / 2.0, hw: w / 2.0, hh: h / 2.0 }
    };
    let fly = cx.t * 0.9;
    let spacing = 0.45;
    let height = |x: f32, z: f32| {
        0.45 * sinf(x * 1.3 + z * 0.6) + 0.3 * cosf(z * 1.1 - x * 0.5) + 0.12 * sinf(x * 2.3 + z * 1.9)
    };
    let mut grid: Vec<Option<(f32, f32, f32, f32)>> = Vec::with_capacity(N * N);
    for gz in 0..N {
        for gx in 0..N {
            let x = (gx as f32 - N as f32 / 2.0) * spacing;
            let z = -(gz as f32) * spacing;
            let y = height(x, z - fly * 1.0);
            grid.push(cam.project(Vec3::new(x, y, z)).map(|p| (p.0, p.1, p.2, y)));
        }
    }
    let color_of = |a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)| {
        let y = (a.3 + b.3) / 2.0;
        let k = ((y + 0.6) / 1.4).clamp(0.0, 1.0);
        let base = lerp(ui::ACCENT, ui::GREEN, (k * 255.0) as u32);
        let base = lerp(base, ui::YELLOW, ((k - 0.6).max(0.0) * 640.0) as u32);
        fade(base, (a.2 + b.2) / 2.0, 1.5, 9.0)
    };
    for gz in 0..N {
        for gx in 0..N {
            let here = grid[gz * N + gx];
            if gx + 1 < N {
                if let (Some(a), Some(b)) = (here, grid[gz * N + gx + 1]) {
                    cx.cv.line(a.0, a.1, b.0, b.1, color_of(a, b));
                }
            }
            if gz + 1 < N {
                if let (Some(a), Some(b)) = (here, grid[(gz + 1) * N + gx]) {
                    cx.cv.line(a.0, a.1, b.0, b.1, color_of(a, b));
                }
            }
        }
    }
}

fn demo_lorenz(cx: &mut Ctx) {
    const KEEP: usize = 1400;
    // advance the system a few steps per frame
    for _ in 0..6 {
        let p = cx.st.point;
        let (dx, dy, dz) = (10.0 * (p.y - p.x), p.x * (28.0 - p.z) - p.y, p.x * p.y - 8.0 / 3.0 * p.z);
        cx.st.point = Vec3::new(p.x + dx * 0.006, p.y + dy * 0.006, p.z + dz * 0.006);
        let centred = Vec3::new(cx.st.point.x, cx.st.point.y, cx.st.point.z - 25.0) * 0.075;
        cx.st.trail.push(centred);
    }
    let excess = cx.st.trail.len().saturating_sub(KEEP);
    if excess > 0 {
        cx.st.trail.drain(..excess);
    }
    let cam = Camera::new(&cx.cv, 4.2 / cx.zoom, 0.0);
    let model = Mat4::from_rotation_y(cx.t * 0.35) * Mat4::from_rotation_x(0.25);
    let pts: Vec<Option<(f32, f32, f32)>> = cx.st.trail.iter().map(|&v| cam.project(model.transform_point3(v))).collect();
    let n = pts.len().max(1);
    for i in 1..pts.len() {
        if let (Some(a), Some(b)) = (pts[i - 1], pts[i]) {
            let age = (i * 255 / n) as u32;
            let color = lerp(BG, lerp(ui::PURPLE, ui::YELLOW, age), 60 + age * 195 / 255);
            cx.cv.line(a.0, a.1, b.0, b.1, color);
        }
    }
}

// ---------------------------------------------------------------- the driver

enum Exit {
    Quit,
    Next,
    Previous,
}

/// Looks a demo up by name.
pub fn find(name: &str) -> Option<usize> {
    DEMOS.iter().position(|d| d.name == name)
}

/// Runs demos starting at `start`, until the user quits. `n` / right arrow
/// goes to the next demo, `p` / left arrow to the previous one.
pub fn run(start: usize) {
    let mut index = start % DEMOS.len();
    loop {
        match run_one(index) {
            Exit::Quit => break,
            Exit::Next => index = (index + 1) % DEMOS.len(),
            Exit::Previous => index = (index + DEMOS.len() - 1) % DEMOS.len(),
        }
    }
    ui::restore_shell_screen();
}

fn run_one(index: usize) -> Exit {
    let demo = &DEMOS[index];
    let mut st = State::new();
    let (mut t, mut speed, mut zoom, mut paused) = (0.0f32, 1.0f32, 1.0f32, false);
    let mut last_ms = crate::time::uptime_ms();
    let (mut fps_ms, mut frames, mut fps) = (last_ms, 0u64, 0u64);
    let mut last_tick = crate::time::ticks();

    // window chrome
    framebuffer::with(|fb| {
        let lay = ui::layout(fb.width(), fb.height());
        fb.clear_to(BG);
        ui::draw_title(fb, &lay, &format!("3d / {}", demo.name), demo.about);
    });

    loop {
        let now = crate::time::uptime_ms();
        let dt = (now - last_ms) as f32 / 1000.0;
        last_ms = now;
        if !paused {
            t += dt * speed;
        }

        framebuffer::with(|fb| {
            let (w, h) = (fb.width(), fb.height());
            let lay = ui::layout(w, h);
            fb.fill_rect(0, lay.body_y(), w, lay.body_h(), BG);
            let cv = Canvas { fb, x: 0, y: lay.body_y() as i32, w: w as i32, h: lay.body_h() as i32 };
            let mut cx = Ctx { cv, t, dt: if paused { 0.0 } else { dt * speed }, zoom, st: &mut st };
            (demo.run)(&mut cx);
            let fb = cx.cv.fb;
            let left = format!("{} ({}/{}){}", demo.name, index + 1, DEMOS.len(), if paused { "  paused" } else { "" });
            let right = format!("{} fps  |  arrows: zoom / switch   [ ]: speed   space: pause   q: quit", fps);
            ui::draw_status(fb, &lay, &left, &right);
            fb.flush();
        });

        frames += 1;
        if now - fps_ms >= 1000 {
            fps = frames * 1000 / (now - fps_ms);
            frames = 0;
            fps_ms = now;
        }

        while let Some(key) = input::try_read_key() {
            match key {
                Key::Char('q') | Key::Char('\u{1b}') | Key::Char('\u{3}') => return Exit::Quit,
                Key::Char('n') | Key::Right | Key::Char('\t') => return Exit::Next,
                Key::Char('p') | Key::Left => return Exit::Previous,
                Key::Up | Key::Char('+') | Key::Char('=') => zoom = (zoom * 1.15).min(4.0),
                Key::Down | Key::Char('-') => zoom = (zoom / 1.15).max(0.3),
                Key::Char(']') => speed = (speed * 1.25).min(6.0),
                Key::Char('[') => speed = (speed / 1.25).max(0.1),
                Key::Char(' ') => paused = !paused,
                _ => {}
            }
        }

        // at most one frame per timer tick (100 Hz); don't spin
        while crate::time::ticks() == last_tick {
            crate::sched::yield_idle();
        }
        last_tick = crate::time::ticks();
    }
}
