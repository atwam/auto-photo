//! Writes the refined alpha of the ICAO frame for each input as a PNG.
use photo_core::geometry::{Adjust, place};
use photo_core::imgops::to_f32;
use photo_core::pipeline::analyze;
use photo_core::spec::by_id;
use photo_core::tone::render_frame;
use photo_core::{ModelBytes, Models};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let r = |n: &str| std::fs::read(format!("models/{n}.onnx"));
    let models = Models::load(ModelBytes { yunet: &r("yunet")?, face_mesh: &r("face_mesh")?, modnet: &r("modnet")? })?;
    let out = std::env::var("OUT").unwrap_or(".".into());
    for path in std::env::args().skip(1) {
        let img = to_f32(&image::open(&path)?.to_rgb8());
        let a = analyze(&models, &img)?;
        let p = place(&a.head, by_id("icao").unwrap(), Adjust::default(), img.width(), img.height());
        let f = render_frame(&img, &a, &p, 600.0 / 25.4);
        let (w, h) = f.linear.dimensions();
        let g = image::GrayImage::from_fn(w, h, |x, y| image::Luma([(f.alpha[(y * w + x) as usize] * 255.0) as u8]));
        let stem = std::path::Path::new(&path).file_stem().unwrap().to_string_lossy().to_string();
        g.save(format!("{out}/{stem}_alpha.png"))?;
    }
    Ok(())
}
