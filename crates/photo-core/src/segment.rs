//! Person matting (MODNet), run on a fixed-size region of interest.

use crate::Error;
use crate::imgops::{Affine, Point, sample_gray, warp_resampled};
use crate::models::{MATTE_H, MATTE_W, Models, as_slice, run};
use image::Rgb32FImage;
use tract_onnx::prelude::*;

/// Alpha matte (1 = person) over a rotated ROI of the source image.
#[derive(Clone, Debug)]
pub struct Matte {
    pub alpha: Vec<f32>,
    pub w: u32,
    pub h: u32,
    /// Maps matte pixel coordinates to source-image coordinates.
    pub to_src: Affine,
}

impl Matte {
    /// Alpha at a source-image position; 0 outside the ROI.
    pub fn at_src(&self, p: Point) -> f32 {
        self.try_at_src(p).unwrap_or(0.0)
    }

    /// Alpha at a source-image position, or `None` outside the matted region.
    pub fn try_at_src(&self, p: Point) -> Option<f32> {
        let q = self.to_src.inverse().apply(p);
        if q.x < 0.0 || q.y < 0.0 || q.x > self.w as f32 || q.y > self.h as f32 {
            return None;
        }
        Some(sample_gray(&self.alpha, self.w, self.h, q.x, q.y))
    }
}

/// Run MODNet over the region of `img` that `roi` (matte pixel → source
/// pixel, for a `MATTE_W`×`MATTE_H` destination) covers.
pub fn matte(models: &Models, img: &Rgb32FImage, roi: Affine) -> Result<Matte, Error> {
    let crop = warp_resampled(img, MATTE_W as u32, MATTE_H as u32, &roi);
    let input = tract_ndarray::Array4::from_shape_fn((1, 3, MATTE_H, MATTE_W), |(_, c, y, x)| {
        crop.get_pixel(x as u32, y as u32).0[c] * 2.0 - 1.0
    });
    let out = run(&models.modnet, input.into_tensor())?;
    let alpha = as_slice(&out[0])?;
    if alpha.len() != MATTE_W * MATTE_H {
        return Err(Error::Model(format!(
            "MODNet: unexpected output size {}",
            alpha.len()
        )));
    }
    Ok(Matte {
        alpha: alpha.to_vec(),
        w: MATTE_W as u32,
        h: MATTE_H as u32,
        to_src: roi,
    })
}
