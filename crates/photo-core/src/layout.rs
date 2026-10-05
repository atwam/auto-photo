//! Print sheet: six photos on 10×15 cm (or 4×6 in) paper plus a seal.

use crate::draw::{Canvas, Color, bold, regular, text_width, wrap};
use crate::spec::{Paper, Spec};
use image::RgbImage;
use std::f32::consts::{PI, TAU};

const WHITE: Color = [255, 255, 255];
const INK: Color = [38, 42, 48];
const MUTED: Color = [110, 116, 124];
const CUT: Color = [170, 170, 170];
const NAVY: Color = [24, 38, 72];
const GOLD: Color = [201, 162, 39];

/// Standards on the seal's rim (language-neutral).
const RIM: &str = "ISO/IEC 19794-5 · ICAO 9303 · ";

const COLS: usize = 3;
const ROWS: usize = 2;
const GAP_MM: f32 = 2.0;
const MARGIN_MM: f32 = 4.0;

/// Text drawn on the sheet. `strings` overrides the English defaults, for
/// localisation; keys: `compliant`, `spec_name`.
#[derive(Default)]
pub struct SheetText {
    pub strings: std::collections::HashMap<String, String>,
}

impl SheetText {
    fn get<'s>(&'s self, key: &str, default: &'s str) -> &'s str {
        self.strings.get(key).map(String::as_str).unwrap_or(default)
    }
}

/// Lay out `photo` (already rendered at `dpi` for `spec`) six times.
pub fn sheet(photo: &RgbImage, spec: &Spec, paper: Paper, dpi: f32, text: &SheetText) -> RgbImage {
    let k = dpi / 25.4;
    let (pw, ph) = paper.size_mm();
    let mut c = Canvas::new((pw * k).round() as u32, (ph * k).round() as u32, WHITE);

    let grid_w = COLS as f32 * spec.width_mm + (COLS - 1) as f32 * GAP_MM;
    let grid_h = ROWS as f32 * spec.height_mm + (ROWS - 1) as f32 * GAP_MM;
    let gx = MARGIN_MM;
    let gy = (ph - grid_h) * 0.5;

    for r in 0..ROWS {
        for col in 0..COLS {
            let x = gx + col as f32 * (spec.width_mm + GAP_MM);
            let y = gy + r as f32 * (spec.height_mm + GAP_MM);
            let (px, py) = ((x * k).round() as i64, (y * k).round() as i64);
            image::imageops::replace(&mut c.img, photo, px, py);
        }
    }
    cut_marks(&mut c, k, gx, gy, spec, grid_w, grid_h);

    // Seal panel to the right of the grid.
    let x0 = gx + grid_w + MARGIN_MM;
    let x1 = pw - MARGIN_MM * 0.75;
    seal_panel(&mut c, k, x0, gy, x1 - x0, grid_h, spec, text);
    c.img
}

/// Short cut lines in the margins and gutters, aligned with photo edges.
fn cut_marks(c: &mut Canvas, k: f32, gx: f32, gy: f32, spec: &Spec, grid_w: f32, grid_h: f32) {
    let len = 2.5 * k;
    let w = 0.12 * k;
    let mut xs = Vec::new();
    for col in 0..COLS {
        let x = gx + col as f32 * (spec.width_mm + GAP_MM);
        xs.extend([x, x + spec.width_mm]);
    }
    let mut ys = Vec::new();
    for r in 0..ROWS {
        let y = gy + r as f32 * (spec.height_mm + GAP_MM);
        ys.extend([y, y + spec.height_mm]);
    }
    let (top, bottom) = (gy * k, (gy + grid_h) * k);
    let (left, right) = (gx * k, (gx + grid_w) * k);
    let gap = 0.6 * k;
    for &x in &xs {
        let x = x * k;
        c.line(x, top - gap - len, x, top - gap, w, CUT);
        c.line(x, bottom + gap, x, bottom + gap + len, w, CUT);
    }
    for &y in &ys {
        let y = y * k;
        c.line(left - gap - len.min(left - gap), y, left - gap, y, w, CUT);
        c.line(right + gap, y, right + gap + len * 0.6, y, w, CUT);
    }
}

/// The seal (flag rosette with the standards on its rim) and a short
/// compliance line, centred vertically in the panel.
#[allow(clippy::too_many_arguments)]
fn seal_panel(c: &mut Canvas, k: f32, x: f32, y: f32, w: f32, h: f32, spec: &Spec, text: &SheetText) {
    let (px, pw) = (x * k, w * k);
    let cx = px + pw * 0.5;
    let r = (pw * 0.5).min(13.5 * k) - 0.2 * k;

    // Text block under the seal, measured first so the group can be centred.
    let title_size = 2.4 * k;
    let name_size = 1.9 * k;
    let title = wrap(bold(), title_size, text.get("compliant", "Compliant with ISO/IEC\u{a0}19794-5"), pw);
    let name = wrap(regular(), name_size, text.get("spec_name", spec.name), pw);
    let format = format!("{} × {} mm", spec.width_mm, spec.height_mm);
    let gap = 3.5 * k;
    let text_h = title.len() as f32 * title_size * 1.25 + 1.2 * k + (name.len() + 1) as f32 * name_size * 1.35;
    let top = y * k + ((h * k) - (2.0 * r + gap + text_h)).max(0.0) * 0.5;
    let cy = top + r;

    seal(c, cx, cy, r, spec.id);

    let mut ty = cy + r + gap + title_size;
    for line in &title {
        c.text(bold(), title_size, cx - text_width(bold(), title_size, line) * 0.5, ty, line, INK);
        ty += title_size * 1.25;
    }
    ty += 1.2 * k;
    for line in name.iter().chain(std::iter::once(&format)) {
        c.text(regular(), name_size, cx - text_width(regular(), name_size, line) * 0.5, ty, line, MUTED);
        ty += name_size * 1.35;
    }
}

/// Rosette seal of radius `r`: scalloped navy edge, gold rings, the
/// standards around the rim, and the spec's flag in the middle.
fn seal(c: &mut Canvas, cx: f32, cy: f32, r: f32, spec_id: &str) {
    // Scalloped edge: radius varies with angle.
    const BUMPS: f32 = 44.0;
    let edge = |x: f32, y: f32| {
        let (dx, dy) = (x - cx, y - cy);
        let d = (dx * dx + dy * dy).sqrt();
        let theta = dy.atan2(dx);
        let rr = r * (1.0 - 0.035 * (0.5 - 0.5 * (theta * BUMPS).cos()));
        (rr - d + 0.5).clamp(0.0, 1.0)
    };
    c.shade(cx - r - 1.0, cy - r - 1.0, cx + r + 1.0, cy + r + 1.0, NAVY, edge);
    let line = (r * 0.012).max(1.0);
    c.ring(cx, cy, r * 0.9 - line, r * 0.9, GOLD);
    c.ring(cx, cy, r * 0.63 - line, r * 0.63, GOLD);
    let rim_size = r * 0.13;
    let rim_r = r * 0.765 - rim_size * 0.35;
    // Repeat the standards as often as they fit, so the letter spacing stays tight.
    let fits = (TAU * rim_r * 0.9 / text_width(bold(), rim_size, RIM)).floor().max(1.0) as usize;
    c.text_circle(bold(), rim_size, cx, cy, rim_r, &RIM.repeat(fits), WHITE);

    // Flag clipped to the inner disk, supersampled 3×3 for clean colour edges.
    let fr = r * 0.6;
    c.ring(cx, cy, 0.0, fr + line, WHITE);
    let n = 3;
    for py in (cy - fr).floor() as i32..=(cy + fr).ceil() as i32 {
        for pxx in (cx - fr).floor() as i32..=(cx + fr).ceil() as i32 {
            let mut acc = [0.0f32; 3];
            let mut hits = 0;
            for sy in 0..n {
                for sx in 0..n {
                    let x = pxx as f32 + (sx as f32 + 0.5) / n as f32;
                    let y = py as f32 + (sy as f32 + 0.5) / n as f32;
                    let (u, v) = ((x - cx) / fr, (y - cy) / fr);
                    if u * u + v * v > 1.0 {
                        continue;
                    }
                    let col = flag(spec_id, u, v);
                    for i in 0..3 {
                        acc[i] += col[i] as f32;
                    }
                    hits += 1;
                }
            }
            if hits > 0 {
                let col = acc.map(|v| (v / hits as f32) as u8);
                c.blend(pxx, py, col, hits as f32 / (n * n) as f32);
            }
        }
    }
}

/// Flag colour at (u, v) in [-1, 1]² (the central square of the flag).
fn flag(spec_id: &str, u: f32, v: f32) -> Color {
    match spec_id {
        "fr" => match u {
            u if u < -1.0 / 3.0 => [0, 38, 84],
            u if u < 1.0 / 3.0 => [255, 255, 255],
            _ => [206, 17, 38],
        },
        "de" => match v {
            v if v < -1.0 / 3.0 => [0, 0, 0],
            v if v < 1.0 / 3.0 => [221, 0, 0],
            _ => [255, 206, 0],
        },
        "uk" => union_flag(u, v),
        _ => eu_flag(u, v),
    }
}

/// Union flag, square crop of the 2:1 flag (flag units: 60×30, centre 0).
fn union_flag(u: f32, v: f32) -> Color {
    const BLUE: Color = [1, 33, 105];
    const RED: Color = [200, 16, 46];
    let (x, y) = (u * 15.0, v * 15.0);
    let (ax, ay) = (x.abs(), y.abs());
    if ax <= 3.0 || ay <= 3.0 {
        return RED; // St George's cross
    }
    if ax <= 5.0 || ay <= 5.0 {
        return WHITE; // its fimbriation
    }
    // Diagonals along y = ±x/2 (corner to corner of the 60×30 flag).
    let norm = (1.0f32 + 0.25).sqrt();
    let d1 = (y - x * 0.5) / norm;
    let d2 = (y + x * 0.5) / norm;
    // Counterchanged red saltire: offset to the clockwise side in each quadrant.
    let red = |d: f32, side: f32| (d * side) > 0.0 && (d * side) < 2.0;
    let side = if (x > 0.0) == (y > 0.0) { -1.0 } else { 1.0 };
    if (d1.abs() <= 3.0 && red(d1, side)) || (d2.abs() <= 3.0 && red(d2, -side)) {
        return RED;
    }
    if d1.abs() <= 3.0 || d2.abs() <= 3.0 {
        return WHITE;
    }
    BLUE
}

/// European flag: 12 gold stars in a circle on blue.
fn eu_flag(u: f32, v: f32) -> Color {
    const BLUE: Color = [0, 51, 153];
    const GOLD_STAR: Color = [255, 204, 0];
    let ring = 0.62;
    let star_r = 0.12;
    for i in 0..12 {
        let a = i as f32 * TAU / 12.0;
        let (sx, sy) = (ring * a.sin(), -ring * a.cos());
        if in_star(u - sx, v - sy, star_r) {
            return GOLD_STAR;
        }
    }
    BLUE
}

/// Point inside an upright five-pointed star of outer radius `r` at the origin.
fn in_star(x: f32, y: f32, r: f32) -> bool {
    let d = (x * x + y * y).sqrt();
    if d > r {
        return false;
    }
    // Angle from the nearest tip (tips every 72°, first at 12 o'clock), in [0, 36°].
    let sector = TAU / 5.0;
    let a = ((x.atan2(-y) + sector * 0.5).rem_euclid(sector) - sector * 0.5).abs();
    // In the tip's local frame: tip at (0, r), inner vertex at 36° with radius 0.382 r.
    let inner = r * 0.382;
    let (tx, ty) = (0.0, r);
    let (ix, iy) = ((PI / 5.0).sin() * inner, (PI / 5.0).cos() * inner);
    let (px, py) = (a.sin() * d, a.cos() * d);
    // Inside when on the same side of the tip→inner edge as the centre.
    let side = |qx: f32, qy: f32| (ix - tx) * (qy - ty) - (iy - ty) * (qx - tx);
    side(px, py).signum() == side(0.0, 0.0).signum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::by_id;

    #[test]
    fn sheet_has_paper_size_and_exact_photo_cells() {
        let spec = by_id("fr").unwrap();
        let k = 300.0 / 25.4;
        let photo = RgbImage::from_pixel((35.0 * k) as u32, (45.0 * k) as u32, image::Rgb([10, 200, 10]));
        for (paper, w, h) in [(Paper::Cm10x15, 1772, 1181), (Paper::In4x6, 1800, 1200)] {
            let img = sheet(&photo, spec, paper, 300.0, &SheetText::default());
            assert_eq!((img.width(), img.height()), (w, h));
            // Six green cells.
            let green = img.pixels().filter(|p| p.0 == [10, 200, 10]).count();
            assert_eq!(green, 6 * (photo.width() * photo.height()) as usize);
        }
    }
}
