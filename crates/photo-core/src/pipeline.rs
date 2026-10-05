//! End-to-end analysis of one portrait.

use crate::Error;
use crate::detect::{Detection, Landmarks, detect_faces, face_landmarks};
use crate::geometry::{Adjust, Head, place};
use crate::imgops::{Affine, Point};
use crate::models::{MATTE_H, MATTE_W, Models};
use crate::segment::{Matte, matte};
use crate::spec::{HeadTop, SPECS};
use image::Rgb32FImage;

/// Faces smaller than this fraction of the main face's area are ignored
/// (people in the background, posters…).
const SECONDARY_FACE_AREA: f32 = 0.2;

pub struct Analysis {
    pub detection: Detection,
    pub landmarks: Landmarks,
    pub head: Head,
    pub matte: Matte,
}

pub fn analyze(models: &Models, img: &Rgb32FImage) -> Result<Analysis, Error> {
    let dets = detect_faces(models, img)?;
    let main = dets.first().ok_or(Error::NoFace)?;
    let area = |d: &Detection| d.bbox.w * d.bbox.h;
    let significant = dets
        .iter()
        .filter(|d| area(d) >= area(main) * SECONDARY_FACE_AREA)
        .count();
    if significant > 1 {
        return Err(Error::MultipleFaces(significant));
    }
    let landmarks = face_landmarks(models, img, main)?;
    let mut head = Head::from_landmarks(&landmarks);

    // Matte a region a bit larger than any spec's frame would be.
    let mut spec = SPECS[0].clone();
    spec.head_top = HeadTop::Skull;
    let p = place(&head, &spec, Adjust::default(), img.width(), img.height());
    let center = p
        .mm_to_src
        .apply(Point::new(spec.width_mm * 0.5, spec.height_mm * 0.5));
    let roi_h = spec.height_mm * 1.3 * p.src_px_per_mm;
    let roi = Affine::crop(
        center,
        MATTE_W as f32,
        MATTE_H as f32,
        roi_h / MATTE_H as f32,
        head.roll,
    );
    let matte = matte(models, img, roi)?;
    head.refine_with_matte(&matte);

    Ok(Analysis {
        detection: main.clone(),
        landmarks,
        head,
        matte,
    })
}
