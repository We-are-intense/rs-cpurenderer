use crate::camera;
use crate::image::*;
use crate::math;
use crate::scanline::*;

struct Viewport {
    x: i32,
    y: i32,
    w: u32,
    h: u32,
}

pub struct Renderer {
    color_attachment: ColorAttachment,
    camera: camera::Camera,
    viewport: Viewport,
}

impl Renderer {
    pub fn new(w: u32, h: u32, camera: camera::Camera) -> Self {
        Self {
            color_attachment: ColorAttachment::new(w, h),
            camera,
            viewport: Viewport { x: 0, y: 0, w, h },
        }
    }

    pub fn clear(&mut self, color: &math::Vec4) {
        self.color_attachment.clear(color);
    }

    pub fn get_canva_width(&self) -> u32 {
        self.color_attachment.width()
    }

    pub fn get_canva_height(&self) -> u32 {
        self.color_attachment.height()
    }

    pub fn get_rendered_image(&self) -> &[u8] {
        self.color_attachment.data()
    }

    pub fn draw_triangle(
        &mut self,
        model: &math::Mat4,
        vertices: &[math::Vec3; 3],
        color: &math::Vec4,
    ) {
        // 1. convert 3D coordination to Homogeneous coordinates
        let mut vertices = vertices.map(|v| math::Vec4::from_vec3(&v, 1.0));

        // 2. MVP transform
        for v in &mut vertices {
            *v = *self.camera.get_frustum().get_mat() * *model * *v;
            *v /= v.w;
        }

        // 3. Viewport transform
        let vertices = vertices.map(|v| {
            math::Vec2::new(
                (v.x + 1.0) * 0.5 * (self.viewport.w as f32 - 1.0) + self.viewport.x as f32,
                self.viewport.h as f32 - (v.y + 1.0) * 0.5 * (self.viewport.h as f32 - 1.0)
                    + self.viewport.y as f32,
            )
        });


        // 4. split triangle into trapeziods
        let [trap1, trap2] = &mut Trapezoid::from_triangle(&vertices);

        // 6. rasterization trapeziods
        if let Some(trap) = trap1 {
            self.draw_trapezoid(trap, color);
        }
        if let Some(trap) = trap2 {
            self.draw_trapezoid(trap, color);
        }


        for i in 0..vertices.len() {
            let p1 = &vertices[i];
            let p2 = &vertices[(i + 1) % vertices.len()];

            self.draw_line(p1, p2, color);
        }
    }

    fn draw_trapezoid(&mut self, trap: &Trapezoid, color: &math::Vec4) {
        let top = (trap.top.ceil().max(0.0)) as i32;
        let bottom =
            (trap.bottom.ceil()).min(self.color_attachment.height() as f32 - 1.0) as i32 - 1;
        let mut y = top as f32;

        while y <= bottom as f32 {
            let mut scanline = Scanline::from_trapezoid(&trap, y);
            self.draw_scanline(&mut scanline, color);
            y += 1.0;
        }
    }

    fn draw_scanline(&mut self, scanline: &mut Scanline, color: &math::Vec4) {
        let vertex = &mut scanline.vertex;
        let y = scanline.y as u32;
        while scanline.width > 0.0 {
            let x = vertex.x;

            if x >= 0.0 && x < self.color_attachment.width() as f32 {
                let x = x as u32;
                self.color_attachment.set(x, y, &color)
            }

            scanline.width -= 1.0;
            *vertex += scanline.step;
        }
    }

    pub fn draw_line(&mut self, p1: &math::Vec2, p2: &math::Vec2, color: &math::Vec4) {
        let clip_result = cohen_sutherland::cohen_sutherland_line_clip(
            p1,
            p2,
            &math::Vec2::zero(),
            &math::Vec2::new(
                self.color_attachment.width() as f32 - 1.0,
                self.color_attachment.height() as f32 - 1.0,
            ),
        );

        if let Some((p1, p2)) = clip_result {
            self.draw_line_without_clip(p1.x as i32, p1.y as i32, p2.x as i32, p2.y as i32, color);
        }
    }

    fn draw_line_without_clip(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: &math::Vec4) {
        let mut dx = (x1 - x0).abs();
        let mut dy = (y1 - y0).abs();
        let mut sx = if x1 >= x0 { 1 } else { -1 };
        let mut sy = if y1 >= y0 { 1 } else { -1 };
        let mut x = x0;
        let mut y = y0;
        let steep = if dx < dy { 1 } else { -1 };

        let final_x = if dx < dy { y1 } else { x1 };

        if dx < dy {
            std::mem::swap(&mut dx, &mut dy);
            std::mem::swap(&mut x, &mut y);
            std::mem::swap(&mut sx, &mut sy);
        }

        let mut e = -dx;
        let step = 2 * dy;
        let desc = -2 * dx;

        while x != final_x {
            if steep > 0 {
                self.color_attachment.set(y as u32, x as u32, color);
            } else {
                self.color_attachment.set(x as u32, y as u32, color);
            }

            e += step;
            if e >= 0 {
                y += sy;
                e += desc;
            }
            x += sx;
        }
    }
}

/// [Cohen-Sutherland Algorithm](https://en.wikipedia.org/wiki/Cohen%E2%80%93Sutherland_algorithm)
mod cohen_sutherland {
    use super::math;

    const INSIDE: u8 = 0;
    const LEFT: u8 = 1;
    const RIGHT: u8 = 2;
    const BOTTOM: u8 = 4;
    const TOP: u8 = 8;

    fn compute_outcode(p: &math::Vec2, min: &math::Vec2, max: &math::Vec2) -> u8 {
        (if p.x < min.x {
            LEFT
        } else if p.x > max.x {
            RIGHT
        } else {
            INSIDE
        } | if p.y < min.y {
            BOTTOM
        } else if p.y > max.y {
            TOP
        } else {
            INSIDE
        })
    }

    pub fn cohen_sutherland_line_clip(
        p1: &math::Vec2,
        p2: &math::Vec2,
        rect_min: &math::Vec2,
        rect_max: &math::Vec2,
    ) -> Option<(math::Vec2, math::Vec2)> {
        let mut pt1 = *p1;
        let mut pt2 = *p2;

        let mut outcode1 = compute_outcode(&pt1, rect_min, rect_max);
        let mut outcode2 = compute_outcode(&pt2, rect_min, rect_max);

        loop {
            if outcode1 & outcode2 != 0 {
                return None;
            } else if outcode1 | outcode2 == 0 {
                return Some((pt1, pt2));
            }

            let mut p = math::Vec2::zero();

            let outcode = if outcode2 > outcode1 {
                outcode2
            } else {
                outcode1
            };

            if outcode & TOP != 0 {
                p.x = p1.x + (pt2.x - pt1.x) * (rect_max.y - pt1.y) / (pt2.y - pt1.y);
                p.y = rect_max.y;
            } else if outcode & BOTTOM != 0 {
                p.x = p1.x + (pt2.x - pt1.x) * (rect_min.y - pt1.y) / (pt2.y - pt1.y);
                p.y = rect_min.y;
            } else if outcode & RIGHT != 0 {
                p.y = pt1.y + (pt2.y - pt1.y) * (rect_max.x - pt1.x) / (pt2.x - pt1.x);
                p.x = rect_max.x;
            } else if outcode & LEFT != 0 {
                p.y = pt1.y + (pt2.y - pt1.y) * (rect_min.x - pt1.x) / (pt2.x - pt1.x);
                p.x = rect_min.x;
            }

            if outcode == outcode1 {
                pt1 = p;
                outcode1 = compute_outcode(&pt1, rect_min, rect_max);
            } else {
                pt2 = p;
                outcode2 = compute_outcode(&pt2, rect_min, rect_max);
            }
        }
    }
}
