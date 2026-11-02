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
//! and do not grow the output buffer.
//!
//! [`Pure2dHarness`] adds an overlapping **N-slot readback ring** (three slots
//! by default, two minimum).  Each slot owns a persistent, row-padded staging
//! buffer and runs an explicit state machine:
//!
//! * `Free` — idle; [`Pure2dHarness::submit_readback`] may claim the slot.
//! * `InFlight` — `copy_texture_to_buffer` was submitted and `map_async` was
//!   queued *immediately* after it, so the GPU copy overlaps the next frame's
//!   CPU-side work.  The submission index lives on the pending entry.
//!
//! Taking a frame always unmaps the buffer and returns the slot to `Free`,
//! whether the map succeeded or failed, so a single bad map can never wedge the
//! ring.
//!
//! [`Pure2dHarness::submit_readback`] claims the next `Free` slot in
//! round-robin order and returns a harness-assigned frame index starting at
//! `0`.  [`Pure2dHarness::take_readback`] always consumes the **oldest** pending
//! frame, so results come out in submission order.  A map error is reported
//! exactly once for the affected frame; the slot is recycled and later frames
//! are unaffected.  When every slot is in flight `submit_readback` returns
//! [`ReadbackError::Busy`] and changes no state.
//!
//! Typical exporter loop (one frame in, at most one frame out per iteration):
//!
//! ```no_run
//! # use ferrous_renderer::headless::{Pure2dHarness, DEFAULT_FORMAT};
//! # fn draw(_h: &mut Pure2dHarness, _frame: u64) {}
//! let mut h = Pure2dHarness::new(640, 480, DEFAULT_FORMAT).unwrap();
//! let mut frame = Vec::new();
//! let mut encoded = 0u64;
//!
//! for i in 0..240u64 {
//!     h.begin_frame();
//!     draw(&mut h, i);
//!     h.render_frame();
//!     if let Some(frame_index) = h.readback_frame(&mut frame).unwrap() {
//!         assert_eq!(frame_index, encoded);
//!         encoded += 1;
//!         // `frame` now holds tightly packed RGBA8 for `frame_index`.
//!     }
//! }
//!
//! // Drain the frames still in flight at the end of the clip.
//! h.flush_readbacks(|frame_index, bytes| {
//!     assert_eq!(frame_index, encoded);
//!     encoded += 1;
//!     let _ = bytes;
//! })
//! .unwrap();
//! # assert_eq!(encoded, 240);
//! ```
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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Receiver;
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
        /// Harness-assigned frame index when the failure came from the
        /// pipelined ring; `None` for the immediate
        /// [`OffscreenTarget`] readback helpers.
        frame_index: Option<u64>,
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
    /// All ring slots are in flight.  Take one frame (or call
    /// [`Pure2dHarness::readback_frame`] /
    /// [`Pure2dHarness::flush_readbacks`]) before submitting another.
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
            Self::MapFailed {
                message,
                frame_index,
            } => match frame_index {
                Some(index) => write!(f, "buffer map failed for frame {index}: {message}"),
                None => write!(f, "buffer map failed: {message}"),
            },
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
                "all readback ring slots are in flight; take a frame first"
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
    /// A ring resize was requested while readback frames are still pending.
    ReadbackPending {
        /// Number of submitted frames that have not been taken yet.
        pending: usize,
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
            Self::ReadbackPending { pending } => write!(
                f,
                "cannot resize the readback ring while {pending} frame(s) are still pending"
            ),
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

        out.resize(expected, 0);
        finish_mapped_read(&self.staging, rx, padded, self.width, self.height, out)?;

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

        let mut pixel = [0u8; 4];
        finish_mapped_read(
            &self.pixel_staging,
            rx,
            wgpu::COPY_BYTES_PER_ROW_ALIGNMENT,
            1,
            1,
            &mut pixel,
        )?;
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
fn wait_for_map(rx: Receiver<Result<(), wgpu::BufferAsyncError>>) -> Result<(), ReadbackError> {
    match rx.recv() {
        Ok(Ok(())) => Ok(()),
        Ok(Err(err)) => Err(ReadbackError::MapFailed {
            message: err.to_string(),
            frame_index: None,
        }),
        Err(_) => Err(ReadbackError::ChannelClosed),
    }
}

/// Copies the padded rows of a mapped staging buffer into a tightly packed
/// destination slice.
///
/// Returns [`ReadbackError::SizeMismatch`] when either the mapped range is
/// smaller than `padded_bytes_per_row * height` or `out` cannot hold
/// `width * 4 * height` bytes.
fn copy_padded_rows_into(
    mapped: &[u8],
    padded_bytes_per_row: u32,
    width: u32,
    height: u32,
    out: &mut [u8],
) -> Result<(), ReadbackError> {
    let unpadded = (width * 4) as usize;
    let required_mapped = padded_bytes_per_row as usize * height as usize;
    let actual_mapped = mapped.len();
    if actual_mapped < required_mapped {
        return Err(ReadbackError::SizeMismatch {
            expected: required_mapped,
            actual: actual_mapped,
        });
    }

    let required_out = unpadded * height as usize;
    if out.len() < required_out {
        return Err(ReadbackError::SizeMismatch {
            expected: required_out,
            actual: out.len(),
        });
    }

    for row in 0..height as usize {
        let src = row * padded_bytes_per_row as usize;
        let dst = row * unpadded;
        out[dst..dst + unpadded].copy_from_slice(&mapped[src..src + unpadded]);
    }
    Ok(())
}

/// Waits for a queued `map_async`, copies the padded rows out and always
/// unmaps the buffer, including on error.
///
/// `out` must already be sized for `width * 4 * height` bytes (the immediate
/// readback helpers resize their `Vec` before calling this).
fn finish_mapped_read(
    buffer: &wgpu::Buffer,
    rx: Receiver<Result<(), wgpu::BufferAsyncError>>,
    padded_bytes_per_row: u32,
    width: u32,
    height: u32,
    out: &mut [u8],
) -> Result<(), ReadbackError> {
    let slice = buffer.slice(..);
    let result = (|| {
        wait_for_map(rx)?;
        let mapped = slice.get_mapped_range();
        copy_padded_rows_into(&mapped, padded_bytes_per_row, width, height, out)?;
        drop(mapped);
        Ok(())
    })();
    // Calling `unmap` after a failed map is harmless and guarantees the buffer
    // never stays in a half-mapped state that would wedge the next submit.
    buffer.unmap();
    result
}

/// State of one readback ring slot.
///
/// The ring only needs to know whether a slot is available; everything else
/// about the in-flight frame lives in the matching [`PendingReadback`] entry.
enum SlotState {
    /// Idle and available for [`Pure2dHarness::submit_readback`].
    Free,
    /// A copy was submitted and `map_async` was queued right after it.  The
    /// submission index is stored on the [`PendingReadback`] entry.
    InFlight,
}

/// One N-slot ring slot: a persistent, row-padded staging buffer plus its state.
struct ReadbackSlot {
    buffer: wgpu::Buffer,
    padded_bytes_per_row: u32,
    state: SlotState,
}

/// A submitted, not-yet-consumed pipelined readback.
struct PendingReadback {
    slot: usize,
    frame_index: u64,
    submission: wgpu::SubmissionIndex,
    width: u32,
    height: u32,
    rx: Receiver<Result<(), wgpu::BufferAsyncError>>,
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
    /// Offscreen layer passes encoded this frame (one per non-empty layer plus
    /// the final target pass; `1` when no layer was used).
    pub layer_passes: u32,
    /// Layer error reported by this frame (unbalanced push/pop, rejected push),
    /// if any.  The frame still renders; unclosed layers are closed
    /// automatically.
    pub layer_error: Option<ferrous_2d::render::LayerError>,
    /// Wall-clock duration of the submit call.  For
    /// [`Pure2dHarness::render_frame`] this **excludes** GPU completion (the
    /// call returns as soon as the command buffer is submitted); use
    /// [`Pure2dHarness::render_frame_blocking`] when the figure should include
    /// software rasterisation.  Either way this is only meaningful as a
    /// relative delta on the same machine — never as an absolute fps figure.
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
    readback_slots: Vec<ReadbackSlot>,
    pending_readbacks: VecDeque<PendingReadback>,
    next_readback_slot: usize,
    next_frame_index: u64,
    readback_scratch: Vec<u8>,
    blocking_polls: AtomicU64,
    slots_allocated: u64,
    inject_map_failure: bool,
}

/// Minimum number of staging buffers in the pipelined readback ring.
const MIN_READBACK_SLOTS: usize = 2;
/// Default number of staging buffers in the pipelined readback ring.
const DEFAULT_READBACK_SLOTS: usize = 3;

impl Pure2dHarness {
    /// Creates a headless device (fallback software adapter if needed) and a
    /// Pure2D renderer with an opaque-black clear colour.  No window and no
    /// `Surface` are created.
    ///
    /// The readback ring has three slots (the default).
    pub fn new(width: u32, height: u32, format: TextureFormat) -> Result<Self, HarnessError> {
        Self::with_readback_ring(width, height, format, DEFAULT_READBACK_SLOTS)
    }

    /// Same as [`Self::new`] but with an explicit clear colour.
    pub fn with_clear_color(
        width: u32,
        height: u32,
        format: TextureFormat,
        clear: wgpu::Color,
    ) -> Result<Self, HarnessError> {
        Self::create(width, height, format, clear, DEFAULT_READBACK_SLOTS)
    }

    /// Same as [`Self::new`] but with an explicit readback ring size.
    ///
    /// `slots` is clamped to a minimum of two: with fewer than two staging
    /// buffers there is nothing to overlap and the ring could never pipeline.
    /// Use [`Self::set_readback_ring_size`] to change the size later.
    pub fn with_readback_ring(
        width: u32,
        height: u32,
        format: TextureFormat,
        slots: usize,
    ) -> Result<Self, HarnessError> {
        Self::create(
            width,
            height,
            format,
            wgpu::Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
            slots,
        )
    }

    /// Shared constructor: creates the device, target, renderer and ring.
    fn create(
        width: u32,
        height: u32,
        format: TextureFormat,
        clear: wgpu::Color,
        slots: usize,
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

        let (readback_slots, slots_allocated) =
            Self::allocate_slots(&renderer.context.device, width, height, slots);
        let capacity = readback_slots.len();

        Ok(Self {
            renderer,
            target,
            width,
            height,
            last_bind_group_count: 0,
            readback_slots,
            pending_readbacks: VecDeque::with_capacity(capacity),
            next_readback_slot: 0,
            next_frame_index: 0,
            readback_scratch: Vec::new(),
            blocking_polls: AtomicU64::new(0),
            slots_allocated,
            inject_map_failure: false,
        })
    }

    /// Allocates `slots.max(MIN_READBACK_SLOTS)` persistent staging buffers and
    /// returns them with the number of buffers actually created.
    fn allocate_slots(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        slots: usize,
    ) -> (Vec<ReadbackSlot>, u64) {
        let count = slots.max(MIN_READBACK_SLOTS);
        let padded = padded_bytes_per_row(width.max(1));
        let size = (padded as u64 * height.max(1) as u64).max(1);
        let mut buffers = Vec::with_capacity(count);
        for index in 0..count {
            let label = format!("2D Headless Readback Ring Staging {index}");
            let buffer = device.create_buffer(&BufferDescriptor {
                label: Some(&label),
                size,
                usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            buffers.push(ReadbackSlot {
                buffer,
                padded_bytes_per_row: padded,
                state: SlotState::Free,
            });
        }
        (buffers, count as u64)
    }

    /// Resizes the readback ring, allocating new staging buffers.
    ///
    /// The ring never has fewer than two slots.  This may only be called when
    /// no readback frame is pending; otherwise it returns
    /// [`HarnessError::ReadbackPending`] and leaves the ring untouched.
    pub fn set_readback_ring_size(&mut self, slots: usize) -> Result<(), HarnessError> {
        if !self.pending_readbacks.is_empty() {
            return Err(HarnessError::ReadbackPending {
                pending: self.pending_readbacks.len(),
            });
        }
        let (readback_slots, allocated) = Self::allocate_slots(
            &self.renderer.context.device,
            self.width,
            self.height,
            slots,
        );
        let capacity = readback_slots.len();
        self.readback_slots = readback_slots;
        self.pending_readbacks = VecDeque::with_capacity(capacity);
        self.next_readback_slot = 0;
        self.slots_allocated = self.slots_allocated.saturating_add(allocated);
        Ok(())
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

    /// Selects the blend mode for subsequent 2D draws until changed.
    ///
    /// Mirrors [`Renderer::set_blend_mode_2d`]; [`Self::begin_frame`] resets the
    /// state to [`ferrous_2d::render::BlendMode2d::Normal`].
    pub fn set_blend_mode(&mut self, mode: ferrous_2d::render::BlendMode2d) {
        self.renderer.set_blend_mode_2d(mode);
    }

    /// The blend mode that applies to the next 2D draw call.
    pub fn blend_mode(&self) -> ferrous_2d::render::BlendMode2d {
        self.renderer.blend_mode_2d()
    }

    /// Opens an offscreen compositing layer (see
    /// [`Renderer::push_layer_2d`]).
    pub fn push_layer(
        &mut self,
        desc: ferrous_2d::render::Layer2d,
    ) -> Result<(), ferrous_2d::render::LayerError> {
        self.renderer.push_layer_2d(desc)
    }

    /// Opens an offscreen layer with an effect chain (see
    /// [`Renderer::push_layer_2d_with_effects`]).
    pub fn push_layer_with_effects(
        &mut self,
        desc: ferrous_2d::render::Layer2d,
        effects: &[ferrous_2d::render::Effect2d],
    ) -> Result<(), ferrous_2d::render::LayerError> {
        self.renderer.push_layer_2d_with_effects(desc, effects)
    }

    /// Opens an offscreen layer masked by a source drawn between
    /// [`Self::begin_mask`] and [`Self::end_mask`] (see
    /// [`Renderer::push_layer_2d_masked`]).
    pub fn push_masked_layer(
        &mut self,
        desc: ferrous_2d::render::Layer2d,
        effects: &[ferrous_2d::render::Effect2d],
        mask: ferrous_2d::render::Mask2d,
    ) -> Result<(), ferrous_2d::render::LayerError> {
        self.renderer.push_layer_2d_masked(desc, effects, mask)
    }

    /// Starts the mask source of the innermost masked layer.
    pub fn begin_mask(&mut self) -> Result<(), ferrous_2d::render::LayerError> {
        self.renderer.begin_mask_2d()
    }

    /// Ends the mask source and attaches it to the masked layer below it.
    pub fn end_mask(&mut self) -> Result<(), ferrous_2d::render::LayerError> {
        self.renderer.end_mask_2d()
    }

    /// Closes the innermost offscreen layer.
    pub fn pop_layer(&mut self) -> Result<(), ferrous_2d::render::LayerError> {
        self.renderer.pop_layer_2d()
    }

    /// Number of currently open offscreen layers.
    pub fn layer_depth(&self) -> u32 {
        self.renderer.layer_depth_2d()
    }

    /// Texture format used by the layered path on this adapter (`None` before
    /// the first layered frame).
    pub fn layer_texture_format(&self) -> Option<wgpu::TextureFormat> {
        self.renderer.layer_texture_format_2d()
    }

    /// Total offscreen layer textures created since the pool was invalidated.
    pub fn layer_textures_allocated(&self) -> u64 {
        self.renderer.layer_textures_allocated_2d()
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

    /// Registers an RGBA8 sprite texture whose texels are already premultiplied
    /// (see [`Renderer::register_sprite_texture_premultiplied`]).
    pub fn register_texture_rgba8_premultiplied(
        &mut self,
        width: u32,
        height: u32,
        data: &[u8],
    ) -> u32 {
        self.renderer
            .register_sprite_texture_premultiplied(width, height, data)
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

    /// Measures a whole text block (see [`Renderer::measure_text_block_2d`]).
    pub fn measure_text_block(
        &self,
        font: FontId2d,
        text: &str,
        style: ferrous_2d::render::TextStyle2d,
    ) -> Option<ferrous_font::TextMetrics> {
        self.renderer.measure_text_block_2d(font, text, style)
    }

    /// Lays out a whole text block (see [`Renderer::layout_text_2d`]).
    pub fn layout_text(
        &self,
        font: FontId2d,
        text: &str,
        style: ferrous_2d::render::TextStyle2d,
        out: &mut Vec<ferrous_font::GlyphLayout>,
    ) -> Option<ferrous_font::TextMetrics> {
        self.renderer.layout_text_2d(font, text, style, out)
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

    /// Queues one line of Pure2D text filled with a gradient (see
    /// [`Renderer::draw_text_2d_with_gradient`]).
    #[allow(clippy::too_many_arguments)] // same shape as `Renderer::draw_text_2d`'s
    pub fn draw_text_with_gradient(
        &mut self,
        font: FontId2d,
        text: &str,
        transform: glam::Mat4,
        size_px: f32,
        align: TextAlign,
        color: [f32; 4],
        gradient: Option<u32>,
    ) {
        self.renderer
            .draw_text_2d_with_gradient(font, text, transform, size_px, align, color, gradient);
    }

    /// Queues a whole text block with a style (see
    /// [`Renderer::draw_text_2d_styled`]).
    pub fn draw_text_styled(
        &mut self,
        font: FontId2d,
        text: &str,
        transform: glam::Mat4,
        style: ferrous_2d::render::TextStyle2d,
        color: [f32; 4],
    ) {
        self.renderer
            .draw_text_2d_styled(font, text, transform, style, color);
    }

    /// Queues a whole text block with a style and a gradient fill (see
    /// [`Renderer::draw_text_2d_with_gradient_styled`]).
    #[allow(clippy::too_many_arguments)] // same shape as the renderer method
    pub fn draw_text_styled_with_gradient(
        &mut self,
        font: FontId2d,
        text: &str,
        transform: glam::Mat4,
        style: ferrous_2d::render::TextStyle2d,
        color: [f32; 4],
        gradient: Option<u32>,
    ) {
        self.renderer
            .draw_text_2d_with_gradient_styled(font, text, transform, style, color, gradient);
    }

    /// Queues pre-laid-out glyphs (see [`Renderer::draw_glyphs_2d`]).
    pub fn draw_glyphs(&mut self, font: FontId2d, glyphs: &[ferrous_2d::render::GlyphDraw2d]) {
        self.renderer.draw_glyphs_2d(font, glyphs);
    }

    /// Renders one frame into the off-screen target and returns metrics.
    ///
    /// This **only submits** the command buffer; it never calls
    /// `device.poll(Maintain::Wait)`, so [`FrameReport::cpu_ms`] measures the
    /// CPU-side submit path and the GPU work overlaps the caller's next frame.
    /// Use [`Self::render_frame_blocking`] when the measurement must include
    /// rasterisation.
    pub fn render_frame(&mut self) -> FrameReport {
        self.render_frame_inner(false)
    }

    /// Renders one frame and blocks until the submission completes.
    ///
    /// Equivalent to the historical `render_frame` behaviour: it polls with
    /// `Maintain::Wait`, so [`FrameReport::cpu_ms`] includes software
    /// rasterisation.  Increments [`Self::blocking_polls`].
    pub fn render_frame_blocking(&mut self) -> FrameReport {
        self.render_frame_inner(true)
    }

    /// Shared render implementation; `blocking` selects the old wait semantics.
    fn render_frame_inner(&mut self, blocking: bool) -> FrameReport {
        let start = Instant::now();

        let device = self.renderer.context.device.clone();
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("Pure2D Headless Frame Encoder"),
        });
        self.renderer.render_to_texture_view(
            &mut encoder,
            &self.target.texture,
            &self.target.view,
            None,
        );
        self.renderer.context.queue.submit(Some(encoder.finish()));
        if blocking {
            self.blocking_polls.fetch_add(1, Ordering::Relaxed);
            // Software rasterisers execute on the CPU; this blocks until
            // rasterisation is done so the measured time is a relative signal.
            device.poll(Maintain::Wait);
        }

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
            layer_passes: self.renderer.layer_passes_2d(),
            layer_error: self.renderer.layer_error_2d(),
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

    /// Records a copy of the current target into the next free ring slot.
    ///
    /// # Ring state machine
    ///
    /// The ring starts with every slot `Free`.  This method claims the next
    /// `Free` slot in round-robin order, meaning the slot transitions
    /// `Free -> InFlight`.  The copy is submitted and `map_async` is queued
    /// **immediately**, so the GPU copy overlaps the caller's next frame.
    /// Taking the oldest pending frame returns the slot to `Free`, whether the
    /// map succeeded or failed (see [`Self::take_readback`]).
    ///
    /// Returns the harness-assigned frame index (starting at `0`, strictly
    /// increasing per successful submit).  If **all** slots are in flight this
    /// returns [`ReadbackError::Busy`] and changes no state: the pending queue,
    /// the next slot and [`Self::next_frame_index`] are untouched.
    pub fn submit_readback(&mut self) -> Result<u64, ReadbackError> {
        let slot_count = self.readback_slots.len();
        let mut chosen = None;
        for offset in 0..slot_count {
            let index = (self.next_readback_slot + offset) % slot_count;
            if matches!(self.readback_slots[index].state, SlotState::Free) {
                chosen = Some(index);
                break;
            }
        }
        let Some(slot_index) = chosen else {
            return Err(ReadbackError::Busy);
        };

        let frame_index = self.next_frame_index;
        let mut encoder =
            self.renderer
                .context
                .device
                .create_command_encoder(&CommandEncoderDescriptor {
                    label: Some("2D Headless Pipelined Readback Encoder"),
                });
        {
            let slot = &self.readback_slots[slot_index];
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
        }
        let submission = self.renderer.context.queue.submit(Some(encoder.finish()));

        // Map as soon as the copy is submitted so the callback can fire while
        // the caller renders the next frame; nothing is mapped synchronously.
        let slice = self.readback_slots[slot_index].buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(MapMode::Read, move |result| {
            let _ = tx.send(result);
        });

        self.readback_slots[slot_index].state = SlotState::InFlight;
        self.pending_readbacks.push_back(PendingReadback {
            slot: slot_index,
            frame_index,
            submission,
            width: self.width,
            height: self.height,
            rx,
        });
        self.next_readback_slot = (slot_index + 1) % slot_count;
        self.next_frame_index = self.next_frame_index.saturating_add(1);
        Ok(frame_index)
    }

    /// Maps and returns the oldest submitted readback into `out`.
    ///
    /// # Ring state machine
    ///
    /// Returns `Ok(false)` when nothing is pending.  Otherwise it polls with
    /// `Maintain::WaitForSubmissionIndex` for the oldest frame (incrementing
    /// [`Self::blocking_polls`]), copies the tight rows into `out`, unmaps the
    /// buffer and returns the slot to `Free`.  Results come out in submission
    /// order.
    ///
    /// On a map error the buffer is still unmapped, the pending entry is
    /// removed and the slot is recycled to `Free`, so the
    /// error is reported **exactly once** as
    /// [`ReadbackError::MapFailed`] with `frame_index: Some(index)` and the next
    /// call returns the next frame normally instead of repeating the failure.
    pub fn take_readback(&mut self, out: &mut Vec<u8>) -> Result<bool, ReadbackError> {
        Ok(self.take_readback_indexed(out)?.is_some())
    }

    /// Same as [`Self::take_readback`] but returns the frame index.
    ///
    /// Returns `Ok(None)` when no frame is pending.  The returned index is the
    /// harness-assigned value from [`Self::submit_readback`], so callers can
    /// detect dropped or reordered frames without maintaining their own counter.
    fn take_readback_indexed(&mut self, out: &mut Vec<u8>) -> Result<Option<u64>, ReadbackError> {
        let Some(pending) = self.pending_readbacks.pop_front() else {
            return Ok(None);
        };
        let PendingReadback {
            slot: slot_index,
            frame_index,
            submission,
            width,
            height,
            rx,
        } = pending;

        self.blocking_polls.fetch_add(1, Ordering::Relaxed);
        self.renderer
            .context
            .device
            .poll(Maintain::WaitForSubmissionIndex(submission));

        let padded = self.readback_slots[slot_index].padded_bytes_per_row;
        out.resize((width * 4 * height) as usize, 0);

        let result = finish_mapped_read(
            &self.readback_slots[slot_index].buffer,
            rx,
            padded,
            width,
            height,
            out,
        );

        // Consume the injection flag on every completed map so a natural map
        // failure cannot leak the flag into a later, healthy frame.
        let injected = std::mem::take(&mut self.inject_map_failure);
        match result {
            Ok(()) => {
                self.readback_slots[slot_index].state = SlotState::Free;
                if injected {
                    Err(ReadbackError::MapFailed {
                        message: "injected map failure (test only)".to_string(),
                        frame_index: Some(frame_index),
                    })
                } else {
                    Ok(Some(frame_index))
                }
            }
            Err(err) => {
                self.readback_slots[slot_index].state = SlotState::Free;
                Err(match err {
                    ReadbackError::MapFailed { message, .. } => ReadbackError::MapFailed {
                        message,
                        frame_index: Some(frame_index),
                    },
                    other => other,
                })
            }
        }
    }

    /// Submits the current frame's readback and, once the ring is full, returns
    /// the oldest completed frame.
    ///
    /// Call this once per frame **after** [`Self::render_frame`]:
    ///
    /// ```no_run
    /// # use ferrous_renderer::headless::{Pure2dHarness, DEFAULT_FORMAT};
    /// # fn draw(_h: &mut Pure2dHarness, _frame: u64) {}
    /// let mut h = Pure2dHarness::new(320, 240, DEFAULT_FORMAT).unwrap();
    /// let mut buf = Vec::new();
    /// let mut expected = 0u64;
    /// for i in 0..90u64 {
    ///     h.begin_frame();
    ///     draw(&mut h, i);
    ///     h.render_frame();
    ///     if let Some(frame_index) = h.readback_frame(&mut buf).unwrap() {
    ///         assert_eq!(frame_index, expected);
    ///         expected += 1;
    ///         let _ = &buf;
    ///     }
    /// }
    /// h.flush_readbacks(|frame_index, _bytes| {
    ///     assert_eq!(frame_index, expected);
    ///     expected += 1;
    /// })
    /// .unwrap();
    /// # assert_eq!(expected, 90);
    /// ```
    ///
    /// # Ring state machine
    ///
    /// If at least one slot is `Free`, the current frame only transitions a
    /// slot `Free -> InFlight` and `Ok(None)` is returned.  If every slot is in
    /// flight, the oldest frame is taken first (returning its slot to `Free`,
    /// possibly reporting [`ReadbackError::MapFailed`] once) and then the
    /// current frame claims the recycled slot; its index is `Some(index)`.
    /// Thus the first `N-1` frames return `Ok(None)` and every later call
    /// returns one frame.
    pub fn readback_frame(&mut self, out: &mut Vec<u8>) -> Result<Option<u64>, ReadbackError> {
        let all_busy = self
            .readback_slots
            .iter()
            .all(|slot| !matches!(slot.state, SlotState::Free));
        if all_busy {
            let taken = self.take_readback_indexed(out);
            let submitted = self.submit_readback();
            match taken {
                Ok(index) => {
                    submitted?;
                    Ok(index)
                }
                Err(err) => {
                    // The failed slot was recycled, so the current frame is
                    // still submitted; the map error wins because it belongs to
                    // the frame the caller expected here.
                    let _ = submitted;
                    Err(err)
                }
            }
        } else {
            self.submit_readback()?;
            Ok(None)
        }
    }

    /// Drains every pending frame in submission order, calling
    /// `f(frame_index, bytes)` for each.
    ///
    /// A map failure is logged with [`log::warn!`] and the drain continues, so
    /// every slot ends up `Free`; the first error is returned at the end.
    /// The bytes buffer is an internal scratch `Vec` reused across frames and
    /// across calls, so steady-state flushing allocates no GPU buffer and does
    /// not grow the scratch allocation.
    pub fn flush_readbacks(&mut self, mut f: impl FnMut(u64, &[u8])) -> Result<(), ReadbackError> {
        let mut first_error = None;
        let mut scratch = std::mem::take(&mut self.readback_scratch);
        loop {
            match self.take_readback_indexed(&mut scratch) {
                Ok(Some(frame_index)) => f(frame_index, &scratch),
                Ok(None) => break,
                Err(err) => {
                    log::warn!("headless readback failed while flushing: {err}");
                    if first_error.is_none() {
                        first_error = Some(err);
                    }
                }
            }
        }
        self.readback_scratch = scratch;
        match first_error {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }

    /// Number of readbacks submitted but not yet taken.
    pub fn pending_readbacks(&self) -> usize {
        self.pending_readbacks.len()
    }

    /// The harness-assigned index that the next successful
    /// [`Self::submit_readback`] will return.
    ///
    /// Starts at `0` and increases by one per successful submit; a rejected
    /// submit (for example [`ReadbackError::Busy`]) does not advance it.
    pub fn next_frame_index(&self) -> u64 {
        self.next_frame_index
    }

    /// Number of `Maintain::Wait` / `Maintain::WaitForSubmissionIndex` polls
    /// this harness has performed.
    ///
    /// `submit_readback` and the non-blocking [`Self::render_frame`] never
    /// increment it; taking a frame and [`Self::render_frame_blocking`] do.
    /// Tests use it to prove that submission and rendering overlap.
    pub fn blocking_polls(&self) -> u64 {
        self.blocking_polls.load(Ordering::Relaxed)
    }

    /// Total number of readback staging buffers this harness has ever created.
    ///
    /// This is a monotonic high-water mark, not the current ring size, so tests
    /// can prove that a steady-state loop creates no new buffers.
    pub fn readback_slots_allocated(&self) -> u64 {
        self.slots_allocated
    }

    /// Test-only hook: report the next completed map as
    /// [`ReadbackError::MapFailed`].
    ///
    /// The affected frame is still unmapped, popped and its slot recycled, so
    /// this exercises the error-recovery path without touching any other
    /// pending frame.  The flag is consumed by the next completed map.
    #[doc(hidden)]
    pub fn inject_map_failure_for_test(&mut self) {
        self.inject_map_failure = true;
    }
}
