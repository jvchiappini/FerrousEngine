//! Retained (static) 2D shape batches.
//!
//! A static batch is a **persistent GPU instance buffer** for shapes that do
//! not change every frame (walls, slabs, baked plan geometry).  It is uploaded
//! once by [`Renderer::create_static_2d_batch`](crate::Renderer::create_static_2d_batch)
//! and drawn with one instanced draw call per batch per frame, **without any
//! per-frame `write_buffer`**.  [`Renderer::update_static_2d_batch`](crate::Renderer::update_static_2d_batch)
//! replaces the contents in place (reusing the buffer when it fits) and
//! [`Renderer::destroy_static_2d_batch`](crate::Renderer::destroy_static_2d_batch)
//! frees it.
//!
//! Only **shapes** are retained here (the SDF rect/circle/line/triangle
//! pipeline).  Textured sprites are a different layer and stay immediate.

use ferrous_2d::render::ShapeInstance;

/// Opaque handle to a retained static 2D batch.
///
/// The handle carries a **generation** so that a handle obtained before a
/// [`destroy`](crate::Renderer::destroy_static_2d_batch) (or after the slot is
/// recycled) can never accidentally address a different batch: stale handles are
/// ignored by `update`/`destroy` and simply do not draw.  Cheap to copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Static2dBatchId {
    pub(crate) index: u32,
    pub(crate) generation: u32,
}

impl Static2dBatchId {
    /// Table slot index (stable while the batch is alive).
    #[inline]
    pub fn index(self) -> u32 {
        self.index
    }

    /// Generation of the slot when the handle was issued.  Bumped on destroy.
    #[inline]
    pub fn generation(self) -> u32 {
        self.generation
    }
}

/// GPU-resident instance buffer for one static batch.
pub(crate) struct Static2dBatch {
    /// `VERTEX | COPY_DST` buffer holding `capacity` [`ShapeInstance`]s.
    pub(crate) buffer: wgpu::Buffer,
    /// Instance capacity (grows amortized to the next power of two).
    pub(crate) capacity: u32,
    /// Live instance count (`<= capacity`).
    pub(crate) count: u32,
    /// Representative depth: the minimum `transform_c3[2]` of the batch.  Used
    /// only to order static batches relative to each other (they are all drawn
    /// before the immediate stream; see `brain/2d_static_batch.md`).
    pub(crate) z: f32,
}

impl Static2dBatch {
    /// Minimum capacity reserved for a batch buffer.
    pub(crate) const MIN_CAPACITY: u32 = 128;
}

/// Slot in the batch table.  `generation` invalidates handed-out ids whenever
/// the slot is freed.  `batch == None` means the slot is free.
pub(crate) struct Static2dBatchSlot {
    pub(crate) generation: u32,
    pub(crate) batch: Option<Static2dBatch>,
}

/// Representative depth of a shape slice: the minimum `transform_c3[2]`.
/// Empty (or all-NaN) slices get `0.0`.
pub(crate) fn batch_min_z(shapes: &[ShapeInstance]) -> f32 {
    let mut z = f32::INFINITY;
    for s in shapes {
        let v = s.transform_c3[2];
        if v < z {
            z = v;
        }
    }
    if z.is_finite() {
        z
    } else {
        0.0
    }
}
