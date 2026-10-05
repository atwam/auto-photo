//! Face detection (YuNet) and dense landmarks (MediaPipe face mesh).

use crate::Error;
use crate::imgops::{Affine, Point, Rect, warp_resampled};
use crate::models::{MESH_SIZE, Models, YUNET_SIZE, as_slice, run};
use image::Rgb32FImage;
use tract_onnx::prelude::*;

const SCORE_THRESHOLD: f32 = 0.6;
const NMS_IOU: f32 = 0.3;

/// A coarse detection: box plus YuNet's 5 points (image-left eye, image-right
/// eye, nose tip, image-left mouth corner, image-right mouth corner).
#[derive(Clone, Debug, serde::Serialize)]
pub struct Detection {
    pub bbox: Rect,
    pub score: f32,
    pub kps: [Point; 5],
}

/// All YuNet detections above threshold, after NMS, best first.
pub fn detect_faces(models: &Models, img: &Rgb32FImage) -> Result<Vec<Detection>, Error> {
    let (w, h) = img.dimensions();
    // Letterbox: scale the long side to 640, pad bottom/right with black.
    let scale = YUNET_SIZE as f32 / w.max(h) as f32;
    let m = Affine::crop(
        Point::new(YUNET_SIZE as f32 * 0.5 / scale, YUNET_SIZE as f32 * 0.5 / scale),
        YUNET_SIZE as f32,
        YUNET_SIZE as f32,
        1.0 / scale,
        0.0,
    );
    let small = warp_resampled(img, YUNET_SIZE as u32, YUNET_SIZE as u32, &m);
    let n = YUNET_SIZE;
    let input = tract_ndarray::Array4::from_shape_fn((1, 3, n, n), |(_, c, y, x)| {
        let (sx, sy) = (x as f32 / scale, y as f32 / scale);
        if sx >= w as f32 || sy >= h as f32 {
            return 0.0;
        }
        // YuNet expects BGR in 0..255.
        small.get_pixel(x as u32, y as u32).0[2 - c] * 255.0
    });
    let out = run(&models.yunet, input.into_tensor())?;
    if out.len() != 12 {
        return Err(Error::Model(format!("YuNet: expected 12 outputs, got {}", out.len())));
    }

    let mut dets = Vec::new();
    for (i, stride) in [8usize, 16, 32].into_iter().enumerate() {
        let cols = n / stride;
        let rows = n / stride;
        let cls = as_slice(&out[i])?;
        let obj = as_slice(&out[i + 3])?;
        let bbox = as_slice(&out[i + 6])?;
        let kps = as_slice(&out[i + 9])?;
        if cls.len() != rows * cols || bbox.len() != rows * cols * 4 || kps.len() != rows * cols * 10 {
            return Err(Error::Model("YuNet: unexpected output layout".into()));
        }
        let s = stride as f32;
        for r in 0..rows {
            for c in 0..cols {
                let idx = r * cols + c;
                let score = (cls[idx].clamp(0.0, 1.0) * obj[idx].clamp(0.0, 1.0)).sqrt();
                if score < SCORE_THRESHOLD {
                    continue;
                }
                let b = &bbox[idx * 4..idx * 4 + 4];
                let cx = (c as f32 + b[0]) * s;
                let cy = (r as f32 + b[1]) * s;
                let bw = b[2].exp() * s;
                let bh = b[3].exp() * s;
                let k = &kps[idx * 10..idx * 10 + 10];
                let pt =
                    |j: usize| Point::new((k[2 * j] + c as f32) * s / scale, (k[2 * j + 1] + r as f32) * s / scale);
                dets.push(Detection {
                    bbox: Rect { x: (cx - bw / 2.0) / scale, y: (cy - bh / 2.0) / scale, w: bw / scale, h: bh / scale },
                    score,
                    kps: [pt(0), pt(1), pt(2), pt(3), pt(4)],
                });
            }
        }
    }
    dets.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut keep: Vec<Detection> = Vec::new();
    for d in dets {
        if keep.iter().all(|k| k.bbox.iou(&d.bbox) < NMS_IOU) {
            keep.push(d);
        }
    }
    Ok(keep)
}

/// MediaPipe face-mesh result in source-image pixel coordinates.
#[derive(Clone, Debug)]
pub struct Landmarks {
    pub points: Vec<Point>,
    /// Depth per point, in source-image pixel units (relative).
    pub z: Vec<f32>,
    pub presence: f32,
}

/// Indices into the 478-point MediaPipe mesh.
pub mod mesh {
    pub const FOREHEAD_TOP: usize = 10;
    pub const CHIN: usize = 152;
    pub const NOSE_TIP: usize = 1;
    /// Subject's right eye (image left).
    pub const R_EYE_OUTER: usize = 33;
    pub const R_EYE_INNER: usize = 133;
    pub const R_EYE_UPPER: usize = 159;
    pub const R_EYE_LOWER: usize = 145;
    pub const L_EYE_OUTER: usize = 263;
    pub const L_EYE_INNER: usize = 362;
    pub const L_EYE_UPPER: usize = 386;
    pub const L_EYE_LOWER: usize = 374;
    pub const R_IRIS: usize = 468;
    pub const L_IRIS: usize = 473;
    pub const LIP_UPPER_INNER: usize = 13;
    pub const LIP_LOWER_INNER: usize = 14;
    pub const MOUTH_R: usize = 61;
    pub const MOUTH_L: usize = 291;
    /// Face outline: cheek extremes at eye height.
    pub const CHEEK_R: usize = 234;
    pub const CHEEK_L: usize = 454;
}

/// Run the face mesh around a detection, refining the crop once from its own
/// output like MediaPipe's tracker does.
pub fn face_landmarks(models: &Models, img: &Rgb32FImage, det: &Detection) -> Result<Landmarks, Error> {
    let eye_r = det.kps[0];
    let eye_l = det.kps[1];
    let mut angle = (eye_l.y - eye_r.y).atan2(eye_l.x - eye_r.x);
    let mut center = det.bbox.center();
    let mut size = det.bbox.w.max(det.bbox.h) * 1.5;
    let mut lm = mesh_pass(models, img, center, size, angle)?;
    for _ in 0..2 {
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        // Box in the rolled frame so the crop stays tight under rotation.
        let rot = Affine::crop(Point::default(), 0.0, 0.0, 1.0, -angle);
        for p in &lm.points[..468] {
            let q = rot.apply(*p);
            x0 = x0.min(q.x);
            y0 = y0.min(q.y);
            x1 = x1.max(q.x);
            y1 = y1.max(q.y);
        }
        let c_rot = Point::new((x0 + x1) * 0.5, (y0 + y1) * 0.5);
        center = rot.inverse().apply(c_rot);
        size = (x1 - x0).max(y1 - y0) * 1.5;
        let er = lm.points[mesh::R_EYE_OUTER];
        let el = lm.points[mesh::L_EYE_OUTER];
        angle = (el.y - er.y).atan2(el.x - er.x);
        lm = mesh_pass(models, img, center, size, angle)?;
    }
    Ok(lm)
}

fn mesh_pass(models: &Models, img: &Rgb32FImage, center: Point, size: f32, angle: f32) -> Result<Landmarks, Error> {
    let n = MESH_SIZE as f32;
    let m = Affine::crop(center, n, n, size / n, angle);
    let crop = warp_resampled(img, MESH_SIZE as u32, MESH_SIZE as u32, &m);
    let input = tract_ndarray::Array4::from_shape_fn((1, MESH_SIZE, MESH_SIZE, 3), |(_, y, x, c)| {
        crop.get_pixel(x as u32, y as u32).0[c]
    });
    let out = run(&models.face_mesh, input.into_tensor())?;
    let pts = as_slice(&out[0])?;
    if pts.len() != 478 * 3 {
        return Err(Error::Model(format!("face mesh: expected 1434 values, got {}", pts.len())));
    }
    let logit = as_slice(&out[1])?.first().copied().unwrap_or(0.0);
    let s = size / n;
    let (points, z) = pts.chunks_exact(3).map(|p| (m.apply(Point::new(p[0], p[1])), p[2] * s)).unzip();
    Ok(Landmarks { points, z, presence: 1.0 / (1.0 + (-logit).exp()) })
}
