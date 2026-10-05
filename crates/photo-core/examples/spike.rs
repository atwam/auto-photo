//! Runs the three models on photos and writes debug overlays next to them.
use image::Rgb;
use photo_core::detect::{detect_faces, face_landmarks, mesh};
use photo_core::imgops::{Affine, Point, to_f32, to_u8};
use photo_core::models::{MATTE_H, MATTE_W};
use photo_core::{ModelBytes, Models, segment};
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let modnet = std::env::var("MODNET").unwrap_or("models/modnet_quantized.onnx".into());
    let t = Instant::now();
    let models = Models::load(ModelBytes {
        yunet: &std::fs::read("models/yunet_2023mar.onnx")?,
        face_mesh: &std::fs::read("models/face_landmarks.onnx")?,
        modnet: &std::fs::read(&modnet)?,
    })?;
    println!("models loaded in {:?}", t.elapsed());
    for path in std::env::args().skip(1) {
        let img = to_f32(&image::open(&path)?.to_rgb8());
        let t = Instant::now();
        let dets = detect_faces(&models, &img)?;
        let t_det = t.elapsed();
        let Some(det) = dets.first() else {
            println!("{path}: no face");
            continue;
        };
        let t = Instant::now();
        let lm = face_landmarks(&models, &img, det)?;
        let t_lm = t.elapsed();
        let chin = lm.points[mesh::CHIN];
        let fh = lm.points[mesh::FOREHEAD_TOP];
        let face_h = chin.dist(fh);
        let er = lm.points[mesh::R_IRIS];
        let el = lm.points[mesh::L_IRIS];
        let angle = (el.y - er.y).atan2(el.x - er.x);
        // ROI: 3:4 box centred slightly above the face, face ~1/3 of its height.
        let scale = face_h * 3.2 / MATTE_H as f32;
        let c = fh.mid(chin);
        let roi = Affine::crop(
            Point::new(c.x, c.y + face_h * 0.3),
            MATTE_W as f32,
            MATTE_H as f32,
            scale,
            angle,
        );
        let t = Instant::now();
        let matte = segment::matte(&models, &img, roi)?;
        let t_mat = t.elapsed();
        println!(
            "{path}: {} faces, score {:.2}, presence {:.2}, det {t_det:?} mesh {t_lm:?} matte {t_mat:?}",
            dets.len(),
            det.score,
            lm.presence
        );

        let mut out = to_u8(&img);
        let (w, h) = out.dimensions();
        for (x, y, px) in out.enumerate_pixels_mut() {
            let a = matte.at_src(Point::new(x as f32 + 0.5, y as f32 + 0.5));
            px.0 = [
                (px.0[0] as f32 * (0.4 + 0.6 * a) + 140.0 * (1.0 - a) * 0.6) as u8,
                (px.0[1] as f32 * (0.4 + 0.6 * a)) as u8,
                (px.0[2] as f32 * (0.4 + 0.6 * a)) as u8,
            ];
        }
        let r = (w.max(h) / 400).max(1) as i32;
        let mut dot = |p: Point, col: [u8; 3], r: i32| {
            for dy in -r..=r {
                for dx in -r..=r {
                    let (x, y) = (p.x as i32 + dx, p.y as i32 + dy);
                    if x >= 0 && y >= 0 && (x as u32) < w && (y as u32) < h {
                        out.put_pixel(x as u32, y as u32, Rgb(col));
                    }
                }
            }
        };
        for p in &lm.points[..468] {
            dot(*p, [0, 255, 0], r / 2);
        }
        for p in det.kps {
            dot(p, [255, 255, 0], r * 2);
        }
        for i in [mesh::CHIN, mesh::FOREHEAD_TOP, mesh::R_IRIS, mesh::L_IRIS] {
            dot(lm.points[i], [255, 0, 255], r * 3);
        }
        let outp = format!("{}.debug.jpg", path.trim_end_matches(".jpg"));
        out.save(&outp)?;
    }
    Ok(())
}
