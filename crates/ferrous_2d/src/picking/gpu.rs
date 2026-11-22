//! GPU id-buffer picking for the 2D scene.
//!
//! This is the on-demand alternative to [`PickScene`]'s spatial index. It
//! renders the pickable [`PickItem`]s into a small `R32Uint` target with the
//! same geometry, clips, tolerance and `(z, insertion)` order the CPU queries
//! use, reads the pixels back and decodes them with the [`IdBuffer`] encoding.
//!
//! # When to use it
//!
//! A query costs one render pass plus a blocking map/readback (hundreds of
//! microseconds at best), so it never replaces the index on the pointer-move
//! path. Use it when:
//!
//! - one pass is cheaper than many indexed queries (a bulk query over a very
//!   dense scene);
//! - the query is a region (marquee / brush) and a whole id buffer is useful;
//! - the scene is already being drawn and the ids can be produced from the same
//!   geometry.
//!
//! Everywhere else, [`PickScene::pick`] is the default and the fallback when no
//! GPU adapter is available.
//!
//! # Semantics
//!
//! - Point and region picks sample **pixel centres**, exactly like
//!   [`IdBuffer::rasterize`]. A region is snapped up to whole pixels, so the
//!   returned set is "items that cover at least one sampled pixel".
//! - A region query is the GPU `MarqueeMode::Intersect`; `Contain` needs the
//!   shape outline and stays on the CPU.
//! - `Image` alpha thresholds are not sampled (the GPU tests the image bounds
//!   only); use the CPU query when the alpha mask is significant.
//! - [`PickId`] values must be unique per scene and must not be `u32::MAX`:
//!   the integer target reserves `0` for "clear" and stores `id + 1`.
//! - Only items that are `visible`, `pickable` and have `opacity > 0.0` are
//!   rendered, matching the CPU filters.
//!
//! [`IdBuffer`]: super::IdBuffer
//! [`PickItem`]: super::PickItem
//! [`PickScene`]: super::PickScene
//! [`PickScene::pick`]: super::PickScene::pick
//! [`PickId`]: super::PickId

use std::fmt;

use bytemuck::{Pod, Zeroable};
use glam::{Affine2, Vec2};

use super::{Aabb, ClipNode, FillRule, IdBuffer, PickHit, PickId, PickItem, PickScene, PickShape};
use super::{ClipId, StrokeCap};

const ID_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R32Uint;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const MIN_BUFFER_SIZE: u64 = 16;
const BUFFER_ALIGN: u64 = 256;
const NO_CLIP: u32 = u32::MAX;
const MAX_CLIP_STEPS: u32 = 64;

const KIND_RECT: u32 = 0;
const KIND_CIRCLE: u32 = 1;
const KIND_PATH: u32 = 2;
const KIND_TEXT: u32 = 3;
const KIND_IMAGE: u32 = 4;

const PATH_CLOSED: u32 = 1 << 0;
const PATH_FILLED: u32 = 1 << 1;
const PATH_EVEN_ODD: u32 = 1 << 2;
const PATH_STROKE: u32 = 1 << 3;
const PATH_ROUND_CAP: u32 = 1 << 4;

/// Errors returned by [`GpuPicker`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GpuPickError {
    /// The requested region does not fit the adapter's texture limit.
    TooLarge {
        /// Requested width.
        width: u32,
        /// Requested height.
        height: u32,
        /// The adapter's maximum 2D texture dimension.
        max: u32,
    },
    /// `map_async` failed (typically a lost device).
    MapFailed(String),
    /// The map callback was dropped because the device was lost.
    ChannelClosed,
}

impl fmt::Display for GpuPickError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge { width, height, max } => {
                write!(
                    formatter,
                    "pick region {width}x{height} exceeds the adapter limit {max}"
                )
            }
            Self::MapFailed(message) => write!(formatter, "id-buffer map failed: {message}"),
            Self::ChannelClosed => {
                formatter.write_str("id-buffer map channel closed (device lost?)")
            }
        }
    }
}

impl std::error::Error for GpuPickError {}

/// One GPU pick item (128 bytes, 16-byte aligned in WGSL).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
struct GpuPickItem {
    aabb: [f32; 4],
    inv0: [f32; 4],
    inv1: [f32; 4],
    local: [f32; 4],
    shape: [f32; 4],
    meta: [u32; 4],
    more: [u32; 4],
    depth_tolerance: [f32; 4],
}

/// World->target affine and target size (32 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
struct GpuPickUniform {
    view_c0: [f32; 2],
    view_c1: [f32; 2],
    view_c3: [f32; 2],
    size: [f32; 2],
}

/// One clip in the GPU clip chain (64 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
struct GpuClip {
    inv0: [f32; 4],
    inv1: [f32; 4],
    rect: [f32; 4],
    meta: [u32; 4],
}

impl GpuClip {
    fn from_node(node: &ClipNode) -> Self {
        let parent = node.parent.map_or(NO_CLIP, |parent| parent.0);
        let rect = [
            node.rect.min.x,
            node.rect.min.y,
            node.rect.max.x,
            node.rect.max.y,
        ];
        if !node.enabled {
            return Self {
                inv0: [1.0, 0.0, 0.0, 1.0],
                inv1: [0.0; 4],
                rect,
                meta: [parent, 0, 0, 0],
            };
        }
        let Some(inverse) = super::inverse(node.transform) else {
            return Self {
                inv0: [1.0, 0.0, 0.0, 1.0],
                inv1: [0.0; 4],
                rect,
                meta: [parent, 2, 0, 0],
            };
        };
        let matrix = inverse.matrix2;
        Self {
            inv0: [
                matrix.x_axis.x,
                matrix.x_axis.y,
                matrix.y_axis.x,
                matrix.y_axis.y,
            ],
            inv1: [inverse.translation.x, inverse.translation.y, 0.0, 0.0],
            rect,
            meta: [parent, 1, 0, 0],
        }
    }
}

/// One item that was actually rendered, with its full `(z, insertion)` rank.
#[derive(Clone, Copy, Debug)]
struct KeptItem {
    id: PickId,
    rank: u32,
    item: usize,
}

struct RenderOutput {
    pixels: Option<Vec<u32>>,
    kept: Vec<KeptItem>,
}

/// The GPU id-buffer picker.
///
/// The picker owns its pipeline, buffers and the `R32Uint` target. The target
/// and the staging buffer are allocated on first use and reused while the
/// requested size does not change, so repeated picks of the same region do not
/// create GPU resources.
pub struct GpuPicker {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    uniform_buffer: wgpu::Buffer,
    item_buffer: wgpu::Buffer,
    item_capacity: u64,
    point_buffer: wgpu::Buffer,
    point_capacity: u64,
    clip_buffer: wgpu::Buffer,
    clip_capacity: u64,
    id_texture: Option<wgpu::Texture>,
    id_view: Option<wgpu::TextureView>,
    depth_view: Option<wgpu::TextureView>,
    staging: Option<wgpu::Buffer>,
    target_size: (u32, u32),
    padded_bytes_per_row: u32,
    order: Vec<usize>,
}

impl GpuPicker {
    /// Creates the pipeline and the reusable buffers. No target is allocated
    /// until the first pick.
    #[must_use]
    pub fn new(device: &wgpu::Device) -> Self {
        Self::new_with_cache(device, None)
    }

    /// Same as [`Self::new`] but threads a shared [`wgpu::PipelineCache`].
    #[must_use]
    pub fn new_with_cache(device: &wgpu::Device, cache: Option<&wgpu::PipelineCache>) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("2D Pick Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(
                            std::mem::size_of::<GpuPickUniform>() as u64,
                        ),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("2D Pick Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("2D Scene ID Shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../../assets/shaders/scene_id_2d.wgsl").into(),
            ),
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("2D Scene ID Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: ID_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Greater,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache,
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("2D Pick Uniforms"),
            size: std::mem::size_of::<GpuPickUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let item_buffer = Self::empty_buffer(device, "2D Pick Item Buffer");
        let point_buffer = Self::empty_buffer(device, "2D Pick Point Buffer");
        // A storage array binding must be at least one element long even when
        // the scene has no clips; the shader indexes `array<Clip>` (64 bytes).
        let clip_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("2D Pick Clip Buffer"),
            size: std::mem::size_of::<GpuClip>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            bind_group_layout,
            uniform_buffer,
            item_buffer,
            item_capacity: MIN_BUFFER_SIZE,
            point_buffer,
            point_capacity: MIN_BUFFER_SIZE,
            clip_buffer,
            clip_capacity: std::mem::size_of::<GpuClip>() as u64,
            id_texture: None,
            id_view: None,
            depth_view: None,
            staging: None,
            target_size: (0, 0),
            padded_bytes_per_row: 0,
            order: Vec::new(),
        }
    }

    fn empty_buffer(device: &wgpu::Device, label: &str) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: MIN_BUFFER_SIZE,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// Picks the topmost item under `point`.
    ///
    /// `tolerance` is in world units (the CPU query converts its screen
    /// tolerance with the item transform; pass the world value here). The
    /// target is 1x1 and the point is sampled at the pixel centre.
    pub fn pick_gpu(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &PickScene,
        point: Vec2,
        tolerance: f32,
    ) -> Result<Option<PickHit>, GpuPickError> {
        let area = Aabb {
            min: point - Vec2::splat(0.5),
            max: point + Vec2::splat(0.5),
        };
        let view = Affine2::from_translation(Vec2::splat(0.5) - point);
        let output = self.render(device, queue, scene, area, tolerance, view, 1, 1, u32::MAX)?;
        let Some(pixels) = output.pixels else {
            return Ok(None);
        };
        let Some(id) = IdBuffer::decode_u32(pixels[0]) else {
            return Ok(None);
        };
        Ok(topmost(&output.kept, id).map(|kept| PickHit {
            id: kept.id,
            item: kept.item,
        }))
    }

    /// Every item under `point`, topmost first, into a reused buffer.
    ///
    /// The id buffer stores one id per pixel, so this re-runs the 1x1 pass with
    /// a decreasing rank cutoff: one pass per hit, still on demand only. Ids
    /// must be unique (see the module docs).
    pub fn pick_all_gpu(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &PickScene,
        point: Vec2,
        tolerance: f32,
        out: &mut Vec<PickHit>,
    ) -> Result<(), GpuPickError> {
        out.clear();
        let area = Aabb {
            min: point - Vec2::splat(0.5),
            max: point + Vec2::splat(0.5),
        };
        let view = Affine2::from_translation(Vec2::splat(0.5) - point);
        let mut cutoff = u32::MAX;
        loop {
            let output = self.render(device, queue, scene, area, tolerance, view, 1, 1, cutoff)?;
            let Some(pixels) = output.pixels else {
                break;
            };
            let Some(id) = IdBuffer::decode_u32(pixels[0]) else {
                break;
            };
            let Some(kept) = topmost(&output.kept, id) else {
                break;
            };
            out.push(PickHit {
                id: kept.id,
                item: kept.item,
            });
            if kept.rank == 0 {
                break;
            }
            cutoff = kept.rank;
        }
        Ok(())
    }

    /// Items that cover at least one pixel of `area` (GPU `Intersect`).
    ///
    /// The area is snapped up to whole pixels, so the result is defined for the
    /// pixel grid starting at `area.min`. Use [`PickScene::marquee`] for
    /// `MarqueeMode::Contain` or exact sub-pixel semantics.
    ///
    /// [`PickScene::marquee`]: super::PickScene::marquee
    pub fn region_gpu(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &PickScene,
        area: Aabb,
        tolerance: f32,
        out: &mut Vec<PickHit>,
    ) -> Result<(), GpuPickError> {
        out.clear();
        let size = area.size();
        if !(size.x > 0.0 && size.y > 0.0) {
            return Ok(());
        }
        let max_dimension = device.limits().max_texture_dimension_2d;
        let width = (size.x.ceil() as u32).max(1);
        let height = (size.y.ceil() as u32).max(1);
        if width > max_dimension || height > max_dimension {
            return Err(GpuPickError::TooLarge {
                width,
                height,
                max: max_dimension,
            });
        }
        let view = Affine2::from_translation(-area.min);
        let output = self.render(
            device,
            queue,
            scene,
            area,
            tolerance,
            view,
            width,
            height,
            u32::MAX,
        )?;
        let Some(pixels) = output.pixels else {
            return Ok(());
        };

        // Map every pixel to the kept item position. Ids are usually dense
        // (a slot index), so a direct table is the fast path; a sorted table
        // covers the rare huge-id case without allocating by id value.
        let max_id = output.kept.iter().map(|kept| kept.id).max().unwrap_or(0);
        let mut dense = if max_id <= 1_000_000 {
            Some(vec![u32::MAX; max_id as usize + 1])
        } else {
            None
        };
        let mut sorted: Vec<(PickId, u32)> = Vec::new();
        if let Some(table) = dense.as_mut() {
            for (position, kept) in output.kept.iter().enumerate() {
                table[kept.id as usize] = position as u32;
            }
        } else {
            sorted = output
                .kept
                .iter()
                .enumerate()
                .map(|(position, kept)| (kept.id, position as u32))
                .collect();
            sorted.sort_unstable_by_key(|(id, _)| *id);
        }
        let mut seen = vec![false; output.kept.len()];
        for raw in pixels {
            let Some(id) = IdBuffer::decode_u32(raw) else {
                continue;
            };
            let position = if let Some(table) = &dense {
                table
                    .get(id as usize)
                    .copied()
                    .filter(|slot| *slot != u32::MAX)
            } else {
                sorted
                    .binary_search_by_key(&id, |(candidate, _)| *candidate)
                    .ok()
                    .map(|index| sorted[index].1)
            };
            if let Some(position) = position {
                seen[position as usize] = true;
            }
        }

        // `kept` is in `(z, insertion)` order, so reverse iteration is
        // topmost first, like the CPU marquee.
        out.extend(
            output
                .kept
                .iter()
                .enumerate()
                .rev()
                .filter(|(position, _)| seen[*position])
                .map(|(_, kept)| PickHit {
                    id: kept.id,
                    item: kept.item,
                }),
        );
        Ok(())
    }

    /// Alias for [`Self::region_gpu`] (the GPU marquee is `Intersect` only).
    pub fn marquee_gpu(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &PickScene,
        area: Aabb,
        tolerance: f32,
        out: &mut Vec<PickHit>,
    ) -> Result<(), GpuPickError> {
        self.region_gpu(device, queue, scene, area, tolerance, out)
    }

    /// The last target size, or `(0, 0)` before the first pick.
    #[must_use]
    pub fn target_size(&self) -> (u32, u32) {
        self.target_size
    }

    /// Bytes of the current `R32Uint` id target (`0` before the first pick).
    #[must_use]
    pub fn id_texture_bytes(&self) -> u64 {
        let (width, height) = self.target_size;
        u64::from(width) * u64::from(height) * 4
    }

    #[allow(clippy::too_many_arguments)]
    fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &PickScene,
        area: Aabb,
        tolerance: f32,
        view: Affine2,
        width: u32,
        height: u32,
        cutoff: u32,
    ) -> Result<RenderOutput, GpuPickError> {
        let items = scene.items();
        self.order.clear();
        self.order.extend(0..items.len());
        self.order.sort_by_key(|&index| (items[index].z, index));

        let tolerance = tolerance.max(0.0);
        let mut gpu_items = Vec::new();
        let mut gpu_points: Vec<[f32; 2]> = Vec::new();
        let mut kept = Vec::new();
        let denominator = self.order.len() as f32 + 1.0;

        for (rank, &index) in self.order.iter().enumerate() {
            let Some(item) = items.get(index) else {
                continue;
            };
            if rank as u32 >= cutoff {
                break;
            }
            if !item.visible || item.opacity <= 0.0 || !item.pickable {
                continue;
            }
            let Some(world_bounds) = item.world_bounds() else {
                continue;
            };
            let Some(inverse) = super::inverse(item.transform) else {
                continue;
            };
            let bounds = match clipped_bounds(scene, item) {
                Ok(None) => world_bounds,
                Ok(Some(clip)) => {
                    let clipped = world_bounds.intersect(clip);
                    if clipped.is_empty() {
                        continue;
                    }
                    clipped
                }
                Err(()) => continue,
            };
            let local_tolerance = tolerance * super::local_scale(item.transform);
            let scale_bound = item
                .transform
                .matrix2
                .x_axis
                .length()
                .max(item.transform.matrix2.y_axis.length());
            let render_aabb = bounds.intersect(area.expand(local_tolerance * scale_bound));
            if render_aabb.is_empty() {
                continue;
            }
            let depth = (rank as f32 + 1.0) / denominator;
            let mut gpu = GpuPickItem {
                aabb: [
                    render_aabb.min.x,
                    render_aabb.min.y,
                    render_aabb.max.x,
                    render_aabb.max.y,
                ],
                inv0: [
                    inverse.matrix2.x_axis.x,
                    inverse.matrix2.x_axis.y,
                    inverse.matrix2.y_axis.x,
                    inverse.matrix2.y_axis.y,
                ],
                inv1: [inverse.translation.x, inverse.translation.y, 0.0, 0.0],
                local: [0.0; 4],
                shape: [0.0; 4],
                meta: [IdBuffer::encode_u32(item.id), 0, 0, gpu_points.len() as u32],
                more: [0, item.clip.map_or(NO_CLIP, |clip| clip.0), 0, 0],
                depth_tolerance: [depth, local_tolerance, 0.0, 0.0],
            };

            match &item.shape {
                PickShape::Rect {
                    size,
                    corner_radius,
                } => {
                    if *size == Vec2::ZERO {
                        continue;
                    }
                    gpu.meta[1] = KIND_RECT;
                    gpu.local = [-size.x * 0.5, -size.y * 0.5, size.x * 0.5, size.y * 0.5];
                    gpu.shape = [size.x, size.y, *corner_radius, 0.0];
                }
                PickShape::Circle { radius } => {
                    if *radius <= 0.0 {
                        continue;
                    }
                    gpu.meta[1] = KIND_CIRCLE;
                    gpu.local = [-*radius, -*radius, *radius, *radius];
                    gpu.shape = [*radius, 0.0, 0.0, 0.0];
                }
                PickShape::Path {
                    points,
                    closed,
                    filled,
                    fill_rule,
                    stroke,
                } => {
                    let Some(local) = item.shape.local_bounds() else {
                        continue;
                    };
                    gpu.meta[1] = KIND_PATH;
                    gpu.local = [local.min.x, local.min.y, local.max.x, local.max.y];
                    let mut flags = 0;
                    if *closed {
                        flags |= PATH_CLOSED;
                    }
                    if *filled {
                        flags |= PATH_FILLED;
                    }
                    if *fill_rule == FillRule::EvenOdd {
                        flags |= PATH_EVEN_ODD;
                    }
                    if let Some(stroke) = stroke {
                        flags |= PATH_STROKE;
                        if stroke.cap == StrokeCap::Round {
                            flags |= PATH_ROUND_CAP;
                        }
                        gpu.shape[3] = stroke.width;
                    }
                    gpu.meta[2] = flags;
                    gpu.meta[3] = gpu_points.len() as u32;
                    gpu.more[0] = points.len() as u32;
                    gpu_points.extend(points.iter().map(|point| [point.x, point.y]));
                }
                PickShape::Text { bounds } => {
                    gpu.meta[1] = KIND_TEXT;
                    gpu.local = [bounds.min.x, bounds.min.y, bounds.max.x, bounds.max.y];
                }
                PickShape::Image { bounds, alpha } => {
                    gpu.meta[1] = KIND_IMAGE;
                    gpu.local = [bounds.min.x, bounds.min.y, bounds.max.x, bounds.max.y];
                    if alpha.is_some() {
                        // The fragment shader still tests the bounds: the alpha
                        // threshold is documented as CPU-only.
                        gpu.meta[2] = 1 << 5;
                    }
                }
            }

            gpu_items.push(gpu);
            kept.push(KeptItem {
                id: item.id,
                rank: rank as u32,
                item: index,
            });
        }

        if gpu_items.is_empty() {
            return Ok(RenderOutput { pixels: None, kept });
        }

        self.ensure_target(device, width, height)?;
        grow_buffer(
            device,
            &mut self.item_buffer,
            &mut self.item_capacity,
            std::mem::size_of_val(gpu_items.as_slice()) as u64,
            "2D Pick Item Buffer",
        );
        grow_buffer(
            device,
            &mut self.point_buffer,
            &mut self.point_capacity,
            std::mem::size_of_val(gpu_points.as_slice()) as u64,
            "2D Pick Point Buffer",
        );
        let gpu_clips: Vec<GpuClip> = scene.clips().iter().map(GpuClip::from_node).collect();
        grow_buffer(
            device,
            &mut self.clip_buffer,
            &mut self.clip_capacity,
            std::mem::size_of_val(gpu_clips.as_slice()) as u64,
            "2D Pick Clip Buffer",
        );

        queue.write_buffer(&self.item_buffer, 0, bytemuck::cast_slice(&gpu_items));
        if !gpu_points.is_empty() {
            queue.write_buffer(&self.point_buffer, 0, bytemuck::cast_slice(&gpu_points));
        }
        if !gpu_clips.is_empty() {
            queue.write_buffer(&self.clip_buffer, 0, bytemuck::cast_slice(&gpu_clips));
        }
        let matrix = view.matrix2;
        let uniform = GpuPickUniform {
            view_c0: [matrix.x_axis.x, matrix.x_axis.y],
            view_c1: [matrix.y_axis.x, matrix.y_axis.y],
            view_c3: [view.translation.x, view.translation.y],
            size: [width as f32, height as f32],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniform));

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("2D Pick Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.item_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.point_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.clip_buffer.as_entire_binding(),
                },
            ],
        });

        let id_view = self
            .id_view
            .as_ref()
            .expect("ensure_target always allocates the id view");
        let depth_view = self
            .depth_view
            .as_ref()
            .expect("ensure_target always allocates the depth view");
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("2D Pick Encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("2D Pick ID Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: id_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..4, 0..gpu_items.len() as u32);
        }
        let staging = self
            .staging
            .as_ref()
            .expect("ensure_target always allocates the staging buffer");
        encoder.copy_texture_to_buffer(
            wgpu::ImageCopyTexture {
                texture: self
                    .id_texture
                    .as_ref()
                    .expect("ensure_target always allocates the id texture"),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyBuffer {
                buffer: staging,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(self.padded_bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        queue.submit(Some(encoder.finish()));

        let pixels = self.map_and_decode(device, width, height)?;
        Ok(RenderOutput {
            pixels: Some(pixels),
            kept,
        })
    }

    fn ensure_target(
        &mut self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> Result<(), GpuPickError> {
        let max = device.limits().max_texture_dimension_2d;
        if width > max || height > max {
            return Err(GpuPickError::TooLarge { width, height, max });
        }
        if self.target_size == (width, height) && self.id_texture.is_some() {
            return Ok(());
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("2D Pick ID Target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: ID_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("2D Pick Depth Target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());

        let padded = (width * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("2D Pick Readback Staging"),
            size: u64::from(padded) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        self.id_texture = Some(texture);
        self.id_view = Some(view);
        self.depth_view = Some(depth_view);
        self.staging = Some(staging);
        self.target_size = (width, height);
        self.padded_bytes_per_row = padded;
        Ok(())
    }

    fn map_and_decode(
        &mut self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> Result<Vec<u32>, GpuPickError> {
        let staging = self
            .staging
            .as_ref()
            .expect("ensure_target always allocates the staging buffer");
        let slice = staging.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        device.poll(wgpu::Maintain::Wait);

        let mapped = match receiver.recv() {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => Err(GpuPickError::MapFailed(error.to_string())),
            Err(_) => Err(GpuPickError::ChannelClosed),
        };

        let mut pixels = Vec::new();
        if mapped.is_ok() {
            let data = slice.get_mapped_range();
            pixels.resize((width * height) as usize, u32::MAX);
            let row_bytes = self.padded_bytes_per_row as usize;
            for row in 0..height as usize {
                let start = row * row_bytes;
                for column in 0..width as usize {
                    let offset = start + column * 4;
                    pixels[row * width as usize + column] = u32::from_le_bytes([
                        data[offset],
                        data[offset + 1],
                        data[offset + 2],
                        data[offset + 3],
                    ]);
                }
            }
            drop(data);
        }
        staging.unmap();
        mapped.map(|()| pixels)
    }
}

fn topmost(kept: &[KeptItem], id: PickId) -> Option<KeptItem> {
    kept.iter()
        .filter(|entry| entry.id == id)
        .max_by_key(|entry| entry.rank)
        .copied()
}

fn clipped_bounds(scene: &PickScene, item: &PickItem) -> Result<Option<Aabb>, ()> {
    let mut bounds: Option<Aabb> = None;
    let mut clip: Option<ClipId> = item.clip;
    let mut steps = 0;
    while let Some(id) = clip {
        if steps >= MAX_CLIP_STEPS {
            break;
        }
        steps += 1;
        let Some(node) = scene.clips().get(id.0 as usize) else {
            return Err(());
        };
        if node.enabled {
            let boxed = Aabb::from_points(&super::transformed_corners(node.rect, node.transform));
            bounds = Some(bounds.map_or(boxed, |current| current.intersect(boxed)));
        }
        clip = node.parent;
    }
    Ok(bounds)
}

fn grow_buffer(
    device: &wgpu::Device,
    buffer: &mut wgpu::Buffer,
    capacity: &mut u64,
    needed: u64,
    label: &str,
) {
    let needed = needed.max(MIN_BUFFER_SIZE);
    if needed <= *capacity {
        return;
    }
    let size = needed.div_ceil(BUFFER_ALIGN) * BUFFER_ALIGN;
    *buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    *capacity = size;
}

const _: () = {
    assert!(std::mem::size_of::<GpuPickItem>() == 128);
    assert!(std::mem::size_of::<GpuClip>() == 64);
    assert!(std::mem::size_of::<GpuPickUniform>() == 32);
    assert!(MAX_CLIP_STEPS <= 64);
};
