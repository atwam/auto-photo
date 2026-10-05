//! End-to-end analysis of one portrait.

use crate::Error;
use crate::checks::{self, Report};
use crate::detect::{Detection, Landmarks, detect_faces, face_landmarks};
use crate::geometry::{Adjust, Head, Placement, place};
use crate::imgops::{Affine, Point};
use crate::models::{MATTE_H, MATTE_W, Models};
use crate::segment::{Matte, matte};
use crate::spec::{HeadTop, SPECS, Spec};
use crate::tone::{self, ToneParams, ToneReport};
use crate::whitebalance::{self, Illuminant};
use image::Rgb32FImage;

/// Faces smaller than this fraction of the main face's area are ignored
/// (people in the background, posters…).
const SECONDARY_FACE_AREA: f32 = 0.2;

/// Matte region height relative to the spec frame at default zoom; leaves
/// room for zooming out a little in the UI.
const MATTE_COVER: f32 = 1.4;

/// The face found in a photo, before background segmentation.
#[derive(Clone)]
pub struct Located {
    pub detection: Detection,
    pub landmarks: Landmarks,
    pub head: Head,
}

pub struct Analysis {
    pub detection: Detection,
    pub landmarks: Landmarks,
    pub head: Head,
    pub matte: Matte,
    /// Light colour estimated on the original photo.
    pub illuminant: Illuminant,
}

pub fn analyze(models: &Models, img: &Rgb32FImage) -> Result<Analysis, Error> {
    segment(models, img, locate(models, img)?)
}

/// Step 1: find exactly one face and its landmarks.
pub fn locate(models: &Models, img: &Rgb32FImage) -> Result<Located, Error> {
    let dets = detect_faces(models, img)?;
    let main = dets.first().ok_or(Error::NoFace)?;
    let area = |d: &Detection| d.bbox.w * d.bbox.h;
    let significant = dets.iter().filter(|d| area(d) >= area(main) * SECONDARY_FACE_AREA).count();
    if significant > 1 {
        return Err(Error::MultipleFaces(significant));
    }
    let landmarks = face_landmarks(models, img, main)?;
    let head = Head::from_landmarks(&landmarks);
    Ok(Located { detection: main.clone(), landmarks, head })
}

/// Step 2: matte the person over a region larger than any spec's frame, and
/// refine the head top with it.
pub fn segment(models: &Models, img: &Rgb32FImage, loc: Located) -> Result<Analysis, Error> {
    let Located { detection, landmarks, mut head } = loc;
    let mut spec = SPECS[0].clone();
    spec.head_top = HeadTop::Skull;
    let p = place(&head, &spec, Adjust::default(), img.width(), img.height());
    let center = p.mm_to_src.apply(Point::new(spec.width_mm * 0.5, spec.height_mm * 0.5));
    let roi_h = spec.height_mm * MATTE_COVER * p.src_px_per_mm;
    let roi = Affine::crop(center, MATTE_W as f32, MATTE_H as f32, roi_h / MATTE_H as f32, head.roll);
    let matte = matte(models, img, roi)?;
    head.refine_with_matte(&matte);
    let illuminant = whitebalance::estimate(img, &landmarks, &matte);
    Ok(Analysis { detection, landmarks, head, matte, illuminant })
}

/// One rendered, corrected and checked photo.
pub struct Produced {
    /// Final sRGB photo.
    pub photo: Rgb32FImage,
    pub placement: Placement,
    pub tone: ToneReport,
    pub report: Report,
}

/// Place, render at `dpi`, tone-correct and check.
pub fn produce(
    img: &Rgb32FImage,
    a: &Analysis,
    spec: &Spec,
    adjust: Adjust,
    params: &ToneParams,
    dpi: f32,
) -> Produced {
    let placement = place(&a.head, spec, adjust, img.width(), img.height());
    let frame = tone::render_frame(img, a, &placement, dpi / 25.4);
    let (photo, tone) = tone::apply(&frame, params.background_color.unwrap_or(spec.background), params);
    let report = checks::run(spec, a, &placement, &frame, &photo, params);
    Produced { photo, placement, tone, report }
}
