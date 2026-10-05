use clap::{Parser, ValueEnum};
use image::codecs::jpeg::{JpegEncoder, PixelDensity};
use photo_core::checks::Status;
use photo_core::geometry::Adjust;
use photo_core::imgops::{to_f32, to_u8};
use photo_core::layout::{SheetText, sheet};
use photo_core::pipeline::{analyze, produce};
use photo_core::spec::{self, Paper, SPECS};
use photo_core::tone::ToneParams;
use photo_core::{ModelBytes, Models};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Turn a portrait into passport/ID photos: a single digital photo and a
/// printable sheet of six.
#[derive(Parser)]
#[command(name = "auto-photo", version)]
struct Args {
    /// Input portrait photo(s).
    #[arg(required = true)]
    inputs: Vec<PathBuf>,
    /// Photo spec.
    #[arg(short, long, default_value = "icao", value_parser = clap::builder::PossibleValuesParser::new(SPECS.iter().map(|s| s.id)))]
    spec: String,
    /// Output directory (defaults to next to each input).
    #[arg(short, long)]
    out: Option<PathBuf>,
    /// Directory with yunet.onnx, face_mesh.onnx, modnet.onnx.
    #[arg(long, default_value = "models", env = "AUTO_PHOTO_MODELS")]
    models: PathBuf,
    /// Print paper for the sheet.
    #[arg(long, value_enum, default_value_t = PaperArg::Cm10x15)]
    paper: PaperArg,
    /// Resolution of the single digital photo, in dpi.
    #[arg(long, default_value_t = 600)]
    dpi: u32,
    /// Resolution of the print sheet, in dpi.
    #[arg(long, default_value_t = 300)]
    sheet_dpi: u32,
    /// Background replacement strength, 0 (keep) to 1 (replace).
    #[arg(long, default_value_t = 1.0)]
    background: f32,
    /// Extra brightness in stops.
    #[arg(long, default_value_t = 0.0, allow_negative_numbers = true)]
    brightness: f32,
    /// Print the full report as JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Clone, Copy, ValueEnum)]
enum PaperArg {
    #[value(name = "10x15")]
    Cm10x15,
    #[value(name = "4x6")]
    In4x6,
}

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(2),
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Returns false when any photo failed a check.
fn run() -> Result<bool, Box<dyn std::error::Error>> {
    let args = Args::parse();
    let spec = spec::by_id(&args.spec).expect("validated by clap");
    let paper = match args.paper {
        PaperArg::Cm10x15 => Paper::Cm10x15,
        PaperArg::In4x6 => Paper::In4x6,
    };
    let read = |name: &str| {
        let p = args.models.join(name);
        std::fs::read(&p).map_err(|e| format!("{}: {e} (run scripts/fetch_models.sh)", p.display()))
    };
    let models = Models::load(ModelBytes {
        yunet: &read("yunet.onnx")?,
        face_mesh: &read("face_mesh.onnx")?,
        modnet: &read("modnet.onnx")?,
    })?;
    let params = ToneParams {
        background: args.background,
        brightness_ev: args.brightness,
        ..Default::default()
    };
    let date = today();
    let mut all_ok = true;

    for input in &args.inputs {
        let img = to_f32(&open_oriented(input)?.to_rgb8());
        let a = match analyze(&models, &img) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("{}: {e}", input.display());
                all_ok = false;
                continue;
            }
        };
        let digital = produce(&img, &a, spec, Adjust::default(), &params, args.dpi as f32);
        let print = produce(
            &img,
            &a,
            spec,
            Adjust::default(),
            &params,
            args.sheet_dpi as f32,
        );
        let report = &digital.report;

        let photo_path = output_path(
            input,
            args.out.as_deref(),
            &format!("{}_photo.jpg", spec.id),
        );
        save_jpeg(&to_u8(&digital.photo), &photo_path, args.dpi)?;
        let sheet_img = sheet(
            &to_u8(&print.photo),
            spec,
            paper,
            args.sheet_dpi as f32,
            report,
            &SheetText { date: &date },
        );
        let sheet_path = output_path(
            input,
            args.out.as_deref(),
            &format!("{}_sheet.jpg", spec.id),
        );
        save_jpeg(&sheet_img, &sheet_path, args.sheet_dpi)?;

        if args.json {
            let v = serde_json::json!({
                "input": input, "photo": photo_path, "sheet": sheet_path,
                "report": report, "placement": digital.placement, "tone": digital.tone,
            });
            println!("{}", serde_json::to_string_pretty(&v)?);
        } else {
            println!(
                "{} → {}, {}",
                input.display(),
                photo_path.display(),
                sheet_path.display()
            );
            for c in &report.checks {
                let mark = match c.status {
                    Status::Pass => "\x1b[32m✓\x1b[0m",
                    Status::Warn => "\x1b[33m!\x1b[0m",
                    Status::Fail => "\x1b[31m✗\x1b[0m",
                };
                println!("  {mark} {:<22} {}", c.label, c.detail);
            }
            for n in spec.notes {
                println!("  · {n}");
            }
        }
        all_ok &= report.status != Status::Fail;
    }
    Ok(all_ok)
}

/// Decode and apply the EXIF orientation (phones often store photos sideways).
fn open_oriented(path: &Path) -> Result<image::DynamicImage, Box<dyn std::error::Error>> {
    use image::ImageDecoder;
    let mut decoder = image::ImageReader::open(path)?
        .with_guessed_format()?
        .into_decoder()?;
    let orientation = decoder.orientation()?;
    let mut img = image::DynamicImage::from_decoder(decoder)?;
    img.apply_orientation(orientation);
    Ok(img)
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

/// Today's UTC date as YYYY-MM-DD.
fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}
