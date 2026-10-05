//! Minimal anti-aliased drawing on an RGB canvas: shapes and text.

use ab_glyph::{Font, FontRef, Glyph, PxScale, ScaleFont, point};
use image::RgbImage;
use std::sync::OnceLock;

pub type Color = [u8; 3];

pub fn regular() -> &'static FontRef<'static> {
    static F: OnceLock<FontRef<'static>> = OnceLock::new();
    F.get_or_init(|| {
        FontRef::try_from_slice(include_bytes!("../assets/Inter-Regular.ttf"))
            .expect("embedded font")
    })
}

pub fn bold() -> &'static FontRef<'static> {
    static F: OnceLock<FontRef<'static>> = OnceLock::new();
    F.get_or_init(|| {
        FontRef::try_from_slice(include_bytes!("../assets/Inter-Bold.ttf")).expect("embedded font")
    })
}

pub struct Canvas {
    pub img: RgbImage,
}

impl Canvas {
    pub fn new(w: u32, h: u32, bg: Color) -> Self {
        Canvas {
            img: RgbImage::from_pixel(w, h, image::Rgb(bg)),
        }
    }

    /// Blend `color` into a pixel with coverage `a` (0..1).
    pub fn blend(&mut self, x: i32, y: i32, color: Color, a: f32) {
        if a <= 0.0
            || x < 0
            || y < 0
            || x >= self.img.width() as i32
            || y >= self.img.height() as i32
        {
            return;
        }
        let a = a.min(1.0);
        let p = self.img.get_pixel_mut(x as u32, y as u32);
        for k in 0..3 {
            p.0[k] = (p.0[k] as f32 * (1.0 - a) + color[k] as f32 * a + 0.5) as u8;
        }
    }

    /// Run `coverage(x, y)` (pixel centre) over a bounding box.
    fn shade(
        &mut self,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        color: Color,
        coverage: impl Fn(f32, f32) -> f32,
    ) {
        for y in (y0.floor() as i32)..=(y1.ceil() as i32) {
            for x in (x0.floor() as i32)..=(x1.ceil() as i32) {
                let a = coverage(x as f32 + 0.5, y as f32 + 0.5);
                self.blend(x, y, color, a);
            }
        }
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Color) {
        self.shade(x, y, x + w, y + h, color, |px, py| {
            let cx = (px + 0.5 - x).clamp(0.0, 1.0) * (x + w - px + 0.5).clamp(0.0, 1.0);
            let cy = (py + 0.5 - y).clamp(0.0, 1.0) * (y + h - py + 0.5).clamp(0.0, 1.0);
            cx * cy
        });
    }

    /// Annulus between radii `r0` and `r1` (`r0 = 0` for a disk).
    pub fn ring(&mut self, cx: f32, cy: f32, r0: f32, r1: f32, color: Color) {
        self.shade(
            cx - r1 - 1.0,
            cy - r1 - 1.0,
            cx + r1 + 1.0,
            cy + r1 + 1.0,
            color,
            |x, y| {
                let d = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
                (r1 - d + 0.5).clamp(0.0, 1.0)
                    * if r0 > 0.0 {
                        (d - r0 + 0.5).clamp(0.0, 1.0)
                    } else {
                        1.0
                    }
            },
        );
    }

    /// Thick line with round caps.
    pub fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, width: f32, color: Color) {
        let r = width * 0.5;
        let (dx, dy) = (x1 - x0, y1 - y0);
        let len2 = (dx * dx + dy * dy).max(1e-6);
        self.shade(
            x0.min(x1) - r - 1.0,
            y0.min(y1) - r - 1.0,
            x0.max(x1) + r + 1.0,
            y0.max(y1) + r + 1.0,
            color,
            |x, y| {
                let t = (((x - x0) * dx + (y - y0) * dy) / len2).clamp(0.0, 1.0);
                let d = ((x - x0 - t * dx).powi(2) + (y - y0 - t * dy).powi(2)).sqrt();
                (r - d + 0.5).clamp(0.0, 1.0)
            },
        );
    }

    /// Tick mark in a box of `size` with top-left at (x, y).
    pub fn tick(&mut self, x: f32, y: f32, size: f32, width: f32, color: Color) {
        self.line(
            x + size * 0.12,
            y + size * 0.55,
            x + size * 0.4,
            y + size * 0.82,
            width,
            color,
        );
        self.line(
            x + size * 0.4,
            y + size * 0.82,
            x + size * 0.9,
            y + size * 0.18,
            width,
            color,
        );
    }

    /// Exclamation mark in a box of `size` with top-left at (x, y).
    pub fn bang(&mut self, x: f32, y: f32, size: f32, width: f32, color: Color) {
        let cx = x + size * 0.5;
        self.line(cx, y + size * 0.12, cx, y + size * 0.6, width, color);
        self.ring(cx, y + size * 0.86, 0.0, width * 0.6, color);
    }

    /// Draw text with its baseline-left at (x, y); returns the advance width.
    pub fn text(
        &mut self,
        font: &FontRef<'_>,
        size: f32,
        x: f32,
        y: f32,
        s: &str,
        color: Color,
    ) -> f32 {
        let scaled = font.as_scaled(PxScale::from(size));
        let mut caret = x;
        let mut prev = None;
        for ch in s.chars() {
            let id = scaled.glyph_id(ch);
            if let Some(p) = prev {
                caret += scaled.kern(p, id);
            }
            let g: Glyph = id.with_scale_and_position(size, point(caret, y));
            caret += scaled.h_advance(id);
            prev = Some(id);
            if let Some(o) = font.outline_glyph(g) {
                let b = o.px_bounds();
                o.draw(|gx, gy, a| {
                    self.blend(
                        b.min.x as i32 + gx as i32,
                        b.min.y as i32 + gy as i32,
                        color,
                        a,
                    )
                });
            }
        }
        caret - x
    }

    pub fn text_centered(
        &mut self,
        font: &FontRef<'_>,
        size: f32,
        cx: f32,
        y: f32,
        s: &str,
        color: Color,
    ) {
        let w = text_width(font, size, s);
        self.text(font, size, cx - w * 0.5, y, s, color);
    }

    /// Text centred on the top of a circle, reading clockwise, baseline on radius `r`.
    pub fn text_arc(
        &mut self,
        font: &FontRef<'_>,
        size: f32,
        cx: f32,
        cy: f32,
        r: f32,
        s: &str,
        color: Color,
    ) {
        let scaled = font.as_scaled(PxScale::from(size));
        let total = text_width(font, size, s);
        // Angle measured clockwise from 12 o'clock.
        let mut along = -total * 0.5;
        for ch in s.chars() {
            let id = scaled.glyph_id(ch);
            let adv = scaled.h_advance(id);
            let theta = (along + adv * 0.5) / r;
            along += adv;
            // Rasterise the glyph upright around its centre-bottom, then rotate.
            let g: Glyph = id.with_scale_and_position(size, point(0.0, 0.0));
            let Some(o) = font.outline_glyph(g) else {
                continue;
            };
            let b = o.px_bounds();
            let (gw, gh) = (b.width() as usize, b.height() as usize);
            let mut cov = vec![0.0f32; gw * gh];
            o.draw(|x, y, a| cov[y as usize * gw + x as usize] = a);
            // Glyph-local origin: (adv/2, 0) is the point on the circle.
            let (sin, cos) = theta.sin_cos();
            let ox = cx + r * sin;
            let oy = cy - r * cos;
            let reach = (gw.max(gh) as f32) * 1.5 + size;
            for py in (oy - reach) as i32..=(oy + reach) as i32 {
                for px in (ox - reach) as i32..=(ox + reach) as i32 {
                    let (dx, dy) = (px as f32 + 0.5 - ox, py as f32 + 0.5 - oy);
                    // Inverse rotation into glyph space.
                    let lx = dx * cos + dy * sin + adv * 0.5 - b.min.x;
                    let ly = -dx * sin + dy * cos - b.min.y;
                    let a = sample(&cov, gw, gh, lx, ly);
                    self.blend(px, py, color, a);
                }
            }
        }
    }
}

fn sample(buf: &[f32], w: usize, h: usize, x: f32, y: f32) -> f32 {
    let (x, y) = (x - 0.5, y - 0.5);
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let at = |x: i32, y: i32| {
        if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
            0.0
        } else {
            buf[y as usize * w + x as usize]
        }
    };
    let (x0, y0) = (x0 as i32, y0 as i32);
    let top = at(x0, y0) * (1.0 - fx) + at(x0 + 1, y0) * fx;
    let bot = at(x0, y0 + 1) * (1.0 - fx) + at(x0 + 1, y0 + 1) * fx;
    top * (1.0 - fy) + bot * fy
}

pub fn text_width(font: &FontRef<'_>, size: f32, s: &str) -> f32 {
    let scaled = font.as_scaled(PxScale::from(size));
    let mut w = 0.0;
    let mut prev = None;
    for ch in s.chars() {
        let id = scaled.glyph_id(ch);
        if let Some(p) = prev {
            w += scaled.kern(p, id);
        }
        w += scaled.h_advance(id);
        prev = Some(id);
    }
    w
}

/// Greedy word wrap to `max_w` pixels.
pub fn wrap(font: &FontRef<'_>, size: f32, s: &str, max_w: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in s.split_whitespace() {
        let candidate = if cur.is_empty() {
            word.to_string()
        } else {
            format!("{cur} {word}")
        };
        if text_width(font, size, &candidate) > max_w && !cur.is_empty() {
            lines.push(std::mem::take(&mut cur));
            cur = word.to_string();
        } else {
            cur = candidate;
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}
