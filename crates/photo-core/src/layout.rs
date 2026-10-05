//! Print sheet: six photos on 10×15 cm (or 4×6 in) paper plus a seal panel.

use crate::checks::{Report, Status};
use crate::draw::{Canvas, Color, bold, regular, wrap};
use crate::spec::{Paper, Spec};
use image::RgbImage;

const WHITE: Color = [255, 255, 255];
const INK: Color = [38, 42, 48];
const MUTED: Color = [110, 116, 124];
const CUT: Color = [170, 170, 170];
const PASS: Color = [22, 122, 74];
const WARN: Color = [196, 120, 18];
const FAIL: Color = [190, 40, 40];

const COLS: usize = 3;
const ROWS: usize = 2;
const GAP_MM: f32 = 2.0;
const MARGIN_MM: f32 = 4.0;

pub struct SheetText<'a> {
    /// e.g. "2026-10-05".
    pub date: &'a str,
}

fn status_color(s: Status) -> Color {
    match s {
        Status::Pass => PASS,
        Status::Warn => WARN,
        Status::Fail => FAIL,
    }
}

/// Lay out `photo` (already rendered at `dpi` for `spec`) six times.
pub fn sheet(
    photo: &RgbImage,
    spec: &Spec,
    paper: Paper,
    dpi: f32,
    report: &Report,
    text: &SheetText<'_>,
) -> RgbImage {
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
    seal_panel(
        &mut c,
        k,
        x0,
        gy,
        x1 - x0,
        grid_h,
        spec,
        paper,
        report,
        text,
    );
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

#[allow(clippy::too_many_arguments)]
fn seal_panel(
    c: &mut Canvas,
    k: f32,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    spec: &Spec,
    paper: Paper,
    report: &Report,
    text: &SheetText<'_>,
) {
    let color = status_color(report.status);
    let (px, pw) = (x * k, w * k);
    let cx = px + pw * 0.5;

    // Seal: double ring, text on the rim, tick and spec code in the middle.
    let r = (pw * 0.5).min(14.0 * k) - 0.3 * k;
    let cy = y * k + r + 0.3 * k;
    c.ring(cx, cy, r - 0.35 * k, r, color);
    c.ring(cx, cy, r * 0.68 - 0.15 * k, r * 0.68, color);
    let rim = match report.status {
        Status::Pass => "SPEC CHECKED · SPEC CHECKED",
        Status::Warn => "CHECK WARNINGS · REVIEW",
        Status::Fail => "NOT COMPLIANT · RETAKE",
    };
    let rim_size = 2.1 * k;
    let rim_r = r * 0.84 - rim_size * 0.35;
    c.text_arc(bold(), rim_size, cx, cy, rim_r, rim, color);
    let code = spec.id.to_uppercase();
    let size = r * 0.36;
    c.text_centered(bold(), size, cx, cy + size * 0.1, &code, color);
    let fmt = format!("{}×{} mm", spec.width_mm, spec.height_mm);
    c.text_centered(regular(), r * 0.15, cx, cy + r * 0.42, &fmt, color);
    let icon = r * 0.28;
    match report.status {
        Status::Pass => c.tick(cx - icon * 0.5, cy - r * 0.58, icon, 0.45 * k, color),
        _ => c.bang(cx - icon * 0.5, cy - r * 0.6, icon, 0.45 * k, color),
    }

    // Spec name and checklist.
    let mut ty = cy + r + 3.6 * k;
    let name_size = 2.3 * k;
    for line in wrap(bold(), name_size, spec.name, pw) {
        c.text(bold(), name_size, px, ty, &line, INK);
        ty += name_size * 1.2;
    }
    ty += 0.8 * k;
    let item = 1.95 * k;
    for chk in &report.checks {
        let col = status_color(chk.status);
        let icon = item * 0.9;
        match chk.status {
            Status::Pass => c.tick(px, ty - icon * 0.85, icon, 0.22 * k, col),
            _ => c.bang(px, ty - icon * 0.9, icon, 0.22 * k, col),
        }
        c.text(regular(), item, px + icon * 1.25, ty, chk.label, INK);
        ty += item * 1.38;
    }

    // Footer, anchored to the bottom of the panel.
    let small = 1.6 * k;
    let mut footer = vec![
        format!("Checked {}", text.date),
        format!("Print at 100% on {} — no scaling.", paper.label()),
        "Automated self-check, not an official certification.".to_string(),
    ];
    if !spec.print_accepted {
        footer.insert(
            0,
            "Printed photos are not accepted for this document.".into(),
        );
    }
    let lines: Vec<String> = footer
        .iter()
        .flat_map(|l| wrap(regular(), small, l, pw))
        .collect();
    let mut fy = (y + h) * k - (lines.len() as f32 - 1.0) * small * 1.3;
    for l in lines {
        let col = if l.starts_with("Printed photos") {
            FAIL
        } else {
            MUTED
        };
        c.text(regular(), small, px, fy, &l, col);
        fy += small * 1.3;
    }
}
