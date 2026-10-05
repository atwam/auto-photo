//! Rendering of the final frame plus tone corrections: white balance,
//! exposure and background cleanup. All maths in linear light.

use crate::detect::mesh;
use crate::geometry::Placement;
use crate::imgops::{Point, linear_to_srgb, luma, srgb_to_linear, warp_resampled};
use crate::pipeline::Analysis;
use image::Rgb32FImage;
use serde::{Deserialize, Serialize};

/// User-facing correction controls. Strengths are 0..1.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ToneParams {
    /// Neutralise colour cast measured on the background.
    pub white_balance: f32,
    /// Automatic exposure correction based on face highlights.
    pub auto_exposure: f32,
    /// Extra exposure in stops, applied after the automatic correction.
    pub brightness_ev: f32,
    /// Blend the background towards `background_color` (1 = full replacement).
    pub background: f32,
    /// Target background colour (sRGB); `None` uses the spec's colour.
    pub background_color: Option<[u8; 3]>,
}

impl Default for ToneParams {
    fn default() -> Self {
        ToneParams {
            white_balance: 1.0,
            auto_exposure: 1.0,
            brightness_ev: 0.0,
            background: 1.0,
            background_color: None,
        }
    }
}

/// The frame cropped from the source, before corrections, with what we need
/// to correct it. Rendering once and re-toning is what makes sliders cheap.
#[derive(Clone)]
pub struct Frame {
    /// Linear RGB.
    pub linear: Rgb32FImage,
    /// Person alpha per pixel (1 = person).
    pub alpha: Vec<f32>,
    /// Pixels that fall outside the source photo.
    pub outside: Vec<bool>,
    /// Skin sample region (cheeks/nose), as pixel indices.
    pub skin: Vec<usize>,
    pub px_per_mm: f32,
}

pub fn render_frame(
    img: &Rgb32FImage,
    a: &Analysis,
    placement: &Placement,
    px_per_mm: f32,
) -> Frame {
    let w = (placement.width_mm * px_per_mm).round() as u32;
    let h = (placement.height_mm * px_per_mm).round() as u32;
    let m = placement.px_to_src(px_per_mm);
    let mut linear = warp_resampled(img, w, h, &m);
    for p in linear.pixels_mut() {
        p.0 = p.0.map(srgb_to_linear);
    }
    let (sw, sh) = (img.width() as f32, img.height() as f32);
    let mut alpha = Vec::with_capacity((w * h) as usize);
    let mut outside = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        for x in 0..w {
            let p = m.apply(Point::new(x as f32 + 0.5, y as f32 + 0.5));
            alpha.push(a.matte.at_src(p));
            outside.push(p.x < 0.0 || p.y < 0.0 || p.x >= sw || p.y >= sh);
        }
    }
    // Skin: ellipse centred between the eyes and the mouth, avoiding eyes,
    // brows and beard as much as a simple shape can.
    let to_px = m.inverse();
    let pts = &a.landmarks.points;
    let eyes = to_px.apply(a.head.eyes);
    let mouth = to_px.apply(pts[mesh::MOUTH_R].mid(pts[mesh::MOUTH_L]));
    let c = Point::new((eyes.x + mouth.x) * 0.5, eyes.y + (mouth.y - eyes.y) * 0.45);
    let rx = to_px
        .apply(pts[mesh::CHEEK_R])
        .dist(to_px.apply(pts[mesh::CHEEK_L]))
        * 0.32;
    let ry = (mouth.y - eyes.y) * 0.35;
    let mut skin = Vec::new();
    for y in (c.y - ry).max(0.0) as u32..((c.y + ry) as u32).min(h) {
        for x in (c.x - rx).max(0.0) as u32..((c.x + rx) as u32).min(w) {
            let (dx, dy) = ((x as f32 - c.x) / rx, (y as f32 - c.y) / ry);
            if dx * dx + dy * dy <= 1.0 {
                skin.push((y * w + x) as usize);
            }
        }
    }
    Frame {
        linear,
        alpha,
        outside,
        skin,
        px_per_mm,
    }
}

/// Measured corrections, reported back so the UI can show what changed.
#[derive(Clone, Debug, Default, Serialize)]
pub struct ToneReport {
    /// Per-channel gains applied for white balance.
    pub wb_gains: [f32; 3],
    /// Exposure change applied, in stops.
    pub exposure_ev: f32,
    /// Background pixels the white balance was measured on, as a fraction of the frame.
    pub background_fraction: f32,
}

/// Background pixels usable as a neutral reference.
fn background_pixels(f: &Frame) -> Vec<usize> {
    (0..f.alpha.len())
        .filter(|&i| f.alpha[i] < 0.03 && !f.outside[i])
        .collect()
}

pub fn apply(f: &Frame, bg_target: [u8; 3], p: &ToneParams) -> (Rgb32FImage, ToneReport) {
    let mut report = ToneReport {
        wb_gains: [1.0; 3],
        ..Default::default()
    };
    let px = |i: usize| f.linear.as_raw()[i * 3..i * 3 + 3].try_into().unwrap();

    // White balance: the background should be neutral. Trimmed mean of
    // reasonably bright, unsaturated background pixels.
    let bg = background_pixels(f);
    report.background_fraction = bg.len() as f32 / f.alpha.len() as f32;
    if report.background_fraction > 0.03 && p.white_balance > 0.0 {
        let mut acc = [0.0f64; 3];
        let mut n = 0usize;
        for &i in &bg {
            let c: [f32; 3] = px(i);
            let l = luma(c);
            let (mx, mn) = (c[0].max(c[1]).max(c[2]), c[0].min(c[1]).min(c[2]));
            if l > 0.05 && l < 0.95 && (mx - mn) / mx.max(1e-4) < 0.35 {
                for k in 0..3 {
                    acc[k] += c[k] as f64;
                }
                n += 1;
            }
        }
        if n > 100 {
            let mean = acc.map(|v| (v / n as f64) as f32);
            let l = luma(mean);
            for k in 0..3 {
                // Limit to ±25% so a deliberately tinted wall can't wreck skin tones.
                let g = (l / mean[k].max(1e-4)).clamp(0.75, 1.25);
                report.wb_gains[k] = 1.0 + (g - 1.0) * p.white_balance;
            }
        }
    }

    // Exposure: bring bright skin (98th percentile) to a healthy highlight
    // level. Using highlights rather than the mean avoids flattening every
    // skin tone to the same brightness.
    let mut gain = 1.0;
    if !f.skin.is_empty() {
        let mut l: Vec<f32> = f
            .skin
            .iter()
            .map(|&i| {
                let c: [f32; 3] = px(i);
                luma([
                    c[0] * report.wb_gains[0],
                    c[1] * report.wb_gains[1],
                    c[2] * report.wb_gains[2],
                ])
            })
            .collect();
        l.sort_by(f32::total_cmp);
        let hi = l[(l.len() as f32 * 0.98) as usize].max(1e-4);
        let target = srgb_to_linear(0.86);
        let auto = (target / hi).clamp(0.5, 3.0);
        gain = auto.powf(p.auto_exposure);
    }
    gain *= 2f32.powf(p.brightness_ev);
    report.exposure_ev = gain.log2();

    let bg_lin = bg_target.map(|v| srgb_to_linear(v as f32 / 255.0));
    let mut out = Rgb32FImage::new(f.linear.width(), f.linear.height());
    for (i, o) in out.pixels_mut().enumerate() {
        let c: [f32; 3] = px(i);
        let mut v = [0.0; 3];
        for k in 0..3 {
            v[k] = c[k] * report.wb_gains[k] * gain;
        }
        let v = soft_clip(v);
        // Outside the photo there is nothing to keep: always fill.
        let a = if f.outside[i] { 0.0 } else { f.alpha[i] };
        let b = if f.outside[i] { 1.0 } else { p.background };
        let keep = a + (1.0 - a) * (1.0 - b);
        for k in 0..3 {
            o.0[k] = linear_to_srgb(v[k] * keep + bg_lin[k] * (1.0 - keep));
        }
    }
    (out, report)
}

/// Roll off highlights instead of hard clipping, preserving hue.
fn soft_clip(v: [f32; 3]) -> [f32; 3] {
    let m = v[0].max(v[1]).max(v[2]);
    const KNEE: f32 = 0.8;
    if m <= KNEE {
        return v;
    }
    let x = (m - KNEE) / (1.0 - KNEE);
    let mapped = KNEE + (1.0 - KNEE) * (x / (1.0 + x));
    v.map(|c| c * mapped / m)
}
