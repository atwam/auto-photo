//! Prints eye→hair-top / eye→chin ratios for skull-height calibration.
use photo_core::imgops::to_f32;
use photo_core::pipeline::analyze;
use photo_core::{ModelBytes, Models};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let r = |n: &str| std::fs::read(format!("models/{n}.onnx"));
    let models = Models::load(ModelBytes {
        yunet: &r("yunet")?,
        face_mesh: &r("face_mesh")?,
        modnet: &r("modnet")?,
    })?;
    for path in std::env::args().skip(1) {
        let img = to_f32(&image::open(&path)?.to_rgb8());
        let a = analyze(&models, &img)?;
        let h = &a.head;
        let d =
            |p: photo_core::imgops::Point| (p.x - h.eyes.x) * h.up.x + (p.y - h.eyes.y) * h.up.y;
        let ec = -d(h.chin);
        let fh = d(a.landmarks.points[photo_core::detect::mesh::FOREHEAD_TOP]);
        println!(
            "{path}: hair/ec {:.3}  forehead10/ec {:.3}",
            h.hair_top.map(|p| d(p) / ec).unwrap_or(f32::NAN),
            fh / ec
        );
    }
    Ok(())
}
