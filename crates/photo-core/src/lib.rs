//! Core passport-photo pipeline: detection, geometry, tone, checks, layout.
//! Pure computation with no I/O so it runs both natively and in wasm.

pub mod checks;
pub mod detect;
pub mod draw;
pub mod geometry;
pub mod imgops;
pub mod layout;
pub mod models;
pub mod pipeline;
pub mod segment;
pub mod spec;
pub mod tone;

pub use models::{ModelBytes, Models};

#[derive(Debug)]
pub enum Error {
    Model(String),
    Image(String),
    NoFace,
    MultipleFaces(usize),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Model(m) => write!(f, "model error: {m}"),
            Error::Image(m) => write!(f, "image error: {m}"),
            Error::NoFace => write!(f, "no face found in the photo"),
            Error::MultipleFaces(n) => {
                write!(f, "{n} faces found; the photo must show exactly one person")
            }
        }
    }
}

impl std::error::Error for Error {}
