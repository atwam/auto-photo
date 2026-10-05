//! Compliance checks on the analysed portrait and the final frame.

use crate::detect::mesh;
use crate::geometry::Placement;
use crate::imgops::{luma, srgb_to_linear};
use crate::pipeline::Analysis;
use crate::spec::Spec;
use crate::tone::{Frame, ToneParams};
use image::Rgb32FImage;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Pass,
    Warn,
    Fail,
}

#[derive(Clone, Debug, Serialize)]
pub struct Check {
    pub id: &'static str,
    pub label: &'static str,
    pub status: Status,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Report {
    pub spec: &'static str,
    pub status: Status,
    pub checks: Vec<Check>,
}

fn check(id: &'static str, label: &'static str, status: Status, detail: String) -> Check {
    Check { id, label, status, detail }
}

fn grade(v: f32, warn: f32, fail: f32) -> Status {
    if v >= fail {
        Status::Fail
    } else if v >= warn {
        Status::Warn
    } else {
        Status::Pass
    }
}

/// `original` is the frame before tone corrections; `output` the final sRGB frame.
pub fn run(
    spec: &Spec,
    a: &Analysis,
    p: &Placement,
    original: &Frame,
    output: &Rgb32FImage,
    tone: &ToneParams,
) -> Report {
    let mut c = Vec::new();
    let pts = &a.landmarks.points;

    // Geometry.
    let (lo, hi) = (spec.head_min_mm, spec.head_max_mm);
    c.push(check(
        "head_size",
        "Head size",
        if p.head_mm >= lo && p.head_mm <= hi { Status::Pass } else { Status::Fail },
        format!("{:.1} mm chin to crown (allowed {lo}–{hi} mm)", p.head_mm),
    ));
    let (elo, ehi) = (spec.eyes_min_mm, spec.eyes_max_mm);
    let e = p.eyes_from_bottom_mm;
    c.push(check(
        "eye_line",
        "Eye position",
        if e >= elo && e <= ehi { Status::Pass } else { Status::Fail },
        format!("eyes {e:.1} mm above bottom edge (allowed {elo}–{ehi} mm)"),
    ));
    let off = p.center_offset_mm.abs();
    c.push(check("centered", "Head centred", grade(off, 1.0, 2.0), format!("{off:.1} mm off centre")));

    // Pose: yaw from eye-to-cheek asymmetry; roll is corrected so only report it.
    let dr = pts[mesh::R_IRIS].dist(pts[mesh::CHEEK_R]);
    let dl = pts[mesh::L_IRIS].dist(pts[mesh::CHEEK_L]);
    let yaw = (dr - dl).abs() / (dr + dl);
    c.push(check(
        "facing",
        "Facing the camera",
        grade(yaw, 0.08, 0.15),
        format!("asymmetry {:.0}%, tilt corrected by {:.1}°", yaw * 100.0, a.head.roll.to_degrees()),
    ));

    // Eyes open: lid opening relative to eye width.
    let ear = |up: usize, low: usize, a: usize, b: usize| pts[up].dist(pts[low]) / pts[a].dist(pts[b]);
    let ear_r = ear(mesh::R_EYE_UPPER, mesh::R_EYE_LOWER, mesh::R_EYE_OUTER, mesh::R_EYE_INNER);
    let ear_l = ear(mesh::L_EYE_UPPER, mesh::L_EYE_LOWER, mesh::L_EYE_OUTER, mesh::L_EYE_INNER);
    let ear_min = ear_r.min(ear_l);
    c.push(check(
        "eyes_open",
        "Eyes open",
        if ear_min < 0.12 {
            Status::Fail
        } else if ear_min < 0.18 {
            Status::Warn
        } else {
            Status::Pass
        },
        format!("eye opening {:.2}", ear_min),
    ));

    // Neutral expression: mouth closed and corners not raised.
    let mouth_w = pts[mesh::MOUTH_R].dist(pts[mesh::MOUTH_L]);
    let gap = pts[mesh::LIP_UPPER_INNER].dist(pts[mesh::LIP_LOWER_INNER]) / mouth_w;
    let lip_mid = pts[mesh::LIP_UPPER_INNER].mid(pts[mesh::LIP_LOWER_INNER]);
    let corners = pts[mesh::MOUTH_R].mid(pts[mesh::MOUTH_L]);
    // Positive when the corners sit above the lip centre (along the face axis).
    let up = a.head.up;
    let lift = ((corners.x - lip_mid.x) * up.x + (corners.y - lip_mid.y) * up.y) / mouth_w;
    let (status, detail) = if gap > 0.12 {
        (Status::Fail, "mouth open".to_string())
    } else if lift > 0.08 || gap > 0.06 {
        (Status::Fail, "smiling; keep a neutral expression".to_string())
    } else if lift > 0.04 {
        (Status::Warn, "slight smile".to_string())
    } else {
        (Status::Pass, "neutral".to_string())
    };
    c.push(check("expression", "Neutral expression", status, detail));

    // Background, measured on the original pixels.
    let bg: Vec<usize> =
        (0..original.alpha.len()).filter(|&i| original.alpha[i] < 0.03 && !original.outside[i]).collect();
    let raw = original.linear.as_raw();
    let lum = |i: usize| luma([raw[i * 3], raw[i * 3 + 1], raw[i * 3 + 2]]);
    let mut status = Status::Pass;
    let mut detail = String::from("plain");
    if bg.len() > original.alpha.len() / 50 {
        let mean = bg.iter().map(|&i| lum(i)).sum::<f32>() / bg.len() as f32;
        let sd = (bg.iter().map(|&i| (lum(i) - mean).powi(2)).sum::<f32>() / bg.len() as f32).sqrt();
        let rel = sd / mean.max(1e-3);
        status = grade(rel, 0.12, 0.3);
        detail = format!("original variation {:.0}%", rel * 100.0);
    }
    if tone.background >= 0.5 {
        // Replaced: the output is plain, but a busy original leaves edge artefacts.
        status = status.min(Status::Warn);
        detail.push_str("; replaced with a uniform colour");
        if spec.id == "uk" {
            status = Status::Warn;
            detail.push_str(" (UK rules forbid digital alteration)");
        }
    } else if status != Status::Pass {
        detail.push_str("; consider enabling background replacement");
    }
    c.push(check("background", "Plain background", status, detail));

    // Exposure on the output: clipped or very dark skin.
    let out = output.as_raw();
    let skin_l: Vec<f32> =
        original.skin.iter().map(|&i| luma([out[i * 3], out[i * 3 + 1], out[i * 3 + 2]].map(srgb_to_linear))).collect();
    if !skin_l.is_empty() {
        let clipped = skin_l.iter().filter(|&&l| l > 0.95).count() as f32 / skin_l.len() as f32;
        let mut s = skin_l.clone();
        s.sort_by(f32::total_cmp);
        let median = s[s.len() / 2];
        let (status, detail) = if clipped > 0.05 {
            (Status::Fail, format!("{:.0}% of the face is blown out", clipped * 100.0))
        } else if median < 0.03 {
            (Status::Fail, "face too dark".into())
        } else if clipped > 0.01 || median < 0.06 {
            (Status::Warn, "exposure borderline".into())
        } else {
            (Status::Pass, "well exposed".into())
        };
        c.push(check("exposure", "Exposure", status, detail));
    }

    // Sharpness: Laplacian variance over the face, at the output scale.
    let sharp = laplacian_variance(original, &original.skin);
    c.push(check(
        "sharpness",
        "Sharp focus",
        if sharp < 2e-5 {
            Status::Fail
        } else if sharp < 6e-5 {
            Status::Warn
        } else {
            Status::Pass
        },
        format!("detail score {:.1}", sharp * 1e5),
    ));

    // Source resolution: print needs ~300 dpi (11.8 px/mm).
    let ppmm = p.src_px_per_mm;
    c.push(check(
        "resolution",
        "Resolution",
        if ppmm < 6.0 {
            Status::Fail
        } else if ppmm < 11.0 {
            Status::Warn
        } else {
            Status::Pass
        },
        format!("{:.0} dpi of source detail", ppmm * 25.4),
    ));

    // The frame must lie within the photo; missing pixels under the face
    // (central half, below the eyes) mean the head or shoulders are cut off.
    let w = original.linear.width() as usize;
    let eye_row = ((p.height_mm - p.eyes_from_bottom_mm) * original.px_per_mm) as usize;
    let cut = original
        .outside
        .iter()
        .enumerate()
        .filter(|&(i, &o)| o && i / w > eye_row && (i % w) > w / 4 && (i % w) < 3 * w / 4)
        .count() as f32
        / original.outside.len() as f32;
    c.push(check(
        "framing",
        "Whole head in photo",
        if cut > 0.01 { Status::Fail } else { grade(p.outside_fraction, 0.02, 0.15) },
        format!("{:.0}% of the frame lies outside the original photo", p.outside_fraction * 100.0),
    ));

    let status = c.iter().map(|c| c.status).max().unwrap_or(Status::Pass);
    Report { spec: spec.id, status, checks: c }
}

fn laplacian_variance(f: &Frame, idx: &[usize]) -> f32 {
    let w = f.linear.width() as usize;
    let h = f.linear.height() as usize;
    let raw = f.linear.as_raw();
    let l = |x: usize, y: usize| {
        let i = (y * w + x) * 3;
        luma([raw[i], raw[i + 1], raw[i + 2]])
    };
    let mut vals = Vec::with_capacity(idx.len());
    for &i in idx {
        let (x, y) = (i % w, i / w);
        if x == 0 || y == 0 || x + 1 >= w || y + 1 >= h {
            continue;
        }
        vals.push(l(x - 1, y) + l(x + 1, y) + l(x, y - 1) + l(x, y + 1) - 4.0 * l(x, y));
    }
    if vals.is_empty() {
        return 0.0;
    }
    let mean = vals.iter().sum::<f32>() / vals.len() as f32;
    vals.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / vals.len() as f32
}
