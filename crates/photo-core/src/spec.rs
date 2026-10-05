//! Per-country photo specifications. Everything country-specific lives here as
//! data, so adding a country means adding one entry.

use serde::Serialize;

/// How a spec measures head height.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum HeadTop {
    /// Chin to the top of the hair.
    Hair,
    /// Chin to the top of the skull, hair excluded (ICAO "crown": where the
    /// top of the head would be without hair).
    Skull,
}

#[derive(Clone, Debug, Serialize)]
pub struct Spec {
    pub id: &'static str,
    pub name: &'static str,
    pub width_mm: f32,
    pub height_mm: f32,
    /// Accepted head height range (chin to `head_top`), mm.
    pub head_min_mm: f32,
    pub head_max_mm: f32,
    pub head_top: HeadTop,
    /// Accepted eye-line height, measured from the bottom edge, mm.
    pub eyes_min_mm: f32,
    pub eyes_max_mm: f32,
    /// Background colour the pipeline aims for (sRGB).
    pub background: [u8; 3],
    /// Whether a white background is explicitly forbidden.
    pub white_background_forbidden: bool,
    /// Minimum pixel size accepted for digital upload, if the authority states one.
    pub digital_min_px: Option<(u32, u32)>,
    /// Accepted digital file size range in bytes, if stated.
    pub digital_bytes: Option<(usize, usize)>,
    /// False when the authority no longer accepts printed photos for passports/ID.
    pub print_accepted: bool,
    pub notes: &'static [&'static str],
    pub sources: &'static [&'static str],
}

impl Spec {
    pub fn head_target_mm(&self) -> f32 {
        (self.head_min_mm + self.head_max_mm) * 0.5
    }
    pub fn aspect(&self) -> f32 {
        self.width_mm / self.height_mm
    }
}

const LIGHT_GREY: [u8; 3] = [224, 224, 222];

pub static SPECS: &[Spec] = &[
    Spec {
        id: "icao",
        name: "ICAO / Schengen 35×45",
        width_mm: 35.0,
        height_mm: 45.0,
        head_min_mm: 32.0,
        head_max_mm: 36.0,
        head_top: HeadTop::Skull,
        // ISO/IEC 19794-5: eye line at 50–70% of the height from the bottom.
        eyes_min_mm: 22.5,
        eyes_max_mm: 31.5,
        background: LIGHT_GREY,
        white_background_forbidden: false,
        digital_min_px: None,
        digital_bytes: None,
        print_accepted: true,
        notes: &[],
        sources: &["ICAO Doc 9303 Part 3; ISO/IEC 19794-5"],
    },
    Spec {
        id: "fr",
        name: "France (passeport, CNI, titre de séjour)",
        width_mm: 35.0,
        height_mm: 45.0,
        head_min_mm: 32.0,
        head_max_mm: 36.0,
        head_top: HeadTop::Skull,
        eyes_min_mm: 22.5,
        eyes_max_mm: 31.5,
        background: [218, 222, 226],
        white_background_forbidden: true,
        digital_min_px: None,
        digital_bytes: None,
        print_accepted: true,
        notes: &[
            "Light uniform background (light grey or light blue); white is forbidden.",
            "Online applications require an ANTS e-photo code from a certified booth or photographer.",
        ],
        sources: &["https://www.service-public.fr (normes photo d'identité, ANTS)"],
    },
    Spec {
        id: "uk",
        name: "United Kingdom passport",
        width_mm: 35.0,
        height_mm: 45.0,
        head_min_mm: 29.0,
        head_max_mm: 34.0,
        head_top: HeadTop::Skull,
        eyes_min_mm: 22.5,
        eyes_max_mm: 31.5,
        background: [226, 224, 218],
        white_background_forbidden: true,
        digital_min_px: Some((600, 750)),
        digital_bytes: Some((50 * 1024, 10 * 1024 * 1024)),
        print_accepted: true,
        notes: &[
            "Plain cream or light grey background (5–10% grey).",
            "Head measured to the crown (top of head, not top of hair); hair may be cropped.",
            "Digital photos must be 'unaltered by computer software': keep background edits minimal.",
        ],
        sources: &["https://www.gov.uk/photos-for-passports"],
    },
    Spec {
        id: "de",
        name: "Germany (Reisepass, Personalausweis)",
        width_mm: 35.0,
        height_mm: 45.0,
        head_min_mm: 32.0,
        head_max_mm: 36.0,
        head_top: HeadTop::Skull,
        eyes_min_mm: 22.5,
        eyes_max_mm: 31.5,
        background: LIGHT_GREY,
        white_background_forbidden: false,
        digital_min_px: None,
        digital_bytes: None,
        print_accepted: false,
        notes: &[
            "Since 1 May 2025 (transition ended 31 July 2025) passport and ID photos must be taken \
             digitally at the authority or by a certified photographer; printed photos are not accepted.",
            "Neutral, ideally light grey background.",
        ],
        sources: &["Bundesdruckerei Fotomustertafel; PassV Anlage 8"],
    },
];

pub fn by_id(id: &str) -> Option<&'static Spec> {
    SPECS.iter().find(|s| s.id.eq_ignore_ascii_case(id))
}

/// Print paper the sheet is laid out on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Paper {
    /// 10×15 cm.
    Cm10x15,
    /// 4×6 in (101.6×152.4 mm).
    In4x6,
}

impl Paper {
    /// Landscape size in mm.
    pub fn size_mm(self) -> (f32, f32) {
        match self {
            Paper::Cm10x15 => (150.0, 100.0),
            Paper::In4x6 => (152.4, 101.6),
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Paper::Cm10x15 => "10×15 cm",
            Paper::In4x6 => "4×6 in",
        }
    }
}
