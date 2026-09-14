//! Headless (windowless) benchmarking + integration-test harness for the 2D
//! render path.
//!
//! # Why this exists
//!
//! FerrousEngine previously had **zero** render tests and **zero** benchmarks
//! (see the 2D audit shared context).  Every performance claim about the 2D
//! pipeline was therefore unmeasurable.  This module provides the smallest
//! possible harness that:
//!
//! 1. Creates a [`ferrous_gpu::EngineContext`] **without a window or
//!    `Surface`** — it uses a fallback software adapter (lavapipe / llvmpipe
//!    on this VPS) when no hardware adapter is present.
//! 2. Builds a [`Renderer`] in [`RendererMode::Pure2D`] and renders a 2D
//!    scene into an off-screen [`wgpu::Texture`].
//! 3. Reports **hardware-independent** metrics (`draw_calls`,
//!    `bytes_uploaded`, `instances_uploaded`, buffer reallocations, bind-group
//!    creations) plus a wall-clock `cpu_ms` that is only meaningful as a
//!    relative delta on the same machine.
//!
//! # Metrics origin
//!
//! The per-draw-call / bytes-uploaded counters come from
//! [`ferrous_2d::render::Renderer2d::frame_counters`], which the engine's
//! Pure2D fast path (`renderer_core::render_internal`) drives for both shape
//! and sprite draws.  Allocations-per-frame are *not* measured here; the
//! `benches/pure2d_headless.rs` benchmark installs `divan::AllocProfiler` as
//! the global allocator so `cargo bench` reports `allocs` and `bytes` per
//! iteration.
//!
//! # Reproducing
//!
//! ```text
//! export CARGO_TARGET_DIR=<shared target dir>
//! export VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json
//! export VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json
//! export WGPU_BACKEND=vulkan
//! cargo test  -p ferrous_renderer --test pure2d_headless
//! cargo bench -p ferrous_renderer --bench pure2d_headless
//! ```

use std::time::Instant;

use ferrous_2d::render::{ShapeInstance, SpriteInstance};
use wgpu::{
    BufferDescriptor, BufferUsages, CommandEncoderDescriptor, Extent3d, ImageCopyBuffer,
    ImageCopyTexture, ImageDataLayout, Maintain, MapMode, Origin3d, TextureAspect,
    TextureDescriptor, TextureDimension, TextureFormat, TextureUsages, TextureView,
    TextureViewDescriptor,
};

use crate::renderer_core::Renderer;

/// Default off-screen colour format. Mirrors the sRGB swapchain format used by
/// native consumers (Koncreta) so the 2D shader/AA maths runs in the same
/// colour space as production.
///
/// Verified against the engine after the colour-space fix (#12): the native
/// swapchain now prefers an `*Srgb` format (`crates/ferrous_engine/src/
/// graphics.rs`) and the 2D pipeline renders linear light into it, so
/// `Rgba8UnormSrgb` is the format the harness must use to reproduce production.
pub const DEFAULT_FORMAT: TextureFormat = TextureFormat::Rgba8UnormSrgb;

/// An off-screen RGBA8 render target that can be read back on the CPU.
///
/// `usage` includes `COPY_SRC` so [`OffscreenTarget::read_rgba8`] can copy the
/// rendered image into a mapped staging buffer.  No `Surface` is ever created.
pub struct OffscreenTarget {
    pub texture: wgpu::Texture,
    pub view: TextureView,
    pub width: u32,
    pub height: u32,
    pub format: TextureFormat,
}

impl OffscreenTarget {
    pub fn new(device: &wgpu::Device, width: u32, height: u32, format: TextureFormat) -> Self {
        let texture = device.create_texture(&TextureDescriptor {
            label: Some("2D Headless Offscreen Target"),
            size: Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format,
            usage: TextureUsages::RENDER_ATTACHMENT
                | TextureUsages::COPY_SRC
                | TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&TextureViewDescriptor::default());
        Self {
            texture,
            view,
            width,
            height,
            format,
        }
    }

    /// Copies the target into a staging buffer and maps it back as tightly
    /// packed RGBA8 bytes.  Only 8-bit RGBA formats are supported; callers that
    /// need a different format should read `texture` directly.
    pub fn read_rgba8(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> Vec<u8> {
        assert!(
            matches!(
                self.format,
                TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb
            ),
            "read_rgba8 only supports Rgba8Unorm/Rgba8UnormSrgb, got {:?}",
            self.format
        );

        // `copy_texture_to_buffer` requires `bytes_per_row` aligned to 256.
        let unpadded_bytes_per_row = self.width * 4;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(align) * align;

        let buffer = device.create_buffer(&BufferDescriptor {
            label: Some("2D Headless Readback Buffer"),
            size: (padded_bytes_per_row * self.height) as u64,
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("2D Headless Readback Encoder"),
        });
        encoder.copy_texture_to_buffer(
            ImageCopyTexture {
                texture: &self.texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            ImageCopyBuffer {
                buffer: &buffer,
                layout: ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(self.height),
                },
            },
            Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        queue.submit(Some(encoder.finish()));

        let slice = buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        device.poll(Maintain::Wait);
        rx.recv().expect("readback channel closed").expect("map failed");

        let mapped = slice.get_mapped_range();
        let mut out = Vec::with_capacity((self.width * self.height * 4) as usize);
        for row in 0..self.height {
            let start = (row * padded_bytes_per_row) as usize;
            out.extend_from_slice(&mapped[start..start + unpadded_bytes_per_row as usize]);
        }
        drop(mapped);
        buffer.unmap();

        // Return the origin-top-left convention row order that the GPU produced;
        // wgpu render targets store row 0 at the top, matching image-space Y-down.
        out
    }

    /// Reads a single pixel as `[r, g, b, a]`.
    pub fn read_pixel(&self, device: &wgpu::Device, queue: &wgpu::Queue, x: u32, y: u32) -> [u8; 4] {
        assert!(x < self.width && y < self.height, "pixel out of bounds");
        let pixels = self.read_rgba8(device, queue);
        let idx = ((y * self.width + x) * 4) as usize;
        [pixels[idx], pixels[idx + 1], pixels[idx + 2], pixels[idx + 3]]
    }
}

/// Hardware-independent metrics for a single rendered frame.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct FrameReport {
    /// Total GPU draw calls issued for 2D geometry this frame.
    pub draw_calls: u32,
    pub shape_draw_calls: u32,
    pub sprite_draw_calls: u32,
    /// Bytes written to GPU instance buffers this frame.
    pub bytes_uploaded: u64,
    /// Shape + sprite instances uploaded this frame.
    pub instances_uploaded: u32,
    /// Instance buffers reallocated because the frame outgrew capacity.
    /// Steady-state frames report `0`.
    pub buffer_reallocations: u32,
    /// Sprite bind groups created during *this* frame's `render_frame` call.
    pub bind_groups_created: u32,
    /// Total sprite bind groups currently registered (including setup).
    pub bind_groups_total: u32,
    /// Wall-clock duration of `render_frame`, **including** software
    /// rasterisation because `device.poll(Maintain::Wait)` blocks until the
    /// lavapipe CPU rasteriser finishes.  Use only as a relative delta on the
    /// same machine — never as an absolute fps figure.
    pub cpu_ms: f64,
}

/// The harness: owns a Pure2D [`Renderer`] and an off-screen [`OffscreenTarget`].
///
/// Typical use:
///
/// ```no_run
/// # use ferrous_renderer::headless::{Pure2dHarness, DEFAULT_FORMAT};
/// # use ferrous_2d::render::ShapeInstance;
/// let mut h = Pure2dHarness::new(256, 256, DEFAULT_FORMAT).unwrap();
/// h.begin_frame();
/// h.draw_shape(ShapeInstance::rect(glam::Vec2::ZERO, glam::Vec2::new(64.0, 64.0), [1.0, 0.0, 0.0, 1.0]));
/// let report = h.render_frame();
/// assert_eq!(report.draw_calls, 1);
/// ```
pub struct Pure2dHarness {
    renderer: Renderer,
    target: OffscreenTarget,
    width: u32,
    height: u32,
    last_bind_group_count: u32,
}

impl Pure2dHarness {
    /// Creates a headless device (fallback software adapter if needed) and a
    /// Pure2D renderer with a transparent-black clear colour.  No window and no
    /// `Surface` are created.
    pub fn new(width: u32, height: u32, format: TextureFormat) -> Result<Self, String> {
        let context = pollster::block_on(ferrous_core::context::EngineContext::new())
            .map_err(|e| format!("failed to create headless EngineContext: {e}"))?;
        let target = OffscreenTarget::new(&context.device, width, height, format);
        let mut renderer = Renderer::new(context, width, height, format, 1, None);
        renderer.enable_pure_2d(wgpu::Color {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        });
        Ok(Self {
            renderer,
            target,
            width,
            height,
            last_bind_group_count: 0,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn target(&self) -> &OffscreenTarget {
        &self.target
    }

    pub fn renderer(&self) -> &Renderer {
        &self.renderer
    }

    pub fn renderer_mut(&mut self) -> &mut Renderer {
        &mut self.renderer
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.renderer.context.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.renderer.context.queue
    }

    pub fn backend(&self) -> wgpu::Backend {
        self.renderer.context.backend
    }

    pub fn adapter_name(&self) -> String {
        self.renderer.context.adapter.get_info().name
    }

    /// Clears both 2D batchers and resets the per-frame counters.  Call once at
    /// the start of every frame (mirrors `AppContext::render.clear_2d()`).
    pub fn begin_frame(&mut self) {
        self.renderer.clear_2d();
        self.renderer.renderer_2d_pure.reset_frame_counters();
    }

    pub fn draw_shape(&mut self, instance: ShapeInstance) {
        self.renderer.draw_2d_shape(instance);
    }

    pub fn draw_sprite(&mut self, texture_id: u32, instance: SpriteInstance) {
        self.renderer.draw_sprite_2d(texture_id, instance);
    }

    /// Registers an RGBA8 sprite texture; returns the `texture_id` to pass to
    /// [`Pure2dHarness::draw_sprite`].  Each registration creates one bind
    /// group (plus one texture + sampler).
    pub fn register_texture_rgba8(&mut self, width: u32, height: u32, data: &[u8]) -> u32 {
        self.renderer.register_sprite_texture(width, height, data)
    }

    /// Renders one frame into the off-screen target and returns metrics.
    pub fn render_frame(&mut self) -> FrameReport {
        let start = Instant::now();

        let device = self.renderer.context.device.clone();
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("Pure2D Headless Frame Encoder"),
        });
        self.renderer
            .render_to_view(&mut encoder, &self.target.view, None);
        self.renderer.context.queue.submit(Some(encoder.finish()));
        // lavapipe executes on the CPU; this blocks until rasterisation is done
        // so that the measured time is a stable relative signal.
        device.poll(Maintain::Wait);

        let cpu_ms = start.elapsed().as_secs_f64() * 1000.0;
        let counters = self.renderer.renderer_2d_pure.frame_counters();

        let bind_groups_total = self.renderer.sprite_bind_groups.len() as u32;
        let created = bind_groups_total.saturating_sub(self.last_bind_group_count);
        self.last_bind_group_count = bind_groups_total;

        FrameReport {
            draw_calls: counters.draw_calls,
            shape_draw_calls: counters.shape_draw_calls,
            sprite_draw_calls: counters.sprite_draw_calls,
            bytes_uploaded: counters.bytes_uploaded,
            instances_uploaded: counters.instances_uploaded,
            buffer_reallocations: counters.buffer_reallocations,
            bind_groups_created: created,
            bind_groups_total,
            cpu_ms,
        }
    }

    /// Reads back the current off-screen target as tightly packed RGBA8.
    pub fn read_pixels(&self) -> Vec<u8> {
        self.target
            .read_rgba8(&self.renderer.context.device, &self.renderer.context.queue)
    }

    /// Reads a single pixel as `[r, g, b, a]`.
    pub fn read_pixel(&self, x: u32, y: u32) -> [u8; 4] {
        self.target.read_pixel(
            &self.renderer.context.device,
            &self.renderer.context.queue,
            x,
            y,
        )
    }
}
