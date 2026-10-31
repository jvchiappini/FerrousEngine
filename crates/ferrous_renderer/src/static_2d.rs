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

use ferrous_2d::render::{ShapeGpu, ShapeInstance};

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

/// Number of consecutive instances that share one culling AABB.
///
/// Small enough that a zoomed-in view skips most of a large plan, large enough
/// that the per-frame chunk test is negligible (100k instances = ~400 tests).
pub(crate) const STATIC_CHUNK: usize = 256;

/// GPU-resident instance buffer for one static batch.
pub(crate) struct Static2dBatch {
    /// `VERTEX | COPY_DST` buffer holding `capacity` packed
    /// [`ShapeGpu`](ferrous_2d::render::ShapeGpu)s.
    pub(crate) buffer: wgpu::Buffer,
    /// Instance capacity (grows amortized to the next power of two).
    pub(crate) capacity: u32,
    /// Live instance count (`<= capacity`).
    pub(crate) count: u32,
    /// Representative depth: the minimum `transform_c3[2]` of the batch.  Used
    /// only to order static batches relative to each other (they are all drawn
    /// before the immediate stream; see `brain/2d_static_batch.md`).
    pub(crate) z: f32,
    /// CPU mirror of what is in `buffer`, diffed on update so an edit only
    /// re-uploads the chunks that actually changed.
    pub(crate) cpu: Vec<ShapeGpu>,
    /// World AABB `[min_x, min_y, max_x, max_y]` of every run of
    /// [`STATIC_CHUNK`] consecutive instances, for per-frame culling.  Draw
    /// order inside the batch is never changed: visible chunks are drawn as
    /// contiguous ranges in their original order.
    pub(crate) chunk_aabbs: Vec<[f32; 4]>,
}

impl Static2dBatch {
    /// Minimum capacity reserved for a batch buffer.
    pub(crate) const MIN_CAPACITY: u32 = 128;
}

/// Packs authoring instances into the compact GPU layout.
pub(crate) fn pack_shapes(shapes: &[ShapeInstance]) -> Vec<ShapeGpu> {
    shapes.iter().map(ShapeGpu::pack).collect()
}

/// One AABB per [`STATIC_CHUNK`] consecutive instances.
pub(crate) fn chunk_aabbs(shapes: &[ShapeInstance]) -> Vec<[f32; 4]> {
    shapes
        .chunks(STATIC_CHUNK)
        .map(|chunk| {
            let mut min = glam::Vec2::splat(f32::INFINITY);
            let mut max = glam::Vec2::splat(f32::NEG_INFINITY);
            for s in chunk {
                let (a, b) = s.world_aabb();
                min = min.min(a);
                max = max.max(b);
            }
            [min.x, min.y, max.x, max.y]
        })
        .collect()
}

/// Appends to `out` the instance ranges `(start, count)` of the chunks whose
/// AABB touches `[rect_min, rect_max]`, merging adjacent visible chunks into
/// one range.  Returns the number of visible instances.
pub(crate) fn visible_runs(
    aabbs: &[[f32; 4]],
    count: u32,
    rect_min: glam::Vec2,
    rect_max: glam::Vec2,
    out: &mut Vec<(u32, u32)>,
) -> u32 {
    let mut visible = 0u32;
    let mut run: Option<(u32, u32)> = None;
    for (i, b) in aabbs.iter().enumerate() {
        let start = (i * STATIC_CHUNK) as u32;
        if start >= count {
            break;
        }
        let len = (STATIC_CHUNK as u32).min(count - start);
        let hit = b[0] <= rect_max.x && b[2] >= rect_min.x && b[1] <= rect_max.y && b[3] >= rect_min.y;
        if hit {
            visible += len;
            run = match run {
                Some((s, n)) => Some((s, n + len)),
                None => Some((start, len)),
            };
        } else if let Some(r) = run.take() {
            out.push(r);
        }
    }
    if let Some(r) = run {
        out.push(r);
    }
    visible
}

/// Instance ranges `[start, end)` of `new` that differ from `old`, at chunk
/// granularity.  Ranges separated by a small clean gap are merged so an edit
/// costs a handful of `write_buffer` calls at most.
pub(crate) fn dirty_ranges(old: &[ShapeGpu], new: &[ShapeGpu]) -> Vec<(usize, usize)> {
    const MERGE_GAP: usize = 4 * STATIC_CHUNK;
    let mut out: Vec<(usize, usize)> = Vec::new();
    let mut start = 0usize;
    while start < new.len() {
        let end = (start + STATIC_CHUNK).min(new.len());
        let same = end <= old.len()
            && bytemuck::cast_slice::<ShapeGpu, u8>(&old[start..end])
                == bytemuck::cast_slice::<ShapeGpu, u8>(&new[start..end]);
        if !same {
            match out.last_mut() {
                Some(last) if start - last.1 <= MERGE_GAP => last.1 = end,
                _ => out.push((start, end)),
            }
        }
        start = end;
    }
    out
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

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec2;

    fn rect_at(x: f32) -> ShapeInstance {
        ShapeInstance::rect(Vec2::new(x, 0.0), Vec2::splat(1.0), [1.0; 4])
    }

    #[test]
    fn visible_runs_merge_adjacent_chunks() {
        // Three chunks laid out left to right.
        let shapes: Vec<ShapeInstance> = (0..STATIC_CHUNK * 3)
            .map(|i| rect_at((i / STATIC_CHUNK) as f32 * 100.0))
            .collect();
        let aabbs = chunk_aabbs(&shapes);
        assert_eq!(aabbs.len(), 3);
        let mut out = Vec::new();
        let n = visible_runs(&aabbs, shapes.len() as u32, Vec2::new(-10.0, -10.0), Vec2::new(150.0, 10.0), &mut out);
        assert_eq!(out, vec![(0, (STATIC_CHUNK * 2) as u32)]);
        assert_eq!(n as usize, STATIC_CHUNK * 2);
        out.clear();
        visible_runs(&aabbs, shapes.len() as u32, Vec2::new(190.0, -10.0), Vec2::new(210.0, 10.0), &mut out);
        assert_eq!(out, vec![((STATIC_CHUNK * 2) as u32, STATIC_CHUNK as u32)]);
    }

    #[test]
    fn dirty_ranges_only_flag_changed_chunks() {
        let a: Vec<ShapeGpu> = (0..STATIC_CHUNK * 20).map(|i| ShapeGpu::pack(&rect_at(i as f32))).collect();
        let mut b = a.clone();
        assert!(dirty_ranges(&a, &b).is_empty());
        b[STATIC_CHUNK * 10 + 3].c3[0] += 1.0;
        assert_eq!(dirty_ranges(&a, &b), vec![(STATIC_CHUNK * 10, STATIC_CHUNK * 11)]);
        b.push(b[0]);
        let r = dirty_ranges(&a, &b);
        assert_eq!(r.last().unwrap().1, b.len());
    }
}
