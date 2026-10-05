//! Loading of the ONNX models used by the pipeline.
//!
//! Models are passed in as bytes so the same code works natively (read from
//! disk) and in the browser (fetched and cached by the page).

use crate::Error;
use tract_onnx::prelude::*;

pub type Plan = std::sync::Arc<TypedRunnableModel>;

/// Fixed input sizes; models are optimised once for these shapes.
pub const YUNET_SIZE: usize = 640;
pub const MESH_SIZE: usize = 256;
pub const MATTE_W: usize = 480;
pub const MATTE_H: usize = 640;

pub struct ModelBytes<'a> {
    pub yunet: &'a [u8],
    pub face_mesh: &'a [u8],
    pub modnet: &'a [u8],
}

pub struct Models {
    pub(crate) yunet: Plan,
    pub(crate) face_mesh: Plan,
    pub(crate) modnet: Plan,
}

fn load(bytes: &[u8], shape: &[usize]) -> Result<Plan, Error> {
    let run = || -> TractResult<Plan> {
        tract_onnx::onnx()
            .with_ignore_value_info(true)
            .with_ignore_output_shapes(true)
            .model_for_read(&mut std::io::Cursor::new(bytes))?
            .with_input_fact(0, f32::fact(shape).into())?
            .into_optimized()?
            .into_runnable()
    };
    run().map_err(|e| Error::Model(format!("{e:#}")))
}

impl Models {
    pub fn load(bytes: ModelBytes<'_>) -> Result<Self, Error> {
        Ok(Models {
            yunet: load(bytes.yunet, &[1, 3, YUNET_SIZE, YUNET_SIZE])?,
            face_mesh: load(bytes.face_mesh, &[1, MESH_SIZE, MESH_SIZE, 3])?,
            modnet: load(bytes.modnet, &[1, 3, MATTE_H, MATTE_W])?,
        })
    }
}

pub(crate) fn run(plan: &Plan, input: Tensor) -> Result<TVec<TValue>, Error> {
    plan.run(tvec!(input.into())).map_err(|e| Error::Model(format!("{e:#}")))
}

pub(crate) fn as_slice(v: &TValue) -> Result<&[f32], Error> {
    v.try_as_plain_ram().and_then(|p| p.as_slice::<f32>()).map_err(|e| Error::Model(format!("{e:#}")))
}
