use clap::Parser;
use image::codecs::jpeg::{JpegEncoder, PixelDensity};
use photo_core::geometry::{Adjust, place};
use photo_core::imgops::{to_f32, to_u8};
use photo_core::pipeline::analyze;
use photo_core::spec::{self, SPECS};
use photo_core::tone::{self, ToneParams};
use photo_core::{ModelBytes, Models};
use std::path::{Path, PathBuf};

/// Turn a portrait into passport/ID photos.
#[derive(Parser)]
#[command(name = "auto-photo", version)]
struct Args {
    /// Input portrait photo(s).
    #[arg(required = true)]
    inputs: Vec<PathBuf>,
    /// Photo spec: icao, fr, uk, de.
    #[arg(short, long, default_value = "icao")]
    spec: String,
    /// Output directory (defaults to next to each input).
    #[arg(short, long)]
    out: Option<PathBuf>,
    /// Directory with yunet.onnx, face_mesh.onnx, modnet.onnx.
    #[arg(long, default_value = "models", env = "AUTO_PHOTO_MODELS")]
    models: PathBuf,
    /// Resolution of the single digital photo, in dpi.
    #[arg(long, default_value_t = 600)]
    dpi: u32,
    /// Background replacement strength, 0 (keep) to 1 (replace).
    #[arg(long, default_value_t = 1.0)]
    background: f32,
    /// Extra brightness in stops.
    #[arg(long, default_value_t = 0.0, allow_negative_numbers = true)]
    brightness: f32,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let spec = spec::by_id(&args.spec).ok_or_else(|| {
        format!(
            "unknown spec '{}'; known: {}",
            args.spec,
            SPECS.iter().map(|s| s.id).collect::<Vec<_>>().join(", ")
        )
    })?;
    let read = |name: &str| {
        std::fs::read(args.models.join(name))
            .map_err(|e| format!("{}: {e}", args.models.join(name).display()))
    };
    let models = Models::load(ModelBytes {
        yunet: &read("yunet.onnx")?,
        face_mesh: &read("face_mesh.onnx")?,
        modnet: &read("modnet.onnx")?,
    })?;

    for input in &args.inputs {
        let img = to_f32(&image::open(input)?.to_rgb8());
        let a = analyze(&models, &img)?;
        let placement = place(&a.head, spec, Adjust::default(), img.width(), img.height());
        let ppmm = args.dpi as f32 / 25.4;
        let frame = tone::render_frame(&img, &a, &placement, ppmm);
        let params = ToneParams {
            background: args.background,
            brightness_ev: args.brightness,
            ..Default::default()
        };
        let (photo, report) = tone::apply(&frame, spec.background, &params);
        let out = output_path(
            input,
            args.out.as_deref(),
            &format!("{}_photo.jpg", spec.id),
        );
        save_jpeg(&to_u8(&photo), &out, args.dpi)?;
        println!("{}: {}", input.display(), out.display());
        println!("  placement {}", serde_json::to_string(&placement)?);
        println!("  tone {}", serde_json::to_string(&report)?);
    }
    Ok(())
}

fn output_path(input: &Path, dir: Option<&Path>, suffix: &str) -> PathBuf {
    let stem = input.file_stem().unwrap_or_default().to_string_lossy();
    let dir = dir
        .map(Path::to_path_buf)
        .unwrap_or_else(|| input.parent().unwrap_or(Path::new(".")).to_path_buf());
    dir.join(format!("{stem}_{suffix}"))
}

fn save_jpeg(
    img: &image::RgbImage,
    path: &Path,
    dpi: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut enc = JpegEncoder::new_with_quality(file, 95);
    enc.set_pixel_density(PixelDensity::dpi(dpi as u16));
    enc.encode_image(img)?;
    Ok(())
}
