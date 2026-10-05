//! Low-level image helpers: points, affine sampling, colour-space conversion.

use image::{Rgb32FImage, RgbImage};

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
    pub fn mid(self, o: Point) -> Point {
        Point::new((self.x + o.x) * 0.5, (self.y + o.y) * 0.5)
    }
    pub fn dist(self, o: Point) -> f32 {
        ((self.x - o.x).powi(2) + (self.y - o.y).powi(2)).sqrt()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn center(&self) -> Point {
        Point::new(self.x + self.w * 0.5, self.y + self.h * 0.5)
    }
    pub fn iou(&self, o: &Rect) -> f32 {
        let ix = (self.x + self.w).min(o.x + o.w) - self.x.max(o.x);
        let iy = (self.y + self.h).min(o.y + o.h) - self.y.max(o.y);
        if ix <= 0.0 || iy <= 0.0 {
            return 0.0;
        }
        let inter = ix * iy;
        inter / (self.w * self.h + o.w * o.h - inter)
    }
}

/// Affine map from destination pixel coordinates to source pixel coordinates:
/// `src = (a*x + b*y + c, d*x + e*y + f)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

impl Affine {
    /// Destination is a `dst_w`×`dst_h` box; its centre maps to `center` in the
    /// source, each destination pixel covers `scale` source pixels, and the box
    /// is rotated by `angle` radians (source-space angle of the dst x axis).
    pub fn crop(center: Point, dst_w: f32, dst_h: f32, scale: f32, angle: f32) -> Self {
        let (s, c) = angle.sin_cos();
        let (a, b, d, e) = (c * scale, -s * scale, s * scale, c * scale);
        let (hx, hy) = (dst_w * 0.5, dst_h * 0.5);
        Affine { a, b, c: center.x - a * hx - b * hy, d, e, f: center.y - d * hx - e * hy }
    }
    pub fn apply(&self, p: Point) -> Point {
        Point::new(self.a * p.x + self.b * p.y + self.c, self.d * p.x + self.e * p.y + self.f)
    }
    pub fn scale(&self) -> f32 {
        (self.a * self.e - self.b * self.d).abs().sqrt()
    }
    pub fn inverse(&self) -> Affine {
        let det = self.a * self.e - self.b * self.d;
        let (a, b, d, e) = (self.e / det, -self.b / det, -self.d / det, self.a / det);
        Affine { a, b, c: -(a * self.c + b * self.f), d, e, f: -(d * self.c + e * self.f) }
    }
}

/// Bilinear sample of a float image with edge clamping.
#[inline]
pub fn sample(src: &Rgb32FImage, x: f32, y: f32) -> [f32; 3] {
    let (w, h) = (src.width() as i32, src.height() as i32);
    let x = x - 0.5;
    let y = y - 0.5;
    let x0 = x.floor();
    let y0 = y.floor();
    let fx = x - x0;
    let fy = y - y0;
    let cx = |v: i32| v.clamp(0, w - 1) as u32;
    let cy = |v: i32| v.clamp(0, h - 1) as u32;
    let (x0, y0) = (x0 as i32, y0 as i32);
    let p00 = src.get_pixel(cx(x0), cy(y0)).0;
    let p10 = src.get_pixel(cx(x0 + 1), cy(y0)).0;
    let p01 = src.get_pixel(cx(x0), cy(y0 + 1)).0;
    let p11 = src.get_pixel(cx(x0 + 1), cy(y0 + 1)).0;
    let mut out = [0.0; 3];
    for i in 0..3 {
        let top = p00[i] + (p10[i] - p00[i]) * fx;
        let bot = p01[i] + (p11[i] - p01[i]) * fx;
        out[i] = top + (bot - top) * fy;
    }
    out
}

/// Bilinear sample of a single-channel buffer with edge clamping.
#[inline]
pub fn sample_gray(buf: &[f32], w: u32, h: u32, x: f32, y: f32) -> f32 {
    let x = x - 0.5;
    let y = y - 0.5;
    let x0 = x.floor();
    let y0 = y.floor();
    let fx = x - x0;
    let fy = y - y0;
    let cx = |v: i32| v.clamp(0, w as i32 - 1) as usize;
    let cy = |v: i32| v.clamp(0, h as i32 - 1) as usize;
    let (x0, y0) = (x0 as i32, y0 as i32);
    let at = |x: usize, y: usize| buf[y * w as usize + x];
    let top = at(cx(x0), cy(y0)) * (1.0 - fx) + at(cx(x0 + 1), cy(y0)) * fx;
    let bot = at(cx(x0), cy(y0 + 1)) * (1.0 - fx) + at(cx(x0 + 1), cy(y0 + 1)) * fx;
    top * (1.0 - fy) + bot * fy
}

/// Resample `src` through `m` into a new `w`×`h` image (bilinear).
pub fn warp(src: &Rgb32FImage, w: u32, h: u32, m: &Affine) -> Rgb32FImage {
    let mut out = Rgb32FImage::new(w, h);
    for (x, y, px) in out.enumerate_pixels_mut() {
        let p = m.apply(Point::new(x as f32 + 0.5, y as f32 + 0.5));
        px.0 = sample(src, p.x, p.y);
    }
    out
}

pub fn to_f32(img: &RgbImage) -> Rgb32FImage {
    let mut out = Rgb32FImage::new(img.width(), img.height());
    for (o, i) in out.pixels_mut().zip(img.pixels()) {
        o.0 = i.0.map(|v| v as f32 / 255.0);
    }
    out
}

pub fn to_u8(img: &Rgb32FImage) -> RgbImage {
    let mut out = RgbImage::new(img.width(), img.height());
    for (o, i) in out.pixels_mut().zip(img.pixels()) {
        o.0 = i.0.map(|v| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
    }
    out
}

#[inline]
pub fn srgb_to_linear(v: f32) -> f32 {
    if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
}

#[inline]
pub fn linear_to_srgb(v: f32) -> f32 {
    let v = v.max(0.0);
    if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 }
}

/// Relative luminance of a linear RGB triple.
#[inline]
pub fn luma(p: [f32; 3]) -> f32 {
    0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]
}

/// Like [`warp`], but box-filters the source first when `m` shrinks it by 2×
/// or more, so large photos don't alias.
pub fn warp_resampled(src: &Rgb32FImage, w: u32, h: u32, m: &Affine) -> Rgb32FImage {
    let k = m.scale().floor() as u32;
    if k < 2 {
        return warp(src, w, h, m);
    }
    let (sw, sh) = ((src.width() / k).max(1), (src.height() / k).max(1));
    let mut small = Rgb32FImage::new(sw, sh);
    let norm = 1.0 / (k * k) as f32;
    for (x, y, px) in small.enumerate_pixels_mut() {
        let mut acc = [0.0f32; 3];
        for dy in 0..k {
            for dx in 0..k {
                let p = src.get_pixel((x * k + dx).min(src.width() - 1), (y * k + dy).min(src.height() - 1)).0;
                for i in 0..3 {
                    acc[i] += p[i];
                }
            }
        }
        px.0 = acc.map(|v| v * norm);
    }
    let kf = k as f32;
    let m = Affine { a: m.a / kf, b: m.b / kf, c: m.c / kf, d: m.d / kf, e: m.e / kf, f: m.f / kf };
    warp(&small, w, h, &m)
}
