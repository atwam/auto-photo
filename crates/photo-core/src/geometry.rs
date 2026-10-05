//! Head measurements in the source photo, and the crop that places the head
//! inside a spec's frame.

use crate::detect::{Landmarks, mesh};
use crate::imgops::{Affine, Point};
use crate::segment::Matte;
use crate::spec::{HeadTop, Spec};
use serde::Serialize;

// Skull top is estimated in units of the eye-line → upper-forehead distance
// (mesh point 10), which unlike eye→chin doesn't change when smiling.
// Calibrated on portraits; the specs' ±2 mm head tolerance absorbs the rest.
/// Skull top above the eyes, from face proportions alone.
const SKULL_FROM_FOREHEAD: f32 = 2.0;
/// Typical hair thickness on top of the skull.
const HAIR_THICKNESS: f32 = 0.17;
/// Anatomical bounds on the skull estimate.
const SKULL_MIN: f32 = 1.85;
const SKULL_MAX: f32 = 2.3;
/// Share of the free vertical space (frame minus head) placed above the head.
const TOP_SHARE: f32 = 0.3;
/// Keep the eye line this far inside the spec's range.
const EYE_MARGIN_MM: f32 = 1.0;

/// Key head points in source-image pixels.
#[derive(Clone, Debug, Serialize)]
pub struct Head {
    pub chin: Point,
    /// Midpoint of the two iris centres.
    pub eyes: Point,
    pub eye_r: Point,
    pub eye_l: Point,
    /// Unit vector along the face axis pointing to the top of the head.
    pub up: Point,
    /// Eye-line roll in radians (0 = level).
    pub roll: f32,
    pub skull_top: Point,
    /// Top of the hair from the matte, when available.
    pub hair_top: Option<Point>,
    /// Face width (cheek to cheek) in source pixels.
    pub face_width: f32,
    /// Eye line to upper forehead along `up`, the unit for skull estimates.
    pub forehead: f32,
}

impl Head {
    pub fn from_landmarks(lm: &Landmarks) -> Self {
        let p = &lm.points;
        let eye_r = p[mesh::R_IRIS];
        let eye_l = p[mesh::L_IRIS];
        let eyes = eye_r.mid(eye_l);
        let chin = p[mesh::CHIN];
        let roll = (eye_l.y - eye_r.y).atan2(eye_l.x - eye_r.x);
        let up = Point::new(roll.sin(), -roll.cos());
        let forehead = dot(sub(p[mesh::FOREHEAD_TOP], eyes), up);
        Head {
            chin,
            eyes,
            eye_r,
            eye_l,
            up,
            roll,
            skull_top: add(eyes, scale(up, forehead * SKULL_FROM_FOREHEAD)),
            hair_top: None,
            face_width: p[mesh::CHEEK_R].dist(p[mesh::CHEEK_L]),
            forehead,
        }
    }

    /// Find the top of the hair by walking up the face axis in the matte, and
    /// blend it into the skull estimate (hair top minus typical hair).
    pub fn refine_with_matte(&mut self, matte: &Matte) {
        let across = Point::new(-self.up.y, self.up.x);
        let reach = dot(sub(self.eyes, self.chin), self.up) * 2.5;
        let half = self.face_width * 0.4;
        let step = (reach / 400.0).max(0.5);
        let mut best: Option<f32> = None;
        let mut s = -half;
        while s <= half {
            let base = add(self.eyes, scale(across, s));
            let mut t = 0.0;
            let mut last_in = None;
            while t < reach {
                if matte.at_src(add(base, scale(self.up, t))) >= 0.5 {
                    last_in = Some(t);
                }
                t += step;
            }
            if let Some(t) = last_in {
                best = Some(best.map_or(t, |b: f32| b.max(t)));
            }
            s += half / 12.0;
        }
        if let Some(t) = best {
            self.hair_top = Some(add(self.eyes, scale(self.up, t)));
            let hair = t / self.forehead;
            let k = (0.5 * (SKULL_FROM_FOREHEAD + hair - HAIR_THICKNESS))
                .clamp(SKULL_MIN, SKULL_MAX)
                .min(hair);
            self.skull_top = add(self.eyes, scale(self.up, k * self.forehead));
        }
    }

    pub fn top(&self, which: HeadTop) -> Point {
        match which {
            HeadTop::Skull => self.skull_top,
            HeadTop::Hair => self.hair_top.unwrap_or(self.skull_top),
        }
    }
}

/// Manual nudges on top of the automatic placement.
#[derive(Clone, Copy, Debug, Serialize, serde::Deserialize)]
pub struct Adjust {
    /// Move the subject right / down in the frame, in mm.
    pub dx_mm: f32,
    pub dy_mm: f32,
    /// > 1 makes the head bigger.
    pub zoom: f32,
    /// Extra rotation in degrees, clockwise.
    pub rotate_deg: f32,
}

impl Default for Adjust {
    fn default() -> Self {
        Adjust {
            dx_mm: 0.0,
            dy_mm: 0.0,
            zoom: 1.0,
            rotate_deg: 0.0,
        }
    }
}

/// Where the frame sits in the source image, plus the resulting measurements.
#[derive(Clone, Debug, Serialize)]
pub struct Placement {
    /// Maps frame millimetres (origin top-left, y down) to source pixels.
    #[serde(skip)]
    pub mm_to_src: Affine,
    pub width_mm: f32,
    pub height_mm: f32,
    /// Chin to the spec's head top, in mm.
    pub head_mm: f32,
    /// Eye line above the bottom edge, in mm.
    pub eyes_from_bottom_mm: f32,
    /// Horizontal offset of the face axis from the frame centre, in mm.
    pub center_offset_mm: f32,
    /// Hair top below the top edge, in mm (negative = cut off).
    pub hair_margin_mm: Option<f32>,
    /// Source pixels per mm; low values mean the photo is upscaled.
    pub src_px_per_mm: f32,
    /// Fraction of the frame that falls outside the source photo.
    pub outside_fraction: f32,
}

/// Place the head at the spec's target size, then shift vertically if needed
/// to bring the eye line inside the spec's range.
pub fn place(head: &Head, spec: &Spec, adjust: Adjust, src_w: u32, src_h: u32) -> Placement {
    let p = place_raw(head, spec, adjust, src_w, src_h);
    let lo = spec.eyes_min_mm + EYE_MARGIN_MM;
    let hi = spec.eyes_max_mm - EYE_MARGIN_MM;
    let shift = if p.eyes_from_bottom_mm < lo {
        lo - p.eyes_from_bottom_mm
    } else if p.eyes_from_bottom_mm > hi {
        hi - p.eyes_from_bottom_mm
    } else {
        return p;
    };
    // Positive shift moves the subject up, i.e. negative dy.
    place_raw(
        head,
        spec,
        Adjust {
            dy_mm: adjust.dy_mm - shift,
            ..adjust
        },
        src_w,
        src_h,
    )
}

fn place_raw(head: &Head, spec: &Spec, adjust: Adjust, src_w: u32, src_h: u32) -> Placement {
    let top = head.top(spec.head_top);
    let head_px = dot(sub(top, head.chin), head.up);
    let target = spec.head_target_mm() * adjust.zoom;
    let s = head_px / target;
    let top_y = (spec.height_mm - target) * TOP_SHARE;
    // Frame centre sits below the head top on the face axis.
    let down = scale(head.up, -1.0);
    let across = Point::new(-head.up.y, head.up.x);
    let center = add(
        add(
            top,
            scale(down, (spec.height_mm * 0.5 - top_y - adjust.dy_mm) * s),
        ),
        scale(across, -adjust.dx_mm * s),
    );
    let angle = head.roll + adjust.rotate_deg.to_radians();
    let mm_to_src = Affine::crop(center, spec.width_mm, spec.height_mm, s, angle);
    let to_mm = mm_to_src.inverse();

    let chin = to_mm.apply(head.chin);
    let top_mm = to_mm.apply(top);
    let eyes = to_mm.apply(head.eyes);
    let hair_margin_mm = head.hair_top.map(|h| to_mm.apply(h).y);

    // Sample a grid to see how much of the frame lies outside the photo.
    let n = 24;
    let mut outside = 0;
    for j in 0..n {
        for i in 0..n {
            let p = mm_to_src.apply(Point::new(
                (i as f32 + 0.5) * spec.width_mm / n as f32,
                (j as f32 + 0.5) * spec.height_mm / n as f32,
            ));
            if p.x < 0.0 || p.y < 0.0 || p.x >= src_w as f32 || p.y >= src_h as f32 {
                outside += 1;
            }
        }
    }

    Placement {
        mm_to_src,
        width_mm: spec.width_mm,
        height_mm: spec.height_mm,
        head_mm: chin.y - top_mm.y,
        eyes_from_bottom_mm: spec.height_mm - eyes.y,
        center_offset_mm: eyes.x - spec.width_mm * 0.5,
        hair_margin_mm,
        src_px_per_mm: s,
        outside_fraction: outside as f32 / (n * n) as f32,
    }
}

impl Placement {
    /// Affine from output pixels (at `px_per_mm`) to source pixels.
    pub fn px_to_src(&self, px_per_mm: f32) -> Affine {
        let m = &self.mm_to_src;
        let k = 1.0 / px_per_mm;
        Affine {
            a: m.a * k,
            b: m.b * k,
            c: m.c,
            d: m.d * k,
            e: m.e * k,
            f: m.f,
        }
    }
}

fn dot(a: Point, b: Point) -> f32 {
    a.x * b.x + a.y * b.y
}
fn sub(a: Point, b: Point) -> Point {
    Point::new(a.x - b.x, a.y - b.y)
}
fn add(a: Point, b: Point) -> Point {
    Point::new(a.x + b.x, a.y + b.y)
}
fn scale(a: Point, k: f32) -> Point {
    Point::new(a.x * k, a.y * k)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::by_id;

    /// Upright head: eyes at y=500, chin 300 px below, skull 280 px above.
    fn head() -> Head {
        Head {
            chin: Point::new(500.0, 800.0),
            eyes: Point::new(500.0, 500.0),
            eye_r: Point::new(440.0, 500.0),
            eye_l: Point::new(560.0, 500.0),
            up: Point::new(0.0, -1.0),
            roll: 0.0,
            skull_top: Point::new(500.0, 220.0),
            hair_top: Some(Point::new(500.0, 190.0)),
            face_width: 300.0,
            forehead: 140.0,
        }
    }

    #[test]
    fn places_head_at_target_size_and_centred() {
        for spec in crate::spec::SPECS {
            let p = place(&head(), spec, Adjust::default(), 1000, 1400);
            assert!(
                (p.head_mm - spec.head_target_mm()).abs() < 0.01,
                "{}: {}",
                spec.id,
                p.head_mm
            );
            assert!(p.center_offset_mm.abs() < 0.01);
            assert!(
                p.eyes_from_bottom_mm >= spec.eyes_min_mm
                    && p.eyes_from_bottom_mm <= spec.eyes_max_mm
            );
            assert_eq!(p.outside_fraction, 0.0);
        }
    }

    #[test]
    fn zoom_and_rotation_adjustments() {
        let spec = by_id("icao").unwrap();
        let p = place(
            &head(),
            spec,
            Adjust {
                zoom: 1.05,
                ..Default::default()
            },
            1000,
            1400,
        );
        assert!((p.head_mm - 34.0 * 1.05).abs() < 0.01);
        // Rotating the frame keeps the head size (measured along the frame's vertical).
        let p = place(
            &head(),
            spec,
            Adjust {
                rotate_deg: 3.0,
                ..Default::default()
            },
            1000,
            1400,
        );
        assert!((p.head_mm - 34.0 * 3f32.to_radians().cos()).abs() < 0.05);
    }

    #[test]
    fn reports_frame_outside_photo() {
        let spec = by_id("icao").unwrap();
        let p = place(&head(), spec, Adjust::default(), 600, 900);
        assert!(p.outside_fraction > 0.0);
    }
}
