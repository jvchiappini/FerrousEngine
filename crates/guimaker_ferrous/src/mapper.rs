//! Maps a `GUIMaker` display list onto a `FerrousEngine` [`GuiBatch`].
//!
//! One command becomes one or more batch primitives; the mapping never
//! reorders commands, so painter order is the display list's order. All logical
//! pixels are multiplied by the frame `scale` before they reach the batch.

use ferrous_svg::{SvgMesh, SvgRect};
use ferrous_ui_render::text_utils::BaselineGlyphs;
use ferrous_ui_render::{
    BackdropBlurSpec, BorderSpec, Font, GuiBatch, GuiQuad, Rect as EngineRect,
    ShadowSpec as BatchShadowSpec, SvgCommand, TextQuad, GRADIENT_BIT, GRADIENT_RADIAL_BIT,
    GRADIENT_V_BIT, TEXTURED_BIT,
};
use guimaker::display_list::{
    DisplayList, DrawCommand, Gradient, ImageFit, TextRun, TextSpanStyle,
};
use guimaker::geom::{Rect, Size};
use guimaker::theme::tokens::BACKDROP_TINT_ALPHA;
use guimaker::theme::Color;

use crate::texture::TextureMap;

/// A rectangle in physical pixels, after the frame scale is applied.
#[derive(Clone, Copy, Debug)]
struct RectSpec {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl RectSpec {
    /// Scales a logical rectangle by `scale`.
    fn scaled(rect: Rect, scale: f32) -> Self {
        Self {
            x: rect.x * scale,
            y: rect.y * scale,
            width: rect.width * scale,
            height: rect.height * scale,
        }
    }

    /// Whether either side is zero or negative.
    fn is_empty(self) -> bool {
        self.width <= 0.0 || self.height <= 0.0
    }

    /// Borrows the rectangle as the engine's scissor/`Rect` type.
    fn to_engine(self) -> EngineRect {
        EngineRect {
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
        }
    }
}

/// A shadow's colour, blur, spread and offset in physical pixels.
#[derive(Clone, Copy, Debug)]
struct ShadowSpec {
    color: [f32; 4],
    blur: f32,
    spread: f32,
    offset: [f32; 2],
}

/// A backdrop blur that waits for the owning node's background rect.
///
/// The display list emits `BackdropBlur` before the card's shadows and its
/// background `Rect`. Deferring the blur until that `Rect` paints it through
/// `GuiBatch::backdrop_blur` with the rect's colour as a translucent tint, so
/// the shadow keeps painting below it.
#[derive(Clone, Copy, Debug)]
struct PendingBackdrop {
    node: u32,
    rect: RectSpec,
    radii: [f32; 4],
    radius: f32,
}

/// Maps a [`DisplayList`] onto [`GuiBatch`] with a reusable image registry.
///
/// The renderer owns the [`TextureMap`] and the temporary layer-opacity stack.
/// It is safe to reuse across frames: [`GuiMakerRenderer::encode`] resets the
/// opacity state and keeps the textures registered by the host.
pub struct GuiMakerRenderer {
    textures: TextureMap,
    opacity: f32,
    opacity_stack: Vec<f32>,
    pending_backdrop: Option<PendingBackdrop>,
}

impl Default for GuiMakerRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl GuiMakerRenderer {
    /// Builds a renderer with an empty texture map.
    #[must_use]
    pub fn new() -> Self {
        Self {
            textures: TextureMap::new(),
            opacity: 1.0,
            opacity_stack: Vec::new(),
            pending_backdrop: None,
        }
    }

    /// Builds a renderer that resolves images through `textures`.
    #[must_use]
    pub fn with_textures(textures: TextureMap) -> Self {
        Self {
            textures,
            ..Self::new()
        }
    }

    /// The image registry.
    #[must_use]
    pub fn textures(&self) -> &TextureMap {
        &self.textures
    }

    /// The image registry, mutable so a host can register or evict textures.
    pub fn textures_mut(&mut self) -> &mut TextureMap {
        &mut self.textures
    }

    /// Maps `display_list` into `gui` using `font` for text.
    ///
    /// `viewport` is the logical target size and must be positive; `scale` is
    /// the logical-to-physical factor and must be finite and positive.
    pub fn encode(
        &mut self,
        display_list: &DisplayList,
        viewport: Size,
        scale: f32,
        gui: &mut GuiBatch,
        font: &Font,
    ) {
        self.encode_with(display_list, viewport, scale, gui, Some(font));
    }

    fn encode_with<F: PaintFont + ?Sized>(
        &mut self,
        display_list: &DisplayList,
        viewport: Size,
        scale: f32,
        gui: &mut GuiBatch,
        font: Option<&F>,
    ) {
        self.pending_backdrop = None;
        if !(viewport.width > 0.0 && viewport.height > 0.0 && scale.is_finite() && scale > 0.0) {
            return;
        }
        self.opacity = 1.0;
        self.opacity_stack.clear();

        for command in &display_list.commands {
            self.encode_command(command, scale, gui, font);
        }
        if let Some(pending) = self.pending_backdrop.take() {
            Self::emit_backdrop(gui, pending, Color::TRANSPARENT.to_linear());
        }
    }

    /// Dispatches one display-list command to its handler.
    fn encode_command<F: PaintFont + ?Sized>(
        &mut self,
        command: &DrawCommand,
        scale: f32,
        gui: &mut GuiBatch,
        font: Option<&F>,
    ) {
        match command {
            DrawCommand::Rect {
                node,
                rect,
                color,
                radii,
            } => {
                let spec = RectSpec::scaled(*rect, scale);
                let radii = scale_radii(*radii, scale);
                self.encode_rect(gui, node.0, spec, *color, radii);
            }
            DrawCommand::Border {
                node,
                rect,
                color,
                width,
                sides,
                radii,
            } => {
                self.encode_border_command(
                    gui, node.0, *rect, *color, *width, *sides, *radii, scale,
                );
            }
            DrawCommand::Shadow {
                node,
                rect,
                color,
                offset,
                blur,
                spread,
                radii,
            } => {
                self.encode_shadow_command(
                    gui, node.0, *rect, *color, *offset, *blur, *spread, *radii, scale,
                );
            }
            DrawCommand::Gradient {
                node,
                rect,
                gradient,
                radii,
            } => {
                let spec = RectSpec::scaled(*rect, scale);
                let radii = scale_radii(*radii, scale);
                self.encode_gradient(gui, node.0, spec, gradient, radii);
            }
            DrawCommand::Text { node, rect, run } => {
                self.encode_text(gui, node.0, *rect, run, scale, font);
            }
            DrawCommand::Image {
                node,
                rect,
                source,
                fit,
                radii,
            } => {
                self.encode_image_command(gui, node.0, *rect, *source, *fit, *radii, scale);
            }
            DrawCommand::Icon {
                node,
                rect,
                path,
                color,
            } => {
                self.encode_icon_command(gui, node.0, *rect, path, *color, scale);
            }
            DrawCommand::Path {
                node,
                points,
                width,
                color,
            } => {
                self.encode_path(gui, node.0, points, *width, *color, scale);
            }
            DrawCommand::ClipPush { rect, .. } => {
                Self::encode_clip_push(gui, RectSpec::scaled(*rect, scale));
            }
            DrawCommand::ClipPop { .. } => gui.pop_clip(),
            DrawCommand::LayerPush { opacity, .. } => self.encode_layer_push(gui, *opacity),
            DrawCommand::LayerPop { .. } => self.encode_layer_pop(gui),
            DrawCommand::BackdropBlur {
                node,
                rect,
                radius,
                radii,
            } => {
                self.pending_backdrop = Some(PendingBackdrop {
                    node: node.0,
                    rect: RectSpec::scaled(*rect, scale),
                    radii: scale_radii(*radii, scale),
                    radius: radius * scale,
                });
            }
        }
    }

    /// Scales and emits an image quad.
    #[allow(clippy::too_many_arguments)] // reason: the command carries its rect, source, fit, radii and the scale
    fn encode_image_command(
        &self,
        gui: &mut GuiBatch,
        node: u32,
        rect: Rect,
        source: u64,
        fit: ImageFit,
        radii: [f32; 4],
        scale: f32,
    ) {
        let spec = RectSpec::scaled(rect, scale);
        let radii = scale_radii(radii, scale);
        self.encode_image(gui, node, spec, radii, source, fit);
    }

    /// Scales and emits an icon.
    fn encode_icon_command(
        &self,
        gui: &mut GuiBatch,
        node: u32,
        rect: Rect,
        path: &str,
        color: Color,
        scale: f32,
    ) {
        let spec = RectSpec::scaled(rect, scale);
        self.encode_icon(gui, node, spec, path, color);
    }

    /// Scales and emits a border stroke.
    #[allow(clippy::too_many_arguments)] // reason: the command carries geometry, colour, one width, per-edge widths and radii
    fn encode_border_command(
        &self,
        gui: &mut GuiBatch,
        node: u32,
        rect: Rect,
        color: Color,
        width: f32,
        sides: [f32; 4],
        radii: [f32; 4],
        scale: f32,
    ) {
        let spec = RectSpec::scaled(rect, scale);
        let radii = scale_radii(radii, scale);
        let sides = sides.map(|side| side * scale);
        self.encode_border(gui, node, spec, color, width * scale, sides, radii);
    }

    /// Scales and emits a shadow quad.
    #[allow(clippy::too_many_arguments)] // reason: a shadow command carries its rect, colour, offset, blur, spread and radii
    fn encode_shadow_command(
        &self,
        gui: &mut GuiBatch,
        node: u32,
        rect: Rect,
        color: Color,
        offset: glam::Vec2,
        blur: f32,
        spread: f32,
        radii: [f32; 4],
        scale: f32,
    ) {
        let spec = RectSpec::scaled(rect, scale);
        let radii = scale_radii(radii, scale);
        let shadow = ShadowSpec {
            color: self.color(color),
            blur: blur * scale,
            spread: spread * scale,
            offset: [offset.x * scale, offset.y * scale],
        };
        Self::encode_shadow(gui, node, spec, shadow, radii);
    }

    /// Scales and emits a stroked polyline.
    fn encode_path(
        &self,
        gui: &mut GuiBatch,
        node: u32,
        points: &[glam::Vec2],
        width: f32,
        color: Color,
        scale: f32,
    ) {
        let scaled: Vec<[f32; 2]> = points
            .iter()
            .map(|point| [point.x * scale, point.y * scale])
            .collect();
        let color = self.color(color);
        emit(gui, node, |gui| {
            gui.polyline(&scaled, width * scale, color);
        });
    }

    /// Emits a rect, or its node's deferred backdrop blur when one is pending.
    fn encode_rect(
        &mut self,
        gui: &mut GuiBatch,
        node: u32,
        rect: RectSpec,
        color: Color,
        radii: [f32; 4],
    ) {
        let pending = match self.pending_backdrop {
            Some(pending) if pending.node == node => {
                self.pending_backdrop = None;
                Some(pending)
            }
            _ => None,
        };
        if let Some(pending) = pending {
            let mut tint = self.color(color);
            tint[3] *= BACKDROP_TINT_ALPHA;
            Self::emit_backdrop(gui, pending, tint);
            return;
        }
        self.encode_shape(gui, node, rect, color, radii);
    }

    /// Emits one deferred backdrop-blur quad and stamps its node id.
    fn emit_backdrop(gui: &mut GuiBatch, pending: PendingBackdrop, tint: [f32; 4]) {
        if pending.rect.is_empty() {
            return;
        }
        emit(gui, pending.node, |gui| {
            gui.backdrop_blur(BackdropBlurSpec {
                rect: pending.rect.to_engine(),
                radii: pending.radii,
                tint,
                blur_radius: pending.radius,
            });
        });
    }

    /// Emits a filled rounded rectangle.
    fn encode_shape(
        &self,
        gui: &mut GuiBatch,
        node: u32,
        rect: RectSpec,
        color: Color,
        radii: [f32; 4],
    ) {
        if rect.is_empty() {
            return;
        }
        let color = self.color(color);
        emit(gui, node, |gui| {
            gui.rect_radii(rect.x, rect.y, rect.width, rect.height, color, radii);
        });
    }

    /// Emits a border stroke around a rounded rectangle.
    #[allow(clippy::too_many_arguments)] // reason: the border carries geometry, colour, one width, per-edge widths and radii
    fn encode_border(
        &self,
        gui: &mut GuiBatch,
        node: u32,
        rect: RectSpec,
        color: Color,
        width: f32,
        sides: [f32; 4],
        radii: [f32; 4],
    ) {
        let uniform = sides.iter().all(|side| (side - width).abs() < f32::EPSILON);
        if rect.is_empty() || (uniform && width <= 0.0) {
            return;
        }
        let color = self.color(color);
        emit(gui, node, |gui| {
            let mut spec =
                BorderSpec::new(rect.x, rect.y, rect.width, rect.height, color, radii, width);
            if !uniform {
                spec = spec.with_sides(sides);
            }
            gui.border(spec);
        });
    }

    /// Emits a shadow quad.
    fn encode_shadow(
        gui: &mut GuiBatch,
        node: u32,
        rect: RectSpec,
        shadow: ShadowSpec,
        radii: [f32; 4],
    ) {
        if rect.is_empty() {
            return;
        }
        emit(gui, node, |gui| {
            gui.shadow(BatchShadowSpec {
                rect: EngineRect {
                    x: rect.x,
                    y: rect.y,
                    width: rect.width,
                    height: rect.height,
                },
                color: shadow.color,
                blur_radius: shadow.blur,
                spread: shadow.spread,
                offset: shadow.offset,
                radii,
            });
        });
    }

    /// Emits a linear or radial gradient quad.
    fn encode_gradient(
        &self,
        gui: &mut GuiBatch,
        node: u32,
        rect: RectSpec,
        gradient: &Gradient,
        radii: [f32; 4],
    ) {
        if rect.is_empty() {
            return;
        }
        let Some((start, end, flags)) = self.gradient(gradient) else {
            return;
        };
        emit(gui, node, |gui| {
            gui.push_quad(GuiQuad {
                pos: [rect.x, rect.y],
                size: [rect.width, rect.height],
                uv0: [0.0, 0.0],
                uv1: [1.0, 1.0],
                color: start,
                color_b: end,
                radii,
                tex_index: 0,
                flags,
                z_order: 0.0,
                node_id: 0,
            });
        });
    }

    /// Emits every line of a text run, when a font is available.
    fn encode_text<F: PaintFont + ?Sized>(
        &self,
        gui: &mut GuiBatch,
        node: u32,
        rect: Rect,
        run: &TextRun,
        scale: f32,
        font: Option<&F>,
    ) {
        let Some(font) = font else {
            return;
        };
        emit(gui, node, |gui| {
            self.text(gui, font, rect, run, scale);
        });
    }

    /// Emits an image quad, texturing it when the source is registered.
    fn encode_image(
        &self,
        gui: &mut GuiBatch,
        node: u32,
        rect: RectSpec,
        radii: [f32; 4],
        source: u64,
        fit: ImageFit,
    ) {
        let Some(texture) = self.textures.get(source).cloned() else {
            return;
        };
        if rect.is_empty() {
            return;
        }
        let image = [
            dimension_f32(texture.texture.width()),
            dimension_f32(texture.texture.height()),
        ];
        let (dest, uv0, uv1) = image_geometry(rect, image, fit);
        let color = [1.0, 1.0, 1.0, self.opacity];
        if radii.iter().all(|radius| *radius <= 0.0) {
            emit(gui, node, |gui| {
                gui.image(
                    dest.x,
                    dest.y,
                    dest.width,
                    dest.height,
                    texture,
                    uv0,
                    uv1,
                    color,
                );
            });
        } else {
            let slot = gui.reserve_texture_slot(texture);
            emit(gui, node, |gui| {
                gui.push_quad(GuiQuad {
                    pos: [dest.x, dest.y],
                    size: [dest.width, dest.height],
                    uv0,
                    uv1,
                    color,
                    color_b: [0.0; 4],
                    radii,
                    tex_index: slot,
                    flags: TEXTURED_BIT,
                    z_order: 0.0,
                    node_id: 0,
                });
            });
        }
    }

    /// Emits an icon as an SVG mesh, or a plain rectangle when unsupported.
    fn encode_icon(&self, gui: &mut GuiBatch, node: u32, rect: RectSpec, path: &str, color: Color) {
        if rect.is_empty() {
            return;
        }
        let color = self.color(color);
        if let Some(mesh) = icon_mesh(path, rect.width, rect.height) {
            let pos = [rect.x, rect.y];
            emit(gui, node, |gui| {
                gui.push_svg(SvgCommand {
                    mesh,
                    pos,
                    color,
                    z: 0.0,
                });
            });
        } else {
            emit(gui, node, |gui| {
                gui.rect_radii(rect.x, rect.y, rect.width, rect.height, color, [0.0; 4]);
            });
        }
    }

    /// Pushes the rectangular scissor for a clip.
    ///
    /// The pinned engine's scissor stack is rectangular. The command's radii
    /// are a documented fallback until the rounded-clip gap lane lands; see
    /// docs/backend.md.
    fn encode_clip_push(gui: &mut GuiBatch, rect: RectSpec) {
        gui.push_clip(rect.to_engine());
    }

    /// Opens a layer and folds its opacity into the current opacity.
    fn encode_layer_push(&mut self, gui: &mut GuiBatch, opacity: f32) {
        gui.begin_layer();
        self.opacity_stack.push(self.opacity);
        self.opacity *= opacity.clamp(0.0, 1.0);
    }

    /// Restores the opacity saved by the matching layer push.
    fn encode_layer_pop(&mut self, gui: &mut GuiBatch) {
        self.opacity = self.opacity_stack.pop().unwrap_or(1.0);
        gui.begin_layer();
    }

    fn color(&self, color: Color) -> [f32; 4] {
        let mut linear = color.to_linear();
        linear[3] *= self.opacity;
        linear
    }

    fn gradient(&self, gradient: &Gradient) -> Option<([f32; 4], [f32; 4], u32)> {
        let (stops, base_flags) = match gradient {
            Gradient::Linear { angle, stops } => {
                let mut flags = GRADIENT_BIT;
                if angle.sin().abs() > angle.cos().abs() {
                    flags |= GRADIENT_V_BIT;
                }
                (stops, flags)
            }
            Gradient::Radial { stops, .. } => (stops, GRADIENT_BIT | GRADIENT_RADIAL_BIT),
        };
        let first = stops.first()?;
        if stops.len() < 2 {
            let color = self.color(first.color);
            return Some((color, color, 0));
        }
        let last = stops.last()?;
        Some((self.color(first.color), self.color(last.color), base_flags))
    }

    fn text<F: PaintFont + ?Sized>(
        &self,
        gui: &mut GuiBatch,
        font: &F,
        rect: Rect,
        run: &TextRun,
        scale: f32,
    ) {
        let size = run.style.size * scale;
        if size <= 0.0 {
            return;
        }
        let tracking = run.style.tracking;
        let base = run.spans.first().map_or(Color::WHITE, |span| span.color);
        for line in run.lines.iter() {
            let start = line.start.min(run.text.len());
            let end = line.end.min(run.text.len());
            if end <= start {
                continue;
            }
            let text = &run.text[start..end];
            let baseline = (rect.y + line.baseline) * scale;
            let pen = rect.x * scale;
            if run.spans.is_empty() && tracking == 0.0 {
                let color = self.color(base);
                font.draw_baseline(gui, text, [pen, baseline], size, color);
            } else {
                paint_line(
                    gui,
                    font,
                    &LineRun {
                        text,
                        origin: [pen, baseline],
                        size,
                        tracking,
                        base,
                        spans: &run.spans,
                        line_start: start,
                        opacity: self.opacity,
                    },
                );
            }
        }
    }
}

/// The stateless convenience entry point.
///
/// It builds a temporary [`GuiMakerRenderer`] with an empty texture map, so
/// [`DrawCommand::Image`] commands are skipped. Use [`GuiMakerRenderer`] when
/// the display list carries images.
pub fn encode(
    display_list: &DisplayList,
    viewport: Size,
    scale: f32,
    gui: &mut GuiBatch,
    font: &Font,
) {
    GuiMakerRenderer::new().encode(display_list, viewport, scale, gui, font);
}

/// Converts a texture dimension (`u32`) to the `f32` geometry space.
///
/// `wgpu` caps texture dimensions at 32768, far below `f32`'s 2^24 exact
/// integer range, so the conversion cannot lose precision.
#[allow(clippy::cast_precision_loss)]
fn dimension_f32(dimension: u32) -> f32 {
    dimension as f32
}

fn scale_radii(radii: [f32; 4], scale: f32) -> [f32; 4] {
    [
        radii[0] * scale,
        radii[1] * scale,
        radii[2] * scale,
        radii[3] * scale,
    ]
}

/// Runs `draw` and stamps the node id on every primitive it produced.
fn emit(gui: &mut GuiBatch, node: u32, draw: impl FnOnce(&mut GuiBatch)) {
    let quads = gui.quads.len();
    let texts = gui.text_quads.len();
    let icons = gui.icon_quads.len();
    draw(gui);
    for quad in &mut gui.quads[quads..] {
        quad.node_id = node;
    }
    for quad in &mut gui.text_quads[texts..] {
        quad.node_id = node;
    }
    for quad in &mut gui.icon_quads[icons..] {
        quad.node_id = node;
    }
}

/// Returns the destination rectangle and UVs for an image fit.
fn image_geometry(
    rect: RectSpec,
    image: [f32; 2],
    fit: ImageFit,
) -> (RectSpec, [f32; 2], [f32; 2]) {
    let [x, y, w, h] = [rect.x, rect.y, rect.width, rect.height];
    let [image_w, image_h] = image;
    if image_w <= 0.0 || image_h <= 0.0 || w <= 0.0 || h <= 0.0 {
        return (rect, [0.0, 0.0], [1.0, 1.0]);
    }
    match fit {
        ImageFit::Fill => (rect, [0.0, 0.0], [1.0, 1.0]),
        ImageFit::Contain => {
            let target = w / h;
            let source = image_w / image_h;
            if source > target {
                let height = w / source;
                let dest = RectSpec {
                    x,
                    y: y + (h - height) * 0.5,
                    width: w,
                    height,
                };
                (dest, [0.0, 0.0], [1.0, 1.0])
            } else {
                let width = h * source;
                let dest = RectSpec {
                    x: x + (w - width) * 0.5,
                    y,
                    width,
                    height: h,
                };
                (dest, [0.0, 0.0], [1.0, 1.0])
            }
        }
        ImageFit::Cover => {
            let target = w / h;
            let source = image_w / image_h;
            if source > target {
                let visible = target / source;
                let left = (1.0 - visible) * 0.5;
                (rect, [left, 0.0], [left + visible, 1.0])
            } else {
                let visible = source / target;
                let top = (1.0 - visible) * 0.5;
                (rect, [0.0, top], [1.0, top + visible])
            }
        }
    }
}

/// Tessellates the icon SVG/path data and scales its vertices into the icon
/// box. `ferrous_svg` parses `<path d>`, raw path data and the basic shapes;
/// unsupported input returns `None` and the caller draws the fallback.
fn icon_mesh(path: &str, width: f32, height: f32) -> Option<SvgMesh> {
    let mut mesh = SvgMesh::from_str(path).ok()?;
    if mesh.vertices.is_empty() {
        return None;
    }
    let mut min = [f32::INFINITY, f32::INFINITY];
    let mut max = [f32::NEG_INFINITY, f32::NEG_INFINITY];
    for vertex in &mesh.vertices {
        min[0] = min[0].min(vertex.position[0]);
        min[1] = min[1].min(vertex.position[1]);
        max[0] = max[0].max(vertex.position[0]);
        max[1] = max[1].max(vertex.position[1]);
    }
    let span_x = (max[0] - min[0]).max(f32::EPSILON);
    let span_y = (max[1] - min[1]).max(f32::EPSILON);
    for vertex in &mut mesh.vertices {
        vertex.position = [
            (vertex.position[0] - min[0]) / span_x * width,
            (vertex.position[1] - min[1]) / span_y * height,
        ];
    }
    mesh.bounding_box = SvgRect {
        x: 0.0,
        y: 0.0,
        width,
        height,
    };
    Some(mesh)
}

/// A glyph source that can emit a whole line on a baseline.
///
/// The real font delegates to [`GuiBatch::draw_text_baseline`], which applies
/// the engine's atlas cell geometry and kerning. Test fixtures implement it
/// over a synthetic [`BaselineGlyphs`] so the mapping runs without a GPU.
trait PaintFont: BaselineGlyphs {
    fn draw_baseline(
        &self,
        gui: &mut GuiBatch,
        text: &str,
        position: [f32; 2],
        size: f32,
        color: [f32; 4],
    );
}

impl PaintFont for Font {
    fn draw_baseline(
        &self,
        gui: &mut GuiBatch,
        text: &str,
        position: [f32; 2],
        size: f32,
        color: [f32; 4],
    ) {
        gui.draw_text_baseline(self, text, position, size, color);
    }
}

/// One line of glyphs to paint, with optional per-span colours.
struct LineRun<'a> {
    text: &'a str,
    origin: [f32; 2],
    size: f32,
    tracking: f32,
    base: Color,
    spans: &'a [TextSpanStyle],
    line_start: usize,
    opacity: f32,
}

/// Emits one line of glyphs with per-character colour and tracking.
///
/// This is the path used whenever a run carries spans or letter tracking. The
/// plain path (no spans, no tracking) calls
/// [`GuiBatch::draw_text_baseline`](ferrous_ui_render::GuiBatch::draw_text_baseline)
/// so the engine applies its own kerning.
fn paint_line<F: BaselineGlyphs + ?Sized>(gui: &mut GuiBatch, font: &F, line: &LineRun<'_>) {
    let [left, bottom, right, top] = font.cell_bounds();
    let cell = [(right - left) * line.size, (top - bottom) * line.size];
    let mut pen = line.origin[0];
    let mut previous: Option<char> = None;
    for (offset, ch) in line.text.char_indices() {
        if let Some(left_char) = previous {
            pen += font.kerning(left_char, ch) * line.size;
        }
        previous = Some(ch);
        let advance = font.advance(ch).unwrap_or(0.5);
        if !ch.is_whitespace() {
            if let Some(uv) = font.uv(ch) {
                let color = span_color(
                    line.spans,
                    line.line_start + offset,
                    line.base,
                    line.opacity,
                );
                gui.push_text(TextQuad {
                    pos: [pen + left * line.size, line.origin[1] - top * line.size],
                    size: cell,
                    uv0: [uv[0], uv[1]],
                    uv1: [uv[2], uv[3]],
                    color,
                    z_order: 0.0,
                    node_id: 0,
                });
            }
        }
        pen += advance * line.size + line.tracking * line.size;
    }
    gui.update_last_segment();
}

fn span_color(spans: &[TextSpanStyle], byte: usize, base: Color, opacity: f32) -> [f32; 4] {
    let color = spans
        .iter()
        .find(|span| byte >= span.start && byte < span.end)
        .map_or(base, |span| span.color);
    let mut linear = color.to_linear();
    linear[3] *= opacity;
    linear
}

#[cfg(test)]
mod tests {
    use super::{GuiMakerRenderer, PaintFont, RectSpec, TextureMap};
    use ferrous_ui_render::text_utils::BaselineGlyphs;
    use ferrous_ui_render::{
        GuiBatch, TextQuad, BACKDROP_BLUR_BIT, GRADIENT_BIT, GRADIENT_RADIAL_BIT, GRADIENT_V_BIT,
        SHADOW_BIT,
    };
    use glam::Vec2;
    use guimaker::display_list::{
        DisplayList, DrawCommand, Gradient, GradientStop, ImageFit, TextRun, TextSpanStyle,
    };
    use guimaker::geom::{Rect, Size};
    use guimaker::text::{Line, TextStyle};
    use guimaker::theme::{Color, FontFamily, FontWeight};
    use guimaker::view::NodeId;
    use std::sync::Arc;

    fn close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 1e-6,
            "{actual} is not within 1e-6 of {expected}"
        );
    }

    fn close_slice(actual: &[f32], expected: &[f32]) {
        assert_eq!(actual.len(), expected.len(), "length mismatch");
        for (actual, expected) in actual.iter().zip(expected) {
            close(*actual, *expected);
        }
    }

    struct FakeGlyphs;

    impl BaselineGlyphs for FakeGlyphs {
        fn advance(&self, _c: char) -> Option<f32> {
            Some(0.5)
        }

        fn kerning(&self, _left: char, _right: char) -> f32 {
            0.0
        }

        fn cell_bounds(&self) -> [f32; 4] {
            [0.0, 0.0, 1.0, 1.0]
        }

        fn uv(&self, _c: char) -> Option<[f32; 4]> {
            Some([0.0, 0.0, 0.1, 0.1])
        }
    }

    impl PaintFont for FakeGlyphs {
        fn draw_baseline(
            &self,
            gui: &mut GuiBatch,
            text: &str,
            position: [f32; 2],
            size: f32,
            color: [f32; 4],
        ) {
            let [left, bottom, right, top] = self.cell_bounds();
            let cell = [(right - left) * size, (top - bottom) * size];
            let mut pen = position[0];
            for ch in text.chars() {
                if !ch.is_whitespace() {
                    if let Some(uv) = self.uv(ch) {
                        gui.push_text(TextQuad {
                            pos: [pen + left * size, position[1] - top * size],
                            size: cell,
                            uv0: [uv[0], uv[1]],
                            uv1: [uv[2], uv[3]],
                            color,
                            z_order: 0.0,
                            node_id: 0,
                        });
                    }
                }
                pen += self.advance(ch).unwrap_or(0.5) * size;
            }
            gui.update_last_segment();
        }
    }

    fn display(commands: Vec<DrawCommand>) -> DisplayList {
        let mut list = DisplayList::new();
        list.commands = commands;
        list
    }

    fn encode_to_batch(list: &DisplayList, scale: f32) -> GuiBatch {
        let mut renderer = GuiMakerRenderer::new();
        let mut batch = GuiBatch::new();
        renderer.encode_with(
            list,
            Size::new(800.0, 600.0),
            scale,
            &mut batch,
            Some(&FakeGlyphs),
        );
        batch
    }

    #[test]
    fn maps_shapes_clips_and_gradients() {
        let list = display(vec![
            DrawCommand::Rect {
                node: NodeId(1),
                rect: Rect::new(10.0, 20.0, 30.0, 40.0),
                color: Color::WHITE,
                radii: [1.0, 2.0, 3.0, 4.0],
            },
            DrawCommand::Border {
                node: NodeId(2),
                rect: Rect::new(10.0, 20.0, 30.0, 40.0),
                color: Color::BLACK,
                width: 1.5,
                sides: [1.5; 4],
                radii: [2.0; 4],
            },
            DrawCommand::Shadow {
                node: NodeId(3),
                rect: Rect::new(10.0, 20.0, 30.0, 40.0),
                color: Color::BLACK,
                offset: Vec2::new(0.0, 4.0),
                blur: 8.0,
                spread: 0.0,
                radii: [4.0; 4],
            },
            DrawCommand::Gradient {
                node: NodeId(4),
                rect: Rect::new(10.0, 20.0, 30.0, 40.0),
                gradient: Gradient::Linear {
                    angle: 0.0,
                    stops: vec![
                        GradientStop {
                            offset: 0.0,
                            color: Color::WHITE,
                        },
                        GradientStop {
                            offset: 1.0,
                            color: Color::BLACK,
                        },
                    ],
                },
                radii: [0.0; 4],
            },
            DrawCommand::ClipPush {
                node: NodeId(5),
                rect: Rect::new(5.0, 6.0, 70.0, 80.0),
                radii: [0.0; 4],
            },
            DrawCommand::ClipPop { node: NodeId(5) },
        ]);
        let batch = encode_to_batch(&list, 2.0);
        assert_eq!(batch.quads.len(), 4);
        close_slice(&batch.quads[0].pos, &[20.0, 40.0]);
        close_slice(&batch.quads[0].size, &[60.0, 80.0]);
        close_slice(&batch.quads[0].radii, &[2.0, 4.0, 6.0, 8.0]);
        assert_eq!(batch.quads[0].node_id, 1);
        close(batch.quads[1].uv0[0], 3.0);
        assert_eq!(batch.quads[2].flags & SHADOW_BIT, SHADOW_BIT);
        assert_eq!(batch.quads[3].flags & GRADIENT_BIT, GRADIENT_BIT);
        assert_eq!(batch.quads[3].flags & GRADIENT_V_BIT, 0);
        assert_eq!(batch.current_scissor, None);
        assert!(batch.scissor_stack.is_empty());
        assert!(batch
            .segments
            .iter()
            .any(|segment| segment.scissor.is_some()));
    }

    #[test]
    fn defers_a_backdrop_blur_to_the_owning_rect() {
        let list = display(vec![
            DrawCommand::BackdropBlur {
                node: NodeId(1),
                rect: Rect::new(10.0, 20.0, 300.0, 200.0),
                radius: 24.0,
                radii: [8.0; 4],
            },
            DrawCommand::Shadow {
                node: NodeId(1),
                rect: Rect::new(10.0, 20.0, 300.0, 200.0),
                color: Color::BLACK,
                offset: Vec2::new(0.0, 4.0),
                blur: 8.0,
                spread: 0.0,
                radii: [8.0; 4],
            },
            DrawCommand::Rect {
                node: NodeId(1),
                rect: Rect::new(10.0, 20.0, 300.0, 200.0),
                color: Color::WHITE,
                radii: [8.0; 4],
            },
        ]);
        let batch = encode_to_batch(&list, 2.0);
        assert_eq!(batch.quads.len(), 2, "ambient key shadow plus the blur");
        assert_eq!(batch.quads[0].flags & SHADOW_BIT, SHADOW_BIT);
        let blur = &batch.quads[1];
        assert_eq!(blur.flags, BACKDROP_BLUR_BIT);
        close(blur.uv0[0], 48.0);
        close_slice(&blur.pos, &[20.0, 40.0]);
        close_slice(&blur.size, &[600.0, 400.0]);
        close(blur.color[3], 0.80);
        close_slice(&blur.color[..3], &Color::WHITE.to_linear()[..3]);
        assert_eq!(blur.node_id, 1);
    }

    #[test]
    fn flushes_an_unmatched_backdrop_with_a_transparent_tint() {
        let list = display(vec![DrawCommand::BackdropBlur {
            node: NodeId(7),
            rect: Rect::new(0.0, 0.0, 10.0, 10.0),
            radius: 4.0,
            radii: [0.0; 4],
        }]);
        let batch = encode_to_batch(&list, 1.0);
        assert_eq!(batch.quads.len(), 1);
        assert_eq!(batch.quads[0].flags, BACKDROP_BLUR_BIT);
        close_slice(&batch.quads[0].color, &[0.0; 4]);
        assert_eq!(batch.quads[0].node_id, 7);
    }

    #[test]
    fn emits_one_shadow_quad_per_command() {
        let shadow = |y: f32| DrawCommand::Shadow {
            node: NodeId(1),
            rect: Rect::new(10.0, y, 30.0, 40.0),
            color: Color::BLACK,
            offset: Vec2::new(0.0, 4.0),
            blur: 8.0,
            spread: 0.0,
            radii: [6.0; 4],
        };
        let batch = encode_to_batch(&display(vec![shadow(10.0), shadow(70.0)]), 1.0);
        assert_eq!(batch.quads.len(), 2);
        assert!(batch
            .quads
            .iter()
            .all(|quad| quad.flags & SHADOW_BIT == SHADOW_BIT));
    }

    #[test]
    fn runs_a_radial_gradient() {
        let radial = display(vec![DrawCommand::Gradient {
            node: NodeId(1),
            rect: Rect::new(0.0, 0.0, 10.0, 10.0),
            gradient: Gradient::Radial {
                radius: 5.0,
                stops: vec![
                    GradientStop {
                        offset: 0.0,
                        color: Color::WHITE,
                    },
                    GradientStop {
                        offset: 1.0,
                        color: Color::BLACK,
                    },
                ],
            },
            radii: [0.0; 4],
        }]);
        let batch = encode_to_batch(&radial, 1.0);
        assert_eq!(
            batch.quads[0].flags & (GRADIENT_BIT | GRADIENT_RADIAL_BIT),
            GRADIENT_BIT | GRADIENT_RADIAL_BIT
        );
    }

    #[test]
    fn falls_back_to_a_rectangular_clip() {
        let list = display(vec![
            DrawCommand::ClipPush {
                node: NodeId(1),
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                radii: [3.0; 4],
            },
            DrawCommand::Rect {
                node: NodeId(2),
                rect: Rect::new(0.0, 0.0, 5.0, 5.0),
                color: Color::WHITE,
                radii: [0.0; 4],
            },
            DrawCommand::ClipPop { node: NodeId(1) },
        ]);
        let batch = encode_to_batch(&list, 2.0);
        assert!(batch.segments.iter().any(|segment| {
            segment
                .scissor
                .is_some_and(|scissor| (scissor.width - 20.0).abs() < 1e-6)
        }));
        assert_eq!(batch.current_scissor, None);
    }

    #[test]
    fn scales_text_glyphs_and_colours_spans() {
        let run = TextRun {
            text: Arc::from("ab"),
            lines: Arc::from([Line {
                start: 0,
                end: 2,
                width: 16.0,
                baseline: 14.0,
                glyph_start: 0,
                glyph_end: 2,
            }]),
            style: TextStyle {
                size: 10.0,
                weight: FontWeight::Regular.value(),
                line_height: 14.0,
                tracking: 0.05,
                family: FontFamily::Sans,
                tabular: false,
            },
            spans: Arc::from([
                TextSpanStyle {
                    start: 0,
                    end: 1,
                    color: Color::WHITE,
                    strong: false,
                    mono: false,
                    link: false,
                },
                TextSpanStyle {
                    start: 1,
                    end: 2,
                    color: Color::BLACK,
                    strong: false,
                    mono: false,
                    link: false,
                },
            ]),
        };
        let list = display(vec![DrawCommand::Text {
            node: NodeId(9),
            rect: Rect::new(0.0, 0.0, 100.0, 20.0),
            run,
        }]);
        let batch = encode_to_batch(&list, 2.0);
        assert_eq!(batch.text_quads.len(), 2);
        assert!(batch.text_quads.iter().all(|quad| quad.node_id == 9));
        close_slice(&batch.text_quads[0].size, &[20.0, 20.0]);
        close_slice(&batch.text_quads[0].color, &Color::WHITE.to_linear());
        close_slice(&batch.text_quads[1].color, &Color::BLACK.to_linear());
    }

    #[test]
    fn multiplies_alpha_inside_a_layer() {
        let list = display(vec![
            DrawCommand::Rect {
                node: NodeId(1),
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                color: Color::WHITE,
                radii: [0.0; 4],
            },
            DrawCommand::LayerPush {
                node: NodeId(2),
                opacity: 0.5,
            },
            DrawCommand::Rect {
                node: NodeId(3),
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                color: Color::WHITE,
                radii: [0.0; 4],
            },
            DrawCommand::LayerPop { node: NodeId(2) },
            DrawCommand::Rect {
                node: NodeId(4),
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                color: Color::WHITE,
                radii: [0.0; 4],
            },
        ]);
        let batch = encode_to_batch(&list, 1.0);
        assert_eq!(batch.quads.len(), 3);
        close(batch.quads[1].color[3], 0.5);
        close(batch.quads[2].color[3], 1.0);
        assert!(batch.segments.iter().any(|segment| segment.layer_start));
    }

    #[test]
    fn fits_images_without_a_gpu() {
        let rect = RectSpec {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        };
        let (dest, uv0, uv1) = super::image_geometry(rect, [200.0, 100.0], ImageFit::Contain);
        close(dest.width, 100.0);
        close(dest.height, 50.0);
        close(dest.y, 25.0);
        close_slice(&uv0, &[0.0, 0.0]);
        close_slice(&uv1, &[1.0, 1.0]);

        let (dest, uv0, uv1) = super::image_geometry(rect, [200.0, 100.0], ImageFit::Cover);
        close(dest.width, 100.0);
        close(dest.height, 100.0);
        close(uv0[0], 0.25);
        close(uv1[0], 0.75);

        let (dest, uv0, uv1) = super::image_geometry(rect, [200.0, 100.0], ImageFit::Fill);
        close(dest.width, 100.0);
        close(dest.height, 100.0);
        close_slice(&uv0, &[0.0, 0.0]);
        close_slice(&uv1, &[1.0, 1.0]);
    }

    #[test]
    fn skips_images_without_a_registered_texture() {
        let list = display(vec![DrawCommand::Image {
            node: NodeId(1),
            rect: Rect::new(0.0, 0.0, 10.0, 10.0),
            source: 42,
            fit: ImageFit::Cover,
            radii: [0.0; 4],
        }]);
        let mut renderer = GuiMakerRenderer::with_textures(TextureMap::new());
        let mut batch = GuiBatch::new();
        renderer.encode_with(
            &list,
            Size::new(800.0, 600.0),
            1.0,
            &mut batch,
            Some(&FakeGlyphs),
        );
        assert!(batch.quads.is_empty());
    }

    #[test]
    fn renders_real_icon_paths_and_falls_back_for_invalid_icons() {
        let list = display(vec![
            DrawCommand::Icon {
                node: NodeId(1),
                rect: Rect::new(0.0, 0.0, 24.0, 24.0),
                path: Arc::from("<rect width=\"100\" height=\"100\"/>"),
                color: Color::WHITE,
            },
            DrawCommand::Icon {
                node: NodeId(2),
                rect: Rect::new(0.0, 0.0, 24.0, 24.0),
                path: Arc::from("<path d=\"M0 0 L10 10\"/>"),
                color: Color::WHITE,
            },
            DrawCommand::Icon {
                node: NodeId(3),
                rect: Rect::new(0.0, 0.0, 24.0, 24.0),
                path: Arc::from("<svg></svg>"),
                color: Color::WHITE,
            },
        ]);
        let batch = encode_to_batch(&list, 1.0);
        // The shape and the path are real vector icons; the empty SVG falls
        // back to a plain rectangle instead of a fake placeholder mesh.
        assert_eq!(batch.svg_commands.len(), 2);
        assert_eq!(batch.quads.len(), 1);
        close(batch.svg_commands[0].mesh.bounding_box.width, 24.0);
    }

    #[test]
    fn ignores_a_degenerate_viewport_or_scale() {
        let list = display(vec![DrawCommand::Rect {
            node: NodeId(1),
            rect: Rect::new(0.0, 0.0, 10.0, 10.0),
            color: Color::WHITE,
            radii: [0.0; 4],
        }]);
        let mut renderer = GuiMakerRenderer::new();
        let mut batch = GuiBatch::new();
        renderer.encode_with(&list, Size::ZERO, 1.0, &mut batch, Some(&FakeGlyphs));
        renderer.encode_with(
            &list,
            Size::new(10.0, 10.0),
            0.0,
            &mut batch,
            Some(&FakeGlyphs),
        );
        assert!(batch.quads.is_empty());
    }
}
