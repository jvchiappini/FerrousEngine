//! Headless screenshots: a story in, tightly packed RGBA8 (and a PNG) out.
//!
//! The path is the real one a window uses, minus the window:
//!
//! 1. [`Story::display_list`] lays the view out and paints the flat commands.
//! 2. [`guimaker_ferrous::encode`] maps them onto a `ferrous_ui_render::GuiBatch`
//!    with a real `ferrous_font` atlas.
//! 3. `ferrous_ui_render::GuiRenderer` draws the batch into an off-screen
//!    `Rgba8UnormSrgb` texture owned by `ferrous_renderer::headless::Pure2dHarness`.
//! 4. The harness reads the texture back into a tight RGBA8 buffer, which
//!    [`write_png`] stores as a PNG.
//!
//! `--size` is the logical viewport; the PNG is the physical target, i.e.
//! `round(logical * scale)` per axis.

use std::path::Path;

use ferrous_renderer::headless::{Pure2dHarness, DEFAULT_FORMAT};
use ferrous_ui_render::{Font, GuiBatch, GuiRenderer};
use guimaker::geom::Size;
use guimaker::theme::{ColorRole, Theme};

use crate::story::Story;
use crate::{GalleryError, Result};

/// The Roboto face the engine bundles and `FerrousTextSystem` measures with.
const ROBOTO: &[u8] = include_bytes!("../../../assets/fonts/Roboto-Regular.ttf");

/// A rendered frame: width, height and tightly packed, top-left-origin RGBA8.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderedImage {
    /// Target width in physical pixels.
    pub width: u32,
    /// Target height in physical pixels.
    pub height: u32,
    /// `width * height * 4` bytes, row-major, top-left origin.
    pub rgba: Vec<u8>,
}

impl RenderedImage {
    /// The number of pixels.
    #[must_use]
    pub fn pixel_count(&self) -> usize {
        self.width as usize * self.height as usize
    }

    /// The byte length a correctly sized buffer must have.
    #[must_use]
    pub fn byte_len(&self) -> usize {
        self.pixel_count() * 4
    }

    /// Whether the pixel buffer matches `width * height * 4`.
    #[must_use]
    pub fn is_consistent(&self) -> bool {
        self.rgba.len() == self.byte_len()
    }
}

/// Renders `story` at `logical` size and `scale`.
///
/// The physical PNG dimensions are `round(logical * scale)` per axis.
///
/// # Errors
///
/// Returns [`GalleryError::Usage`] for a degenerate viewport or scale and
/// [`GalleryError::Gpu`] when the adapter, the render or the readback fails.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
// reason: the viewport and scale are validated positive and finite, then rounded
// and floored at one pixel, so the physical dimensions fit `u32`.
pub fn render_story(
    story: Story,
    theme: &Theme,
    logical: Size,
    scale: f32,
) -> Result<RenderedImage> {
    if !(logical.width > 0.0 && logical.height > 0.0 && scale.is_finite() && scale > 0.0) {
        return Err(GalleryError::Usage(format!(
            "invalid render request: viewport {}x{} at scale {scale}",
            logical.width, logical.height
        )));
    }

    let display_list = story.display_list(theme, logical, scale);
    render_display_list(&display_list, theme, logical, scale)
}

/// Renders an already painted [`DisplayList`](guimaker::display_list::DisplayList)
/// at `logical` size and `scale`.
///
/// [`render_story`] calls this; the pixel guards use it too, so a mechanical
/// check renders through the same path as a golden.
///
/// # Errors
///
/// Returns [`GalleryError::Usage`] for a degenerate viewport or scale and
/// [`GalleryError::Gpu`] when the adapter, the render or the readback fails.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
// reason: the viewport and scale are validated positive and finite, then rounded
// and floored at one pixel, so the physical dimensions fit `u32`.
pub fn render_display_list(
    display_list: &guimaker::display_list::DisplayList,
    theme: &Theme,
    logical: Size,
    scale: f32,
) -> Result<RenderedImage> {
    if !(logical.width > 0.0 && logical.height > 0.0 && scale.is_finite() && scale > 0.0) {
        return Err(GalleryError::Usage(format!(
            "invalid render request: viewport {}x{} at scale {scale}",
            logical.width, logical.height
        )));
    }

    let width = (logical.width * scale).round().max(1.0) as u32;
    let height = (logical.height * scale).round().max(1.0) as u32;

    let harness = Pure2dHarness::new(width, height, DEFAULT_FORMAT).map_err(|error| {
        GalleryError::Gpu(format!("could not create the headless device: {error}"))
    })?;
    let device = harness.renderer().context.device.clone();
    let queue = harness.renderer().context.queue.clone();

    let font = Font::load_bytes(
        ROBOTO,
        device.as_ref(),
        queue.as_ref(),
        ferrous_font::charset::latin_western(),
    );

    let mut renderer =
        GuiRenderer::new(device.clone(), DEFAULT_FORMAT, 4096, width, height, 1, None);
    renderer.set_font_atlas(&font.atlas.view, &font.atlas.sampler);

    let mut batch = GuiBatch::new();
    guimaker_ferrous::encode(display_list, logical, scale, &mut batch, &font);
    // The batch's damage union only covers emitted primitives. A screenshot is
    // a full-frame render, so drop it and let the pass use the whole target.
    batch.damage_union = None;

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("guimaker_gallery screenshot encoder"),
    });

    // A backdrop blur must sample the layers **below** the overlay, never the
    // target it is drawing into (which already holds the overlay's own shadow).
    // Clear the target first, then hand that snapshot to the renderer: the
    // blur passes read the clear colour / 2D scene, and the GUI pass draws the
    // overlay on top of it. Without this the dialog blurred its own shadow and
    // produced the grey smears the reviewer saw.
    if display_list.commands.iter().any(|command| {
        matches!(
            command,
            guimaker::display_list::DrawCommand::BackdropBlur { .. }
        )
    }) {
        {
            let _clear = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("guimaker_gallery backdrop snapshot clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &harness.target().view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear_color(theme)),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
        }
        renderer.set_backdrop_source(&harness.target().view, width, height);
    }

    renderer.prepare(queue.as_ref(), &batch);

    renderer.render(
        &mut encoder,
        &harness.target().view,
        None,
        &batch,
        wgpu::LoadOp::Clear(clear_color(theme)),
    );
    queue.submit(Some(encoder.finish()));

    let mut rgba = Vec::new();
    harness
        .read_pixels_into(&mut rgba)
        .map_err(|error| GalleryError::Gpu(format!("readback failed: {error}")))?;

    Ok(RenderedImage {
        width,
        height,
        rgba,
    })
}

/// Writes `image` as an 8-bit RGBA PNG, creating parent directories.
///
/// # Errors
///
/// Returns [`GalleryError::Io`] or [`GalleryError::Image`] on failure.
pub fn write_png(path: impl AsRef<Path>, image: &RenderedImage) -> Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let file = std::fs::File::create(path)?;
    let writer = std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(writer, image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|error| GalleryError::Image(format!("write header: {error}")))?;
    writer
        .write_image_data(&image.rgba)
        .map_err(|error| GalleryError::Image(format!("write pixels: {error}")))?;
    Ok(())
}

/// The clear colour that matches the theme backdrop.
///
/// `Color::to_linear` is the same linear-light value the display list hands the
/// shader, and `Rgba8UnormSrgb` stores linear clear values with the sRGB
/// transfer, so the cleared pixels match the painted `Bg` rect exactly.
fn clear_color(theme: &Theme) -> wgpu::Color {
    let linear = theme.role(ColorRole::Bg).to_linear();
    wgpu::Color {
        r: f64::from(linear[0]),
        g: f64::from(linear[1]),
        b: f64::from(linear[2]),
        a: f64::from(linear[3]),
    }
}
