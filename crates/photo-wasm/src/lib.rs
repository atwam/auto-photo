//! Browser bindings. Runs inside a Web Worker; all processing stays local.

use image::codecs::jpeg::{JpegEncoder, PixelDensity};
use image::{Rgb32FImage, RgbImage};
use photo_core::geometry::Adjust;
use photo_core::imgops::{Point, to_u8};
use photo_core::layout::{SheetText, sheet};
use photo_core::pipeline::{Analysis, Located, locate, produce, segment};
use photo_core::spec::{self, Paper, SPECS, Spec};
use photo_core::tone::ToneParams;
use photo_core::{ModelBytes, Models};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
fn start() {
    console_error_panic_hook::set_once();
}

fn err(e: impl std::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

fn to_js<T: Serialize>(v: &T) -> Result<JsValue, JsError> {
    v.serialize(&serde_wasm_bindgen::Serializer::json_compatible()).map_err(err)
}

/// All available specs, for the spec picker.
#[wasm_bindgen]
pub fn specs() -> Result<JsValue, JsError> {
    to_js(&SPECS)
}

#[wasm_bindgen]
pub struct Engine {
    models: Models,
}

#[wasm_bindgen]
impl Engine {
    #[wasm_bindgen(constructor)]
    pub fn new(yunet: &[u8], face_mesh: &[u8], modnet: &[u8]) -> Result<Engine, JsError> {
        let models = Models::load(ModelBytes { yunet, face_mesh, modnet }).map_err(err)?;
        Ok(Engine { models })
    }

    /// Fast face detection only (no landmarks), for live camera hints.
    /// Returns `[{bbox, score, kps}]`, best first.
    pub fn detect(&self, rgba: &[u8], width: u32, height: u32) -> Result<JsValue, JsError> {
        let img = rgb_from_rgba(rgba, width, height)?;
        let dets = photo_core::detect::detect_faces(&self.models, &img).map_err(err)?;
        to_js(&dets)
    }

    /// Step 1: find the face in an RGBA image (as from canvas `ImageData`).
    pub fn locate(&self, rgba: &[u8], width: u32, height: u32) -> Result<Portrait, JsError> {
        let img = rgb_from_rgba(rgba, width, height)?;
        let located = locate(&self.models, &img).map_err(err)?;
        Ok(Portrait { img, located: Some(located), analysis: None })
    }
}

#[wasm_bindgen]
pub struct Portrait {
    img: Rgb32FImage,
    located: Option<Located>,
    analysis: Option<Analysis>,
}

#[derive(Serialize)]
struct FaceInfo<'a> {
    bbox: [f32; 4],
    /// Flat x,y pairs for the 478 mesh points.
    landmarks: Vec<f32>,
    head: &'a photo_core::geometry::Head,
}

/// Options for rendering, mirrored by the TypeScript side.
#[derive(Deserialize, Default)]
#[serde(default)]
struct RenderOpts {
    adjust: Option<Adjust>,
    tone: Option<ToneParams>,
    dpi: Option<f32>,
}

#[wasm_bindgen]
impl Portrait {
    /// Face box, mesh points and head geometry (hair top once segmented).
    pub fn face(&self) -> Result<JsValue, JsError> {
        let (det, lm, head) = match (&self.analysis, &self.located) {
            (Some(a), _) => (&a.detection, &a.landmarks, &a.head),
            (None, Some(l)) => (&l.detection, &l.landmarks, &l.head),
            _ => return Err(err("no face")),
        };
        let b = det.bbox;
        to_js(&FaceInfo {
            bbox: [b.x, b.y, b.w, b.h],
            landmarks: lm.points.iter().flat_map(|p| [p.x, p.y]).collect(),
            head,
        })
    }

    /// Step 2: separate the person from the background.
    pub fn segment(&mut self, engine: &Engine) -> Result<(), JsError> {
        let loc = self.located.take().ok_or_else(|| err("already segmented"))?;
        self.analysis = Some(segment(&engine.models, &self.img, loc).map_err(err)?);
        Ok(())
    }

    /// Person matte over the whole photo, downscaled to `width`×`height`, as
    /// 8-bit alpha. Outside the matted region the value is 255 (not background).
    pub fn matte(&self, width: u32, height: u32) -> Result<Vec<u8>, JsError> {
        let a = self.analysis()?;
        let (sx, sy) = (self.img.width() as f32 / width as f32, self.img.height() as f32 / height as f32);
        let mut out = Vec::with_capacity((width * height) as usize);
        for y in 0..height {
            for x in 0..width {
                let p = Point::new((x as f32 + 0.5) * sx, (y as f32 + 0.5) * sy);
                out.push((a.matte.try_at_src(p).unwrap_or(1.0) * 255.0 + 0.5) as u8);
            }
        }
        Ok(out)
    }

    /// Render one photo for `spec` with the given options.
    pub fn render(&self, spec: &str, opts: JsValue) -> Result<Rendered, JsError> {
        let spec = find_spec(spec)?;
        let opts: RenderOpts = parse_opts(opts)?;
        let a = self.analysis()?;
        let p = produce(
            &self.img,
            a,
            spec,
            opts.adjust.unwrap_or_default(),
            &opts.tone.unwrap_or_default(),
            opts.dpi.unwrap_or(600.0),
        );
        let m = p.placement.mm_to_src;
        let info = serde_json::json!({
            "placement": p.placement,
            "tone": p.tone,
            "report": p.report,
            // Frame mm → source px, for animating the crop.
            "transform": [m.a, m.b, m.c, m.d, m.e, m.f],
        });
        Ok(Rendered::new(to_u8(&p.photo), info.to_string()))
    }

    /// Render the print sheet: six photos plus the seal.
    /// `strings`: optional localised sheet text (see `SheetText`: `compliant`,
    /// `spec_name`), as an object of key → string.
    pub fn sheet(&self, spec: &str, opts: JsValue, paper: &str, strings: JsValue) -> Result<Rendered, JsError> {
        let spec = find_spec(spec)?;
        let opts: RenderOpts = parse_opts(opts)?;
        let paper = match paper {
            "4x6" => Paper::In4x6,
            _ => Paper::Cm10x15,
        };
        let strings: std::collections::HashMap<String, String> = if strings.is_undefined() || strings.is_null() {
            Default::default()
        } else {
            serde_wasm_bindgen::from_value(strings).map_err(err)?
        };
        let dpi = opts.dpi.unwrap_or(300.0);
        let a = self.analysis()?;
        let p = produce(&self.img, a, spec, opts.adjust.unwrap_or_default(), &opts.tone.unwrap_or_default(), dpi);
        let img = sheet(&to_u8(&p.photo), spec, paper, dpi, &SheetText { strings });
        Ok(Rendered::new(img, serde_json::json!({ "report": p.report }).to_string()))
    }
}

impl Portrait {
    fn analysis(&self) -> Result<&Analysis, JsError> {
        self.analysis.as_ref().ok_or_else(|| err("call segment() first"))
    }
}

fn rgb_from_rgba(rgba: &[u8], width: u32, height: u32) -> Result<Rgb32FImage, JsError> {
    if rgba.len() != (width * height * 4) as usize {
        return Err(err("pixel buffer does not match dimensions"));
    }
    let mut img = Rgb32FImage::new(width, height);
    for (o, px) in img.pixels_mut().zip(rgba.chunks_exact(4)) {
        o.0 = [px[0] as f32 / 255.0, px[1] as f32 / 255.0, px[2] as f32 / 255.0];
    }
    Ok(img)
}

fn find_spec(id: &str) -> Result<&'static Spec, JsError> {
    spec::by_id(id).ok_or_else(|| err(format!("unknown spec {id}")))
}

fn parse_opts(v: JsValue) -> Result<RenderOpts, JsError> {
    if v.is_undefined() || v.is_null() {
        return Ok(RenderOpts::default());
    }
    serde_wasm_bindgen::from_value(v).map_err(err)
}

/// An RGB image result plus JSON info.
#[wasm_bindgen]
pub struct Rendered {
    img: RgbImage,
    info: String,
}

impl Rendered {
    fn new(img: RgbImage, info: String) -> Self {
        Rendered { img, info }
    }
}

#[wasm_bindgen]
impl Rendered {
    #[wasm_bindgen(getter)]
    pub fn width(&self) -> u32 {
        self.img.width()
    }
    #[wasm_bindgen(getter)]
    pub fn height(&self) -> u32 {
        self.img.height()
    }
    /// JSON string with placement / tone / report.
    #[wasm_bindgen(getter)]
    pub fn info(&self) -> String {
        self.info.clone()
    }
    /// RGBA pixels, ready for `new ImageData(...)`.
    pub fn rgba(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity((self.img.width() * self.img.height() * 4) as usize);
        for p in self.img.pixels() {
            out.extend_from_slice(&[p.0[0], p.0[1], p.0[2], 255]);
        }
        out
    }
    /// JPEG with the DPI recorded, so printers size it correctly.
    pub fn jpeg(&self, quality: u8, dpi: u16) -> Result<Vec<u8>, JsError> {
        let mut buf = Vec::new();
        let mut enc = JpegEncoder::new_with_quality(&mut buf, quality);
        enc.set_pixel_density(PixelDensity::dpi(dpi));
        enc.encode_image(&self.img).map_err(err)?;
        Ok(buf)
    }
}
