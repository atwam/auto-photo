//! Illuminant estimation on the original photo. Several cues vote in
//! log-chromaticity space (u = ln R/G, v = ln B/G, linear light):
//! - eye whites (sclera), the most reliable neutral reference on a face,
//! - the background (usually light and roughly neutral, but may be cream or blue),
//! - gray-edge over the whole photo (edges average to the illuminant colour).

use crate::detect::{Landmarks, mesh};
use crate::imgops::{Point, luma, srgb_to_linear};
use crate::segment::Matte;
use image::Rgb32FImage;
use serde::Serialize;

/// Largest correction allowed per axis (e^1.3 ≈ 3.7×), enough for tungsten.
const MAX_LOG: f32 = 1.3;
/// Eye whites are slightly warm even under neutral light.
const SCLERA_TINT: [f32; 2] = [0.12, -0.04];
/// A cue further than this from the others' consensus is mostly ignored.
const OUTLIER: f32 = 0.4;
/// Casts smaller than this are left alone (soft threshold).
const DEAD_ZONE: f32 = 0.04;

const R_EYE: [usize; 16] = [33, 7, 163, 144, 145, 153, 154, 155, 133, 173, 157, 158, 159, 160, 161, 246];
const L_EYE: [usize; 16] = [263, 249, 390, 373, 374, 380, 381, 382, 362, 398, 384, 385, 386, 387, 388, 466];

#[derive(Clone, Debug, Serialize)]
pub struct Cue {
    pub name: &'static str,
    /// (ln R/G, ln B/G) of the neutral reference.
    pub uv: [f32; 2],
    pub weight: f32,
}

#[derive(Clone, Debug, Serialize)]
pub struct Illuminant {
    pub uv: [f32; 2],
    /// Linear-light channel gains that neutralise it, luminance-preserving.
    pub gains: [f32; 3],
    pub cues: Vec<Cue>,
}

impl Default for Illuminant {
    fn default() -> Self {
        Illuminant { uv: [0.0; 2], gains: [1.0; 3], cues: Vec::new() }
    }
}

fn uv_of(c: [f32; 3]) -> Option<[f32; 2]> {
    if c.iter().any(|&v| v <= 1e-5) {
        return None;
    }
    Some([(c[0] / c[1]).ln(), (c[2] / c[1]).ln()])
}

fn lin(img: &Rgb32FImage, x: u32, y: u32) -> [f32; 3] {
    img.get_pixel(x, y).0.map(srgb_to_linear)
}

fn clipped(img: &Rgb32FImage, x: u32, y: u32) -> bool {
    img.get_pixel(x, y).0.iter().any(|&v| v > 0.98)
}

pub fn estimate(img: &Rgb32FImage, lm: &Landmarks, matte: &Matte) -> Illuminant {
    let mut cues = Vec::new();
    if let Some(uv) = sclera(img, lm) {
        let uv = [uv[0] - SCLERA_TINT[0], uv[1] - SCLERA_TINT[1]];
        cues.push(Cue { name: "eyes", uv, weight: 2.0 });
    }
    if let Some(uv) = background(img, matte) {
        cues.push(Cue { name: "background", uv, weight: 1.0 });
    }
    if let Some(uv) = gray_edge(img) {
        cues.push(Cue { name: "edges", uv, weight: 0.75 });
    }
    // Down-weight cues far from the consensus (e.g. a deliberately coloured
    // wall). Consensus = the medoid: the cue closest to all others.
    let dist = |a: [f32; 2], b: [f32; 2]| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt();
    let medoid = cues
        .iter()
        .map(|c| (c.uv, cues.iter().map(|o| dist(c.uv, o.uv) * o.weight).sum::<f32>()))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(uv, _)| uv);
    let weights: Vec<f32> = cues
        .iter()
        .map(|c| match medoid {
            Some(m) if dist(c.uv, m) > OUTLIER => c.weight * 0.1,
            _ => c.weight,
        })
        .collect();
    for (c, w) in cues.iter_mut().zip(&weights) {
        c.weight = *w;
    }
    if cues.is_empty() {
        return Illuminant::default();
    }
    let uv = weighted_mean(&cues.iter().map(|c| (c.uv, c.weight)).collect::<Vec<_>>());
    let uv = uv.map(|v| (v.abs() - DEAD_ZONE).max(0.0).copysign(v).clamp(-MAX_LOG, MAX_LOG));
    Illuminant { uv, gains: gains_for(uv), cues }
}

fn weighted_mean(v: &[([f32; 2], f32)]) -> [f32; 2] {
    let total: f32 = v.iter().map(|x| x.1).sum::<f32>().max(1e-6);
    let mut m = [0.0f32; 2];
    for (uv, w) in v {
        m[0] += uv[0] * w / total;
        m[1] += uv[1] * w / total;
    }
    m
}

/// Gains that map the illuminant (e^u, 1, e^v) to neutral at the same luminance.
pub fn gains_for(uv: [f32; 2]) -> [f32; 3] {
    let illum = [uv[0].exp(), 1.0, uv[1].exp()];
    let l = luma(illum);
    [l / illum[0], l / illum[1], l / illum[2]]
}

fn sclera(img: &Rgb32FImage, lm: &Landmarks) -> Option<[f32; 2]> {
    let p = &lm.points;
    let mut samples: Vec<[f32; 3]> = Vec::new();
    for (contour, iris) in [(&R_EYE, mesh::R_IRIS), (&L_EYE, mesh::L_IRIS)] {
        let poly: Vec<Point> = contour.iter().map(|&i| p[i]).collect();
        let c = p[iris];
        let r = (1..=4).map(|k| p[iris + k].dist(c)).sum::<f32>() / 4.0 * 1.15;
        let (x0, x1) = poly.iter().fold((f32::MAX, f32::MIN), |(a, b), q| (a.min(q.x), b.max(q.x)));
        let (y0, y1) = poly.iter().fold((f32::MAX, f32::MIN), |(a, b), q| (a.min(q.y), b.max(q.y)));
        for y in y0.max(0.0) as u32..(y1.ceil() as u32).min(img.height()) {
            for x in x0.max(0.0) as u32..(x1.ceil() as u32).min(img.width()) {
                let q = Point::new(x as f32 + 0.5, y as f32 + 0.5);
                if q.dist(c) > r && inside(&poly, q) && !clipped(img, x, y) {
                    samples.push(lin(img, x, y));
                }
            }
        }
    }
    if samples.len() < 30 {
        return None;
    }
    // Keep the brighter half: lids, lashes and shadowed corners are darker.
    samples.sort_by(|a, b| luma(*a).total_cmp(&luma(*b)));
    let bright = &samples[samples.len() / 2..];
    let mut acc = [0.0f32; 3];
    for s in bright {
        for k in 0..3 {
            acc[k] += s[k];
        }
    }
    uv_of(acc)
}

fn background(img: &Rgb32FImage, matte: &Matte) -> Option<[f32; 2]> {
    let step = ((img.width() * img.height()) as f32 / 60_000.0).sqrt().max(1.0) as u32;
    let mut us = Vec::new();
    let mut vs = Vec::new();
    let mut total = 0usize;
    for y in (0..img.height()).step_by(step as usize) {
        for x in (0..img.width()).step_by(step as usize) {
            let Some(a) = matte.try_at_src(Point::new(x as f32 + 0.5, y as f32 + 0.5)) else { continue };
            total += 1;
            if a > 0.03 || clipped(img, x, y) {
                continue;
            }
            let c = lin(img, x, y);
            if luma(c) < 0.03 {
                continue;
            }
            if let Some(uv) = uv_of(c) {
                us.push(uv[0]);
                vs.push(uv[1]);
            }
        }
    }
    if us.len() < 200 || us.len() * 20 < total {
        return None;
    }
    Some([median(&mut us), median(&mut vs)])
}

/// Minkowski (p = 6) norm of per-channel gradients on a downscaled copy.
fn gray_edge(img: &Rgb32FImage) -> Option<[f32; 2]> {
    let step = (img.width().max(img.height()) / 512).max(1);
    let (w, h) = (img.width() / step, img.height() / step);
    if w < 8 || h < 8 {
        return None;
    }
    let at = |x: u32, y: u32| lin(img, x * step, y * step);
    let mut acc = [0.0f64; 3];
    for y in 0..h - 1 {
        for x in 0..w - 1 {
            if clipped(img, x * step, y * step) {
                continue;
            }
            let (c, r, d) = (at(x, y), at(x + 1, y), at(x, y + 1));
            for k in 0..3 {
                let g = ((r[k] - c[k]).powi(2) + (d[k] - c[k]).powi(2)).sqrt() as f64;
                acc[k] += g.powi(6);
            }
        }
    }
    uv_of(acc.map(|v| v.powf(1.0 / 6.0) as f32))
}

fn median(v: &mut [f32]) -> f32 {
    v.sort_by(f32::total_cmp);
    v[v.len() / 2]
}

fn inside(poly: &[Point], p: Point) -> bool {
    let mut c = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            c = !c;
        }
        j = i;
    }
    c
}
