//! Applies a synthetic colour cast to each photo and checks the illuminant
//! estimate recovers it. Usage: wb_test <r> <g> <b> photos...
use photo_core::imgops::{linear_to_srgb, srgb_to_linear, to_f32, to_u8};
use photo_core::pipeline::analyze;
use photo_core::{ModelBytes, Models};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let cast: [f32; 3] = [args[1].parse()?, args[2].parse()?, args[3].parse()?];
    let true_uv = [(cast[0] / cast[1]).ln(), (cast[2] / cast[1]).ln()];
    let r = |n: &str| std::fs::read(format!("models/{n}.onnx"));
    let models = Models::load(ModelBytes { yunet: &r("yunet")?, face_mesh: &r("face_mesh")?, modnet: &r("modnet")? })?;
    let out = std::env::var("OUT").ok();
    for path in &args[4..] {
        let mut img = to_f32(&image::open(path)?.to_rgb8());
        // First the photo's own cast, then the synthetic one on top.
        let base = analyze(&models, &img)?.illuminant.uv;
        for p in img.pixels_mut() {
            for k in 0..3 {
                p.0[k] = linear_to_srgb(srgb_to_linear(p.0[k]) * cast[k]).min(1.0);
            }
        }
        if let Some(o) = &out {
            let stem = std::path::Path::new(path).file_stem().unwrap().to_string_lossy();
            to_u8(&img).save(format!("{o}/{stem}_cast.jpg"))?;
        }
        let a = analyze(&models, &img)?;
        let est = a.illuminant.uv;
        let want = [base[0] + true_uv[0], base[1] + true_uv[1]];
        println!(
            "{path}: want ({:.2},{:.2}) got ({:.2},{:.2}) cues {:?}",
            want[0],
            want[1],
            est[0],
            est[1],
            a.illuminant.cues.iter().map(|c| format!("{} ({:.2},{:.2})", c.name, c.uv[0], c.uv[1])).collect::<Vec<_>>()
        );
    }
    Ok(())
}
