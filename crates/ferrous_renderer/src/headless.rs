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
//!    on CI) when no hardware adapter is present.
//! 2. Builds a [`Renderer`] in [`RendererMode::Pure2D`] and renders a 2D
//!    scene into an off-screen [`wgpu::Texture`].
//! 3. Reports **hardware-independent** metrics (`draw_calls`,
//!    `bytes_uploaded`, `instances_uploaded`, buffer reallocations, bind-group
//!    creations) plus a wall-clock `cpu_ms` that is only meaningful as a
//!    relative delta on the same machine.
//!
//! # Readback
//!
//! [`OffscreenTarget::read_rgba8_into`] reuses a persistent staging buffer and
//! the caller's `Vec` allocation, so steady-state reads allocate no GPU buffer
//! and do not grow the output buffer.  [`Pure2dHarness::submit_readback`] /
//! [`Pure2dHarness::take_readback`] implement a two-slot ring so the copy of
//! frame N is overlapped with the rendering of frame N+1; results come out in
//! submission order with one frame of latency.
//!
//! # Metrics origin
//!
//! The per-draw-call / bytes-uploaded counters come from
//! [`ferrous_2d::render::Renderer2d::frame_counters`], which the engine's
//! Pure2D fast path (`renderer_core::render_internal`) drives for both shape
//! and sprite draws.  `benches/pure2d_headless.rs` installs
//! `divan::AllocProfiler` as the global allocator so `cargo bench` also
//! reports allocations per iteration.
//!
//! # Reproducing
//!
//! ```text
//! cargo test  -p ferrous_renderer --test pure2d_headless
//! cargo bench -p ferrous_renderer --bench pure2d_headless
//! ```

use std::collections::VecDeque;
use std::fmt;
use std::time::Instant;

use ferrous_2d::render::{FontId2d, ShapeInstance, SpriteInstance};
use ferrous_font::TextAlign;
use wgpu::{
    BufferDescriptor, BufferUsages, CommandEncoderDescriptor, Extent3d, ImageCopyBuffer,
    ImageCopyTexture, ImageDataLayout, Maintain, MapMode, Origin3d, TextureAspect,
    TextureDescriptor, TextureDimension, TextureFormat, TextureUsages, TextureView,
    TextureViewDescriptor,
};

use crate::renderer_core::Renderer;

/// Default off-screen colour format. Mirrors the sRGB swapchain format used by
/// native consumers so the 2D shader/AA maths runs in the same colour space as
/// production.
///
/// Verified against the engine after the colour-space fix (#12): the native
/// swapchain now prefers an `*Srgb` format (`crates/ferrous_engine/src/
/// graphics.rs`) and the 2D pipeline renders linear light into it, so
/// `Rgba8UnormSrgb` is the format the harness must use to reproduce production.
pub const DEFAULT_FORMAT: TextureFormat = TextureFormat::Rgba8UnormSrgb;

/// `true` for the two 8-bit RGBA formats the readback helpers support.
pub fn is_supported_readback_format(format: TextureFormat) -> bool {
    matches!(
        format,
        TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb
    )
}

/// Padded row stride (in bytes) required by `copy_texture_to_buffer`.
pub fn padded_bytes_per_row(width: u32) -> u32 {
    (width * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT
}

/// Errors returned by the typed readback API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadbackError {
    /// The target format is not one of [`TextureFormat::Rgba8Unorm`] /
    /// [`TextureFormat::Rgba8UnormSrgb`].
    UnsupportedFormat {
        /// The unsupported target format.
        format: TextureFormat,
    },
    /// `map_async` failed with a `wgpu::BufferAsyncError`.
    MapFailed {
        /// The error message reported by wgpu.
        message: String,
    },
    /// The map callback was dropped (typically because the device was lost).
    ChannelClosed,
    /// The mapped staging buffer was smaller than the expected frame size.
    SizeMismatch {
        /// Minimum number of bytes required.
        expected: usize,
        /// Number of bytes actually mapped.
        actual: usize,
    },
    /// `read_pixel` was asked for a texel outside the target.
    OutOfBounds {
        /// Requested x coordinate.
        x: u32,
        /// Requested y coordinate.
        y: u32,
        /// Target width.
        width: u32,
        /// Target height.
        height: u32,
    },
    /// Both pipelined readback slots are in use; call
    /// [`Pure2dHarness::take_readback`] first.
    Busy,
}

impl fmt::Display for ReadbackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedFormat { format } => {
                write!(
                    f,
                    "readback only supports Rgba8Unorm/Rgba8UnormSrgb, got {format:?}"
                )
            }
            Self::MapFailed { message } => write!(f, "buffer map failed: {message}"),
            Self::ChannelClosed => {
                write!(f, "readback channel closed (device lost?)")
            }
            Self::SizeMismatch { expected, actual } => write!(
                f,
                "mapped readback buffer too small: expected at least {expected} bytes, got {actual}"
            ),
            Self::OutOfBounds {
                x,
                y,
                width,
                height,
            } => write!(f, "pixel ({x}, {y}) is outside the {width}x{height} target"),
            Self::Busy => write!(
                f,
                "both pipelined readback slots are busy; call take_readback first"
            ),
        }
    }
}

impl std::error::Error for ReadbackError {}

/// Errors returned when creating a [`Pure2dHarness`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HarnessError {
    /// The windowless `EngineContext` could not be created.
    DeviceCreation {
        /// Underlying wgpu/adapter diagnostic.
        message: String,
    },
    /// The requested off-screen format cannot be used by the Pure2D path.
    UnsupportedFormat {
        /// The unsupported target format.
        format: TextureFormat,
    },
}

impl fmt::Display for HarnessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DeviceCreation { message } => {
                write!(f, "failed to create headless EngineContext: {message}")
            }
            Self::UnsupportedFormat { format } => {
                write!(f, "unsupported headless Pure2D format: {format:?}")
            }
        }
    }
}

impl std::error::Error for HarnessError {}

/// An off-screen RGBA8 render target that can be read back on the CPU.
///
/// The target owns a persistent, row-padded staging buffer (created once in
/// [`OffscreenTarget::new`]) and a 256-byte staging buffer for single-texel
/// reads, so repeated readbacks do not allocate GPU buffers.  No `Surface` is
/// ever created.
pub struct OffscreenTarget {
    /// The rendered texture.
    pub texture: wgpu::Texture,
    /// Default view of [`Self::texture`].
    pub view: TextureView,
    /// Target width in texels.
    pub width: u32,
    /// Target height in texels.
    pub height: u32,
    /// Target texture format.
    pub format: TextureFormat,
    staging: wgpu::Buffer,
    pixel_staging: wgpu::Buffer,
    padded_bytes_per_row: u32,
}

impl OffscreenTarget {
    /// Create an off-screen target and its persistent staging buffers.
    ///
    /// The format is not validated here; readback validates it and returns
    /// [`ReadbackError::UnsupportedFormat`] instead of panicking.
    pub fn new(device: &wgpu::Device, width: u32, height: u32, format: TextureFormat) -> Self {
        let texture = device.create_texture(&TextureDescriptor {
            label: Some("2D Headless Offscreen Target"),
            size: Extent3d {
                width: width.max(1),
                height: height.max(1),
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

        let padded = padded_bytes_per_row(width.max(1));
        let staging = device.create_buffer(&BufferDescriptor {
            label: Some("2D Headless Readback Staging"),
            size: (padded as u64 * height.max(1) as u64).max(1),
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let pixel_staging = device.create_buffer(&BufferDescriptor {
            label: Some("2D Headless Pixel Staging"),
            size: wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as u64,
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        Self {
            texture,
            view,
            width,
            height,
            format,
            staging,
            pixel_staging,
            padded_bytes_per_row: padded,
        }
    }

    /// Padded bytes per row used by the persistent staging buffer.
    pub fn padded_bytes_per_row(&self) -> u32 {
        self.padded_bytes_per_row
    }

    /// Number of staging buffers this target has allocated since creation.
    ///
    /// Always `1`: the staging buffer is persistent.  Tests use it to prove
    /// that repeated reads never create a new buffer.
    pub fn staging_allocations(&self) -> u64 {
        1
    }

    /// Copies the target into the persistent staging buffer and writes tightly
    /// packed RGBA8 bytes into `out` (resizing it, but reusing its allocation).
    ///
    /// Returns a typed error for unsupported formats, map failures, a lost
    /// device and a staging buffer smaller than expected.
    pub fn read_rgba8_into(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        out: &mut Vec<u8>,
    ) -> Result<(), ReadbackError> {
        if !is_supported_readback_format(self.format) {
            return Err(ReadbackError::UnsupportedFormat {
                format: self.format,
            });
        }

        let unpadded_bytes_per_row = self.width * 4;
        let padded = self.padded_bytes_per_row;
        let expected = (unpadded_bytes_per_row * self.height) as usize;

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
                buffer: &self.staging,
                layout: ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
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

        let slice = self.staging.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        device.poll(Maintain::Wait);
        wait_for_map(rx)?;

        let mapped = slice.get_mapped_range();
        let actual = mapped.len();
        let required = padded as usize * self.height as usize;
        if actual < required {
            drop(mapped);
            self.staging.unmap();
            return Err(ReadbackError::SizeMismatch {
                expected: required,
                actual,
            });
        }

        out.resize(expected, 0);
        let unpadded = unpadded_bytes_per_row as usize;
        for row in 0..self.height as usize {
            let src = row * padded as usize;
            let dst = row * unpadded;
            out[dst..dst + unpadded].copy_from_slice(&mapped[src..src + unpadded]);
        }
        drop(mapped);
        self.staging.unmap();

        // Return the origin-top-left convention row order that the GPU produced;
        // wgpu render targets store row 0 at the top, matching image-space Y-down.
        Ok(())
    }

    /// Legacy panicking readback.
    ///
    /// Thin wrapper over [`Self::read_rgba8_into`] kept for existing callers.
    /// It panics when the typed call fails; new code should call
    /// `read_rgba8_into` directly.
    pub fn read_rgba8(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> Vec<u8> {
        let mut out = Vec::new();
        self.read_rgba8_into(device, queue, &mut out)
            .expect("OffscreenTarget::read_rgba8 failed (legacy panicking wrapper)");
        out
    }

    /// Reads a single texel as `[r, g, b, a]`, copying only that 1x1 region.
    ///
    /// Returns [`ReadbackError::OutOfBounds`] for coordinates outside the
    /// target and [`ReadbackError::UnsupportedFormat`] for non-RGBA8 formats.
    pub fn read_pixel_into(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        x: u32,
        y: u32,
    ) -> Result<[u8; 4], ReadbackError> {
        if !is_supported_readback_format(self.format) {
            return Err(ReadbackError::UnsupportedFormat {
                format: self.format,
            });
        }
        if x >= self.width || y >= self.height {
            return Err(ReadbackError::OutOfBounds {
                x,
                y,
                width: self.width,
                height: self.height,
            });
        }

        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("2D Headless Pixel Readback Encoder"),
        });
        encoder.copy_texture_to_buffer(
            ImageCopyTexture {
                texture: &self.texture,
                mip_level: 0,
                origin: Origin3d { x, y, z: 0 },
                aspect: TextureAspect::All,
            },
            ImageCopyBuffer {
                buffer: &self.pixel_staging,
                layout: ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT),
                    rows_per_image: Some(1),
                },
            },
            Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        queue.submit(Some(encoder.finish()));

        let slice = self.pixel_staging.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        device.poll(Maintain::Wait);
        wait_for_map(rx)?;

        let mapped = slice.get_mapped_range();
        let pixel = [mapped[0], mapped[1], mapped[2], mapped[3]];
        drop(mapped);
        self.pixel_staging.unmap();
        Ok(pixel)
    }

    /// Legacy panicking single-pixel readback.
    ///
    /// Thin wrapper over [`Self::read_pixel_into`]; panics on error.
    pub fn read_pixel(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        x: u32,
        y: u32,
    ) -> [u8; 4] {
        self.read_pixel_into(device, queue, x, y)
            .expect("OffscreenTarget::read_pixel failed (legacy panicking wrapper)")
    }
}

/// Wait for a `map_async` result delivered through a channel.
fn wait_for_map(
    rx: std::sync::mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>,
) -> Result<(), ReadbackError> {
    match rx.recv() {
        Ok(Ok(())) => Ok(()),
        Ok(Err(err)) => Err(ReadbackError::MapFailed {
            message: err.to_string(),
        }),
        Err(_) => Err(ReadbackError::ChannelClosed),
    }
}

/// One pipelined readback slot (a persistent staging buffer).
struct ReadbackSlot {
    buffer: wgpu::Buffer,
    padded_bytes_per_row: u32,
}

/// A submitted, not-yet-consumed pipelined readback.
#[derive(Debug, Clone, Copy)]
struct PendingReadback {
    slot: usize,
    width: u32,
    height: u32,
}

/// Hardware-independent metrics for a single rendered frame.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct FrameReport {
    /// Total GPU draw calls issued for 2D geometry this frame.
    pub draw_calls: u32,
    /// Shape (SDF) draw calls.
    pub shape_draw_calls: u32,
    /// Sprite draw calls.
    pub sprite_draw_calls: u32,
    /// MSDF text draw calls.
    pub text_draw_calls: u32,
    /// Bytes written to GPU instance buffers this frame.
    pub bytes_uploaded: u64,
    /// Shape + sprite instances uploaded this frame.
    pub instances_uploaded: u32,
    /// Instances discarded by CPU-side viewport culling this frame (shapes +
    /// sprites).  `0` when `Render2dConfig::cull_2d` is disabled.
    pub culled_instances: u32,
    /// Retained static 2D batches drawn this frame (one draw call each).  Their
    /// instances are persistent, so they do **not** add to `instances_uploaded`
    /// / `bytes_uploaded`.
    pub static_batches_drawn: u32,
    /// Shape instances covered by the retained static batches drawn this frame.
    pub static_instances_drawn: u32,
    /// Instance buffers reallocated because the frame outgrew capacity.
    /// Steady-state frames report `0`.
    pub buffer_reallocations: u32,
    /// Sprite bind groups created during *this* frame's `render_frame` call.
    pub bind_groups_created: u32,
    /// Total sprite bind groups currently registered (including setup).
    pub bind_groups_total: u32,
    /// Wall-clock duration of `render_frame`, **including** software
    /// rasterisation because `device.poll(Maintain::Wait)` blocks until the
    /// software rasteriser finishes.  Use only as a relative delta on the same
    /// machine — never as an absolute fps figure.
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
    readback_slots: [ReadbackSlot; 2],
    pending_readbacks: VecDeque<PendingReadback>,
    next_readback_slot: usize,
}

impl Pure2dHarness {
    /// Creates a headless device (fallback software adapter if needed) and a
    /// Pure2D renderer with an opaque-black clear colour.  No window and no
    /// `Surface` are created.
    pub fn new(width: u32, height: u32, format: TextureFormat) -> Result<Self, HarnessError> {
        Self::with_clear_color(
            width,
            height,
            format,
            wgpu::Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
        )
    }

    /// Same as [`Self::new`] but with an explicit clear colour.
    pub fn with_clear_color(
        width: u32,
        height: u32,
        format: TextureFormat,
        clear: wgpu::Color,
    ) -> Result<Self, HarnessError> {
        if !is_supported_readback_format(format) {
            return Err(HarnessError::UnsupportedFormat { format });
        }
        let context =
            pollster::block_on(ferrous_core::context::EngineContext::new()).map_err(|e| {
                HarnessError::DeviceCreation {
                    message: e.to_string(),
                }
            })?;
        let target = OffscreenTarget::new(&context.device, width, height, format);
        let mut renderer = Renderer::new(context, width, height, format, 1, None);
        renderer.enable_pure_2d(clear);

        let padded = padded_bytes_per_row(width.max(1));
        let make_slot = |label: &str| ReadbackSlot {
            buffer: renderer.context.device.create_buffer(&BufferDescriptor {
                label: Some(label),
                size: (padded as u64 * height.max(1) as u64).max(1),
                usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
                mapped_at_creation: false,
            }),
            padded_bytes_per_row: padded,
        };
        let readback_slots = [
            make_slot("2D Headless Pipelined Staging 0"),
            make_slot("2D Headless Pipelined Staging 1"),
        ];

        Ok(Self {
            renderer,
            target,
            width,
            height,
            last_bind_group_count: 0,
            readback_slots,
            pending_readbacks: VecDeque::with_capacity(2),
            next_readback_slot: 0,
        })
    }

    /// Target width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Target height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The off-screen target.
    pub fn target(&self) -> &OffscreenTarget {
        &self.target
    }

    /// The Pure2D renderer.
    pub fn renderer(&self) -> &Renderer {
        &self.renderer
    }

    /// Mutable access to the Pure2D renderer.
    pub fn renderer_mut(&mut self) -> &mut Renderer {
        &mut self.renderer
    }

    /// The wgpu device.
    pub fn device(&self) -> &wgpu::Device {
        &self.renderer.context.device
    }

    /// The wgpu queue.
    pub fn queue(&self) -> &wgpu::Queue {
        &self.renderer.context.queue
    }

    /// The selected backend.
    pub fn backend(&self) -> wgpu::Backend {
        self.renderer.context.backend
    }

    /// The selected adapter name (useful in test output).
    pub fn adapter_name(&self) -> String {
        self.renderer.context.adapter.get_info().name
    }

    /// Replaces the clear colour used from the next frame on.
    pub fn set_clear_color(&mut self, color: wgpu::Color) {
        self.renderer.set_clear_color(color);
    }

    /// Clears both 2D batchers and resets the per-frame counters.  Call once at
    /// the start of every frame (mirrors `AppContext::render.clear_2d()`).
    pub fn begin_frame(&mut self) {
        self.renderer.clear_2d();
        self.renderer.renderer_2d_pure.reset_frame_counters();
    }

    /// Queues one shape for this frame.
    pub fn draw_shape(&mut self, instance: ShapeInstance) {
        self.renderer.draw_2d_shape(instance);
    }

    /// Queues one sprite for this frame.
    pub fn draw_sprite(&mut self, texture_id: u32, instance: SpriteInstance) {
        self.renderer.draw_sprite_2d(texture_id, instance);
    }

    /// Registers an RGBA8 sprite texture; returns the `texture_id` to pass to
    /// [`Pure2dHarness::draw_sprite`].  Each registration creates one bind
    /// group (plus one texture + sampler).
    pub fn register_texture_rgba8(&mut self, width: u32, height: u32, data: &[u8]) -> u32 {
        self.renderer.register_sprite_texture(width, height, data)
    }

    /// Bakes and registers a 2D font atlas (see
    /// [`Renderer::register_font_2d`]).
    pub fn register_font<I>(
        &mut self,
        bytes: &[u8],
        charset: I,
    ) -> Result<FontId2d, ferrous_font::FontError>
    where
        I: IntoIterator<Item = char>,
    {
        self.renderer.register_font_2d(bytes, charset)
    }

    /// Frees a 2D font registered with [`Self::register_font`].
    pub fn free_font(&mut self, font: FontId2d) {
        self.renderer.free_font_2d(font);
    }

    /// Measures one line of Pure2D text (see [`Renderer::measure_text_2d`]).
    pub fn measure_text(
        &self,
        font: FontId2d,
        text: &str,
        size_px: f32,
        align: TextAlign,
    ) -> Option<ferrous_font::LineMetrics> {
        self.renderer.measure_text_2d(font, text, size_px, align)
    }

    /// Queues one line of Pure2D text (see [`Renderer::draw_text_2d`]).
    pub fn draw_text(
        &mut self,
        font: FontId2d,
        text: &str,
        transform: glam::Mat4,
        size_px: f32,
        align: TextAlign,
        color: [f32; 4],
    ) {
        self.renderer
            .draw_text_2d(font, text, transform, size_px, align, color);
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
        // Software rasterisers execute on the CPU; this blocks until
        // rasterisation is done so that the measured time is a relative signal.
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
            text_draw_calls: counters.text_draw_calls,
            bytes_uploaded: counters.bytes_uploaded,
            instances_uploaded: counters.instances_uploaded,
            culled_instances: counters.culled_instances,
            static_batches_drawn: counters.static_batches_drawn,
            static_instances_drawn: counters.static_instances_drawn,
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

    /// Typed readback into a reusable buffer (see
    /// [`OffscreenTarget::read_rgba8_into`]).
    pub fn read_pixels_into(&self, out: &mut Vec<u8>) -> Result<(), ReadbackError> {
        self.target.read_rgba8_into(
            &self.renderer.context.device,
            &self.renderer.context.queue,
            out,
        )
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

    /// Typed single-pixel read (see [`OffscreenTarget::read_pixel_into`]).
    pub fn read_pixel_checked(&self, x: u32, y: u32) -> Result<[u8; 4], ReadbackError> {
        self.target.read_pixel_into(
            &self.renderer.context.device,
            &self.renderer.context.queue,
            x,
            y,
        )
    }

    /// Records a copy of the current target into the next pipelined slot.
    ///
    /// The copy is submitted immediately, so it overlaps the next frame's
    /// rendering.  At most two copies may be in flight; returns
    /// [`ReadbackError::Busy`] when both slots are occupied.
    pub fn submit_readback(&mut self) -> Result<(), ReadbackError> {
        if self.pending_readbacks.len() >= 2 {
            return Err(ReadbackError::Busy);
        }
        let slot_index = self.next_readback_slot;
        let slot = &self.readback_slots[slot_index];

        let mut encoder =
            self.renderer
                .context
                .device
                .create_command_encoder(&CommandEncoderDescriptor {
                    label: Some("2D Headless Pipelined Readback Encoder"),
                });
        encoder.copy_texture_to_buffer(
            ImageCopyTexture {
                texture: &self.target.texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            ImageCopyBuffer {
                buffer: &slot.buffer,
                layout: ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(slot.padded_bytes_per_row),
                    rows_per_image: Some(self.height),
                },
            },
            Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        self.renderer.context.queue.submit(Some(encoder.finish()));

        self.pending_readbacks.push_back(PendingReadback {
            slot: slot_index,
            width: self.width,
            height: self.height,
        });
        self.next_readback_slot = (slot_index + 1) % self.readback_slots.len();
        Ok(())
    }

    /// Maps and returns the oldest submitted readback into `out`.
    ///
    /// Returns `Ok(false)` when nothing is pending.  Results come out in
    /// submission order; the first frame's bytes are available one submit
    /// later (one frame of latency).
    pub fn take_readback(&mut self, out: &mut Vec<u8>) -> Result<bool, ReadbackError> {
        let Some(pending) = self.pending_readbacks.front().copied() else {
            return Ok(false);
        };
        let slot = &self.readback_slots[pending.slot];
        let device = &self.renderer.context.device;

        let slice = slot.buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        device.poll(Maintain::Wait);
        wait_for_map(rx)?;

        let mapped = slice.get_mapped_range();
        let actual = mapped.len();
        let unpadded = (pending.width * 4) as usize;
        let required = slot.padded_bytes_per_row as usize * pending.height as usize;
        if actual < required {
            drop(mapped);
            slot.buffer.unmap();
            return Err(ReadbackError::SizeMismatch {
                expected: required,
                actual,
            });
        }

        out.resize(unpadded * pending.height as usize, 0);
        for row in 0..pending.height as usize {
            let src = row * slot.padded_bytes_per_row as usize;
            let dst = row * unpadded;
            out[dst..dst + unpadded].copy_from_slice(&mapped[src..src + unpadded]);
        }
        drop(mapped);
        slot.buffer.unmap();
        self.pending_readbacks.pop_front();
        Ok(true)
    }

    /// Number of readbacks submitted but not yet taken.
    pub fn pending_readbacks(&self) -> usize {
        self.pending_readbacks.len()
    }
}
