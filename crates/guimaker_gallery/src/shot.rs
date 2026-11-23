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
use guimaker::display_list::{DisplayList, DrawCommand};
use guimaker::geom::Size;
use guimaker::theme::{ColorRole, Theme};
use wgpu::{Device, Queue};

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

/// Renders an already painted [`DisplayList`]
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
    render_display_list_with_atlas(display_list, theme, logical, scale, None)
}

/// Renders the same display list with a solid colour bound where the font
/// atlas normally sits.
///
/// This is the negative control for the backdrop-blur guards: the display list,
/// the geometry, the tint and the blur are untouched, only the glyph texture
/// changes. If two renders agree pixel for pixel in the card interior, the
/// backdrop never samples the atlas.
///
/// # Errors
///
/// Same as [`render_display_list`], plus a GPU error if the solid atlas cannot
/// be created.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
// reason: the viewport and scale are validated positive and finite, then rounded
// and floored at one pixel, so the physical dimensions fit `u32`.
pub fn render_display_list_with_solid_atlas(
    display_list: &guimaker::display_list::DisplayList,
    theme: &Theme,
    logical: Size,
    scale: f32,
    colour: [u8; 4],
) -> Result<RenderedImage> {
    render_display_list_with_atlas(display_list, theme, logical, scale, Some(colour))
}

/// Shared implementation for [`render_display_list`] and its solid-atlas
/// negative control.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
// reason: the viewport and scale are validated positive and finite, then rounded
// and floored at one pixel, so the physical dimensions fit `u32`.
fn render_display_list_with_atlas(
    display_list: &guimaker::display_list::DisplayList,
    theme: &Theme,
    logical: Size,
    scale: f32,
    solid_atlas: Option<[u8; 4]>,
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

    let font = story_font(device.as_ref(), queue.as_ref(), display_list);

    let mut renderer =
        GuiRenderer::new(device.clone(), DEFAULT_FORMAT, 4096, width, height, 1, None);
    let solid_atlas =
        solid_atlas.map(|colour| solid_atlas_texture(device.as_ref(), queue.as_ref(), colour));
    match &solid_atlas {
        Some((_texture, view, sampler)) => renderer.set_font_atlas(view, sampler),
        None => renderer.set_font_atlas(&font.atlas.view, &font.atlas.sampler),
    }
    let target = &harness.target().view;

    if let Some(split) = backdrop_split(&display_list.commands) {
        // Pass 1: paint everything below the overlay, then submit so the
        // target holds a complete scene the blur can sample.
        let mut first = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("guimaker_gallery backdrop source encoder"),
        });
        let lower = encode_commands(&display_list.commands[..split], logical, scale, &font);
        renderer.prepare(queue.as_ref(), &lower);
        renderer.render(
            &mut first,
            target,
            None,
            &lower,
            wgpu::LoadOp::Clear(clear_color(theme)),
        );
        queue.submit(Some(first.finish()));

        // The blur must read the layers already painted, never the overlay's
        // own shadow: hand it the target we just filled, then draw the suffix
        // on top with `Load`.
        renderer.set_backdrop_source(target, width, height);
        let mut second = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("guimaker_gallery overlay encoder"),
        });
        let overlay = encode_commands(&display_list.commands[split..], logical, scale, &font);
        renderer.prepare(queue.as_ref(), &overlay);
        renderer.render(&mut second, target, None, &overlay, wgpu::LoadOp::Load);
        queue.submit(Some(second.finish()));
    } else {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("guimaker_gallery screenshot encoder"),
        });
        let batch = encode_commands(&display_list.commands, logical, scale, &font);
        renderer.prepare(queue.as_ref(), &batch);
        renderer.render(
            &mut encoder,
            target,
            None,
            &batch,
            wgpu::LoadOp::Clear(clear_color(theme)),
        );
        queue.submit(Some(encoder.finish()));
    }

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

/// Builds the font atlas for a display list, with fallback faces for glyphs the
/// bundled Roboto does not cover.
///
/// The GUI text pipeline binds one font atlas, so every face's glyphs are baked
/// into **one** texture (`BakedAtlas::build_multi`): the Latin text uses Roboto,
/// CJK/Arabic/symbols come from the first system face that covers them. Only
/// the characters the list actually draws are baked, so a story with Latin text
/// pays nothing for a fallback.
fn story_font(device: &Device, queue: &Queue, display_list: &DisplayList) -> Font {
    let mut chars: Vec<char> = Vec::new();
    for command in &display_list.commands {
        if let DrawCommand::Text { run, .. } = command {
            for ch in run.text.chars() {
                if !ch.is_whitespace() && !chars.contains(&ch) {
                    chars.push(ch);
                }
            }
        }
    }
    if chars.is_empty() {
        return Font::load_bytes(
            ROBOTO,
            device,
            queue,
            ferrous_font::charset::latin_western(),
        );
    }
    let Ok(primary) = ferrous_font::FontParser::new(ROBOTO.to_vec()) else {
        return Font::load_bytes(ROBOTO, device, queue, chars);
    };
    let missing: Vec<char> = chars
        .iter()
        .copied()
        .filter(|ch| primary.get_glyph_index(*ch) == 0)
        .collect();
    // Keep the historical `latin_western` cells first so a Latin-only story
    // draws byte-identical glyphs (same UVs) as before fallback drawing; append
    // only the used characters outside that set.
    let mut primary_chars = ferrous_font::charset::latin_western();
    for ch in &chars {
        if primary.get_glyph_index(*ch) != 0 && !primary_chars.contains(ch) {
            primary_chars.push(*ch);
        }
    }
    let mut owned: Vec<(Vec<u8>, Vec<char>)> = vec![(ROBOTO.to_vec(), primary_chars)];
    for (bytes, covered) in ferrous_font::system::find_for(&missing) {
        owned.push((bytes, covered));
    }
    // A character no face covers keeps the primary's `.notdef` cell, which is
    // exactly what the single-face path drew before fallback existed.
    let covered: Vec<char> = owned.iter().flat_map(|(_, chars)| chars.clone()).collect();
    for ch in &chars {
        if !covered.contains(ch) {
            owned[0].1.push(*ch);
        }
    }
    let faces: Vec<(&[u8], Vec<char>)> = owned
        .iter()
        .map(|(bytes, covered)| (bytes.as_slice(), covered.clone()))
        .collect();
    Font::load_multi(&faces, device, queue)
        .unwrap_or_else(|_| Font::load_bytes(ROBOTO, device, queue, chars))
}

/// A one-texel atlas filled with `colour`, used as the glyph negative control.
fn solid_atlas_texture(
    device: &Device,
    queue: &Queue,
    colour: [u8; 4],
) -> (wgpu::Texture, wgpu::TextureView, wgpu::Sampler) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("guimaker_gallery solid atlas"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &colour,
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(4),
            rows_per_image: Some(1),
        },
        wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("guimaker_gallery solid atlas sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    (texture, view, sampler)
}

/// The index of the first command owned by the first backdrop-blur node.
///
/// The display list emits a card's `BackdropBlur` before its shadows and its
/// background `Rect`, so the first command of that node starts the overlay.
/// The commands before it are the layers the blur must sample; the commands
/// from it onward are the overlay drawn over the blurred snapshot.
#[must_use]
pub fn backdrop_split(commands: &[DrawCommand]) -> Option<usize> {
    let node = commands.iter().find_map(|command| match command {
        DrawCommand::BackdropBlur { node, .. } => Some(*node),
        _ => None,
    })?;
    commands.iter().position(|command| command.node() == node)
}

/// Encodes one slice of the display list into its own full-frame batch.
fn encode_commands(commands: &[DrawCommand], logical: Size, scale: f32, font: &Font) -> GuiBatch {
    let mut list = DisplayList::new();
    list.commands.extend_from_slice(commands);
    let mut batch = GuiBatch::new();
    guimaker_ferrous::encode(&list, logical, scale, &mut batch, font);
    // The batch's damage union only covers emitted primitives. A screenshot is
    // a full-frame render, so drop it and let the pass use the whole target.
    batch.damage_union = None;
    batch
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

#[cfg(test)]
mod tests {
    use super::backdrop_split;
    use guimaker::display_list::DrawCommand;
    use guimaker::geom::Rect;
    use guimaker::theme::Color;
    use guimaker::view::NodeId;

    fn rect(node: u32) -> DrawCommand {
        DrawCommand::Rect {
            node: NodeId(node),
            rect: Rect::new(0.0, 0.0, 1.0, 1.0),
            color: Color::WHITE,
            radii: [0.0; 4],
        }
    }

    fn blur(node: u32) -> DrawCommand {
        DrawCommand::BackdropBlur {
            node: NodeId(node),
            rect: Rect::new(0.0, 0.0, 1.0, 1.0),
            radius: 24.0,
            radii: [0.0; 4],
        }
    }

    #[test]
    fn splits_at_the_first_backdrop_node_command() {
        let commands = vec![rect(1), rect(2), blur(3), rect(3), rect(4)];
        assert_eq!(backdrop_split(&commands), Some(2));
    }

    #[test]
    fn splits_at_the_first_command_of_the_backdrop_node() {
        // A parent may emit a command before the blur; the split is still the
        // node's first command, so the whole overlay stays in the suffix.
        let commands = vec![rect(1), rect(3), blur(3), rect(3)];
        assert_eq!(backdrop_split(&commands), Some(1));
    }

    #[test]
    fn leaves_a_list_without_a_backdrop_whole() {
        let commands = vec![rect(1), rect(2), rect(3)];
        assert_eq!(backdrop_split(&commands), None);
    }
}
