//! GPU batching system for UI rendering.
//!
//! This module contains GuiBatch, which aggregates UI primitives into
//! rendering segments optimized for GPU execution.

use crate::gpu_types::{DrawSegment, GuiQuad, SvgCommand, TextQuad};
use crate::to_batches::ToBatches;
use crate::TEXTURED_BIT;
use ferrous_ui_core::render_collector::NodeBatch;
use ferrous_ui_core::{Background, Rect, RenderCommand};
use std::collections::HashMap;

/// Grouping of UI primitives organized by rendering segments.
#[derive(Clone)]
pub struct GuiBatch {
    pub quads: Vec<GuiQuad>,
    pub text_quads: Vec<TextQuad>,
    pub icon_quads: Vec<TextQuad>,
    pub svg_commands: Vec<SvgCommand>,
    pub segments: Vec<DrawSegment>,
    pub current_scissor: Option<ferrous_ui_core::Rect>,
    pub scissor_stack: Vec<ferrous_ui_core::Rect>,
    #[cfg(feature = "assets")]
    pub textures: Vec<std::sync::Arc<ferrous_assets::Texture2d>>,
    pub damage_union: Option<ferrous_ui_core::Rect>,
    /// Per-node batch products retained between frames (see [`Self::push_node`]).
    retained: HashMap<u64, NodeRetained>,
    /// Identity of the tree the retained entries belong to; a new tree drops them.
    retained_epoch: u64,
    /// Monotonic frame counter used to sweep nodes that were not visited.
    retained_frame: u64,
    /// Number of nodes whose batch products were rebuilt this frame (tests only).
    #[cfg(test)]
    retained_rebuilds: u64,
}

/// Everything the retained batch products of a node depend on.
///
/// A node's batch slices are reused only when the whole key matches. It covers
/// every input `ToBatches` reads:
///
/// - the node id: the retained map is keyed by the full slot-map handle
///   (`index + generation`), which also derives the truncated GPU `node_id`.
/// - `revision`: bumped whenever the collector ran `draw` for the node, which
///   is the only code path that rewrites `Node::cached_cmds`. A clean node
///   therefore keeps its revision, and a redraw always invalidates. This is the
///   cheap content revision: the commands themselves are not hashed.
/// - `z_bits`: the paint depth written into every instance.
/// - `clip`: the active scissor stack when the node starts (the clip rects
///   convert into absolute segment scissors).
/// - `font`: the address of the font atlas, whose glyph/icon UVs are baked
///   into the produced text and icon quads (`0` when no font is in use).
/// - `textures`: the identity and order of the texture slots already
///   registered, because `Image` commands bake their resolved slot index.
/// - `overflow`: the node's own clip rect, which wraps its own commands.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct RetainedKey {
    revision: u64,
    z_bits: u32,
    clip: u64,
    font: u64,
    textures: u64,
    overflow: u64,
}

/// One batch operation retained for a node.
///
/// The operations keep the command order so scissor changes land on the same
/// segment boundaries as a fresh conversion. Clip rects are stored raw (not
/// pre-intersected): replay feeds them back through `push_clip`, which is
/// correct because [`RetainedKey::clip`] guarantees the same entering scissor.
#[derive(Clone, Debug)]
enum RetainedOp {
    /// Append `n` gui quads from the node's retained quad slice.
    Quads(u32),
    /// Append `n` text quads from the node's retained text slice.
    Texts(u32),
    /// Append `n` icon quads from the node's retained icon slice.
    Icons(u32),
    /// Append `n` SVG commands from the node's retained SVG slice.
    Svgs(u32),
    /// Open a widget clip by this rect.
    PushClip(Rect),
    /// Close a widget clip.
    PopClip,
    /// Re-run the segment bookkeeping for a command that emitted no primitives
    /// but still called `update_last_segment` (an empty text run with a font).
    UpdateSegment,
}

/// Batch products retained for one node.
#[derive(Clone, Default)]
struct NodeRetained {
    key: RetainedKey,
    last_seen: u64,
    quads: Vec<GuiQuad>,
    texts: Vec<TextQuad>,
    icons: Vec<TextQuad>,
    svgs: Vec<SvgCommand>,
    ops: Vec<RetainedOp>,
    #[cfg(feature = "assets")]
    textures: Vec<std::sync::Arc<ferrous_assets::Texture2d>>,
}

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

#[inline]
fn hash_u64(mut hash: u64, value: u64) -> u64 {
    hash ^= value;
    hash.wrapping_mul(FNV_PRIME)
}

#[inline]
fn hash_f32(hash: u64, value: f32) -> u64 {
    hash_u64(hash, value.to_bits() as u64)
}

#[inline]
fn hash_rect(hash: u64, rect: Rect) -> u64 {
    let hash = hash_f32(hash, rect.x);
    let hash = hash_f32(hash, rect.y);
    let hash = hash_f32(hash, rect.width);
    hash_f32(hash, rect.height)
}

/// Identity of an optional overflow clip.
fn overflow_identity(overflow: Option<Rect>) -> u64 {
    match overflow {
        Some(rect) => hash_rect(hash_u64(FNV_OFFSET, 1), rect),
        None => hash_u64(FNV_OFFSET, 0),
    }
}

/// Identity of the font atlas, or `0` when no font is bound.
///
/// The atlas content key is used instead of the `Font` pointer: a `Font` can
/// be rebuilt with a different atlas at the same address, and glyph UVs come
/// from the atlas.  The icon atlas contributes its cell count, so swapping the
/// icon set also invalidates the retained slices.
#[cfg(feature = "text")]
fn font_identity(font: Option<&ferrous_assets::Font>) -> u64 {
    font.map_or(0, |font| {
        let key = font.atlas.key.raw();
        let mut hash = hash_u64(FNV_OFFSET, key as u64);
        hash = hash_u64(hash, (key >> 64) as u64);
        let icons = font
            .icons
            .as_ref()
            .map_or(0, |icons| icons.icons.len() as u64 + 1);
        hash_u64(hash, icons)
    })
}

/// Whether a command's batch output can change while its node stays clean.
///
/// [`Background::Procedural`] and [`Background::Texture`] hold closures that
/// are evaluated per batch, so their quads must be rebuilt every frame even
/// when the widget tree reports the node as clean.
fn is_dynamic_command(command: &RenderCommand) -> bool {
    matches!(
        command,
        RenderCommand::GradientQuad {
            background: Background::Procedural(_) | Background::Texture { .. },
            ..
        }
    )
}

impl GuiBatch {
    /// Snapshot of the segment count and the last segment's end offsets.
    ///
    /// Used to detect a command that ran the segment bookkeeping without
    /// emitting any primitive, so replay can reproduce it.
    fn segment_ends(&self) -> (usize, Option<(u32, u32, u32, u32)>) {
        let last = self.segments.last().map(|segment| {
            (
                segment.quad_range.end,
                segment.text_range.end,
                segment.icon_range.end,
                segment.svg_range.end,
            )
        });
        (self.segments.len(), last)
    }

    /// Identity of the active scissor stack.
    fn clip_identity(&self) -> u64 {
        let mut hash = FNV_OFFSET;
        match self.current_scissor {
            Some(rect) => hash = hash_rect(hash_u64(hash, 1), rect),
            None => hash = hash_u64(hash, 0),
        }
        for rect in &self.scissor_stack {
            hash = hash_rect(hash_u64(hash, 2), *rect);
        }
        hash
    }

    /// Identity and order of the registered texture slots.
    #[cfg(feature = "assets")]
    fn texture_identity(&self) -> u64 {
        let mut hash = FNV_OFFSET;
        for texture in &self.textures {
            hash = hash_u64(hash, std::sync::Arc::as_ptr(texture) as usize as u64);
        }
        hash
    }

    /// Identity and order of the registered texture slots (no-assets build).
    #[cfg(not(feature = "assets"))]
    fn texture_identity(&self) -> u64 {
        0
    }

    /// Starts a retained frame for `epoch`.
    ///
    /// The retained store survives [`Self::clear`] (only the emitted primitives
    /// are cleared), so a recycled batch keeps last frame's products. Passing a
    /// different `epoch` means a different tree owns the batch and drops the
    /// store instead of matching stale node handles.
    pub fn begin_retained_frame(&mut self, epoch: u64) {
        if epoch != self.retained_epoch {
            self.retained.clear();
            self.retained_epoch = epoch;
        }
        self.retained_frame = self.retained_frame.wrapping_add(1);
        #[cfg(test)]
        {
            self.retained_rebuilds = 0;
        }
    }

    /// Drops the retained products of nodes that were not visited this frame.
    ///
    /// Runs after the tree walk, so removed nodes and fully clipped subtrees
    /// cannot leave ranges behind: the store is bounded by the number of nodes
    /// visited in the current frame.
    pub fn end_retained_frame(&mut self) {
        let frame = self.retained_frame;
        self.retained.retain(|_, entry| entry.last_seen == frame);
    }

    /// Appends one node's cached commands, reusing last frame's batch slices
    /// when the node is clean and its retained key still matches.
    ///
    /// Must be called between [`Self::begin_retained_frame`] and
    /// [`Self::end_retained_frame`], once per emitted [`NodeBatch`], in paint
    /// order. The node's overflow clip is opened here; the matching
    /// `NodeVisit::PopClip` is expected after its children.
    #[cfg(feature = "text")]
    pub fn push_node(&mut self, node: NodeBatch<'_>, font: Option<&ferrous_assets::Font>) {
        debug_assert!(
            self.retained_frame > 0,
            "push_node must run between begin_retained_frame and end_retained_frame"
        );
        self.push_node_impl(&node, font_identity(font), |batch, cmd, z, node_id| {
            cmd.to_batches(batch, font, z, node_id)
        });
    }

    /// Appends one node's cached commands (text-disabled build).
    #[cfg(not(feature = "text"))]
    pub fn push_node(&mut self, node: NodeBatch<'_>) {
        debug_assert!(
            self.retained_frame > 0,
            "push_node must run between begin_retained_frame and end_retained_frame"
        );
        self.push_node_impl(&node, 0, |batch, cmd, z, node_id| {
            cmd.to_batches(batch, z, node_id)
        });
    }

    /// Core of [`Self::push_node`], parameterised over the command converter.
    fn push_node_impl<C>(&mut self, node: &NodeBatch<'_>, font: u64, mut convert: C)
    where
        C: FnMut(&mut GuiBatch, &RenderCommand, f32, u32),
    {
        let frame = self.retained_frame;
        let previous = self.retained.remove(&node.handle);
        let had_previous = previous.is_some();
        let revision = match (&previous, node.redrawn) {
            (Some(entry), false) => entry.key.revision,
            (Some(entry), true) => entry.key.revision.wrapping_add(1),
            (None, _) => 0,
        };
        let key = RetainedKey {
            revision,
            z_bits: node.z.to_bits(),
            clip: self.clip_identity(),
            font,
            textures: self.texture_identity(),
            overflow: overflow_identity(node.overflow),
        };

        // The node's own overflow clip is cheap to rebuild and is not part of
        // the retained ops: open it for both paths, and let the collector's
        // closing `NodeVisit::PopClip` restore the parent scissor after the
        // children, exactly as the pre-retention collector did.
        if let Some(rect) = node.overflow {
            self.push_clip(rect);
        }

        let mut entry = previous.unwrap_or_default();
        // A procedural or texture background is evaluated at batch time (the
        // closure may animate or read a buffer that changed), so its node is
        // never replayed from the retained slices.
        let dynamic = node.cmds.iter().any(is_dynamic_command);
        if had_previous && !node.redrawn && !dynamic && entry.key == key {
            self.replay_entry(&entry);
            entry.last_seen = frame;
            self.retained.insert(node.handle, entry);
            return;
        }

        #[cfg(test)]
        {
            self.retained_rebuilds += 1;
        }
        entry.ops.clear();
        entry.quads.clear();
        entry.texts.clear();
        entry.icons.clear();
        entry.svgs.clear();
        #[cfg(feature = "assets")]
        entry.textures.clear();

        let quad_start = self.quads.len();
        let text_start = self.text_quads.len();
        let icon_start = self.icon_quads.len();
        let svg_start = self.svg_commands.len();
        #[cfg(feature = "assets")]
        let texture_start = self.textures.len();

        for cmd in node.cmds {
            let quads = self.quads.len();
            let texts = self.text_quads.len();
            let icons = self.icon_quads.len();
            let svgs = self.svg_commands.len();
            let segments = self.segment_ends();
            convert(self, cmd, node.z, node.node_id);
            let mut emitted = false;
            if self.quads.len() != quads {
                entry
                    .ops
                    .push(RetainedOp::Quads((self.quads.len() - quads) as u32));
                emitted = true;
            }
            if self.text_quads.len() != texts {
                entry
                    .ops
                    .push(RetainedOp::Texts((self.text_quads.len() - texts) as u32));
                emitted = true;
            }
            if self.icon_quads.len() != icons {
                entry
                    .ops
                    .push(RetainedOp::Icons((self.icon_quads.len() - icons) as u32));
                emitted = true;
            }
            if self.svg_commands.len() != svgs {
                entry
                    .ops
                    .push(RetainedOp::Svgs((self.svg_commands.len() - svgs) as u32));
                emitted = true;
            }
            // Data commands never touch the scissor; only the two clip variants
            // do, and their effect must be replayed in the same order.
            match cmd {
                RenderCommand::PushClip { rect } => {
                    entry.ops.push(RetainedOp::PushClip(*rect));
                    emitted = true;
                }
                RenderCommand::PopClip => {
                    entry.ops.push(RetainedOp::PopClip);
                    emitted = true;
                }
                _ => {}
            }
            if !emitted && self.segment_ends() != segments {
                // The command changed the segment layout without emitting data
                // (e.g. an empty text run still calls `update_last_segment`).
                entry.ops.push(RetainedOp::UpdateSegment);
            }
        }
        entry.quads.extend_from_slice(&self.quads[quad_start..]);
        entry
            .texts
            .extend_from_slice(&self.text_quads[text_start..]);
        entry
            .icons
            .extend_from_slice(&self.icon_quads[icon_start..]);
        entry
            .svgs
            .extend_from_slice(&self.svg_commands[svg_start..]);
        #[cfg(feature = "assets")]
        entry
            .textures
            .extend_from_slice(&self.textures[texture_start..]);

        entry.key = key;
        entry.last_seen = frame;
        self.retained.insert(node.handle, entry);
    }

    /// Replays a node's retained operations into the live batch.
    fn replay_entry(&mut self, entry: &NodeRetained) {
        #[cfg(feature = "assets")]
        for texture in &entry.textures {
            self.reserve_texture_slot(texture.clone());
        }
        let (mut quads, mut texts, mut icons, mut svgs) = (0usize, 0usize, 0usize, 0usize);
        for op in &entry.ops {
            match op {
                RetainedOp::Quads(n) => {
                    let end = quads + *n as usize;
                    self.append_quads(&entry.quads[quads..end]);
                    quads = end;
                }
                RetainedOp::Texts(n) => {
                    let end = texts + *n as usize;
                    self.append_texts(&entry.texts[texts..end]);
                    texts = end;
                }
                RetainedOp::Icons(n) => {
                    let end = icons + *n as usize;
                    self.append_icons(&entry.icons[icons..end]);
                    icons = end;
                }
                RetainedOp::Svgs(n) => {
                    let end = svgs + *n as usize;
                    self.append_svgs(&entry.svgs[svgs..end]);
                    svgs = end;
                }
                RetainedOp::PushClip(rect) => self.push_clip(*rect),
                RetainedOp::PopClip => self.pop_clip(),
                RetainedOp::UpdateSegment => self.update_last_segment(),
            }
        }
    }

    /// Appends retained gui quads, reproducing the damage of `push_quad`.
    fn append_quads(&mut self, quads: &[GuiQuad]) {
        if let Some(union) = quad_damage(quads) {
            self.add_damage_rect(union);
        }
        self.ensure_segment();
        self.quads.extend_from_slice(quads);
        self.update_last_segment();
    }

    /// Appends retained text glyphs, reproducing the damage of `push_text`.
    fn append_texts(&mut self, texts: &[TextQuad]) {
        if let Some(union) = glyph_damage(texts) {
            self.add_damage_rect(union);
        }
        self.ensure_segment();
        self.text_quads.extend_from_slice(texts);
        self.update_last_segment();
    }

    /// Appends retained icon glyphs, reproducing the damage of `push_icon`.
    fn append_icons(&mut self, icons: &[TextQuad]) {
        if let Some(union) = glyph_damage(icons) {
            self.add_damage_rect(union);
        }
        self.ensure_segment();
        self.icon_quads.extend_from_slice(icons);
        self.update_last_segment();
    }

    /// Appends retained SVG commands, reproducing the damage of `push_svg`.
    fn append_svgs(&mut self, commands: &[SvgCommand]) {
        if let Some(union) = svg_damage(commands) {
            self.add_damage_rect(union);
        }
        self.ensure_segment();
        self.svg_commands.extend_from_slice(commands);
        self.update_last_segment();
    }

    /// Number of nodes whose batch products were rebuilt this frame.
    #[cfg(test)]
    pub(crate) fn retained_rebuild_count(&self) -> u64 {
        self.retained_rebuilds
    }

    /// Drives [`Self::push_node`] with an explicit font identity (tests only).
    ///
    /// Conversion runs with no font so the test does not need a GPU atlas; the
    /// point is to observe that a changed identity invalidates the retained key.
    #[cfg(all(test, feature = "text"))]
    pub(crate) fn push_node_with_font_identity(&mut self, node: NodeBatch<'_>, font: u64) {
        self.push_node_impl(&node, font, |batch, cmd, z, node_id| {
            cmd.to_batches(batch, None, z, node_id)
        });
    }
}

/// Screenspace rect covered by a positioned `size` box.
#[inline]
fn rect_of(pos: [f32; 2], size: [f32; 2]) -> Rect {
    Rect {
        x: pos[0],
        y: pos[1],
        width: size[0],
        height: size[1],
    }
}

/// Union of the non-empty `rects`, or `None` when none has area.
///
/// Empty rects are filtered out because [`GuiBatch::add_damage_rect`] ignores
/// them; keeping them would let a degenerate quad widen the replayed damage.
#[inline]
fn union_rects(rects: impl Iterator<Item = Rect>) -> Option<Rect> {
    rects
        .filter(|rect| rect.width > 0.0 && rect.height > 0.0)
        .reduce(|acc, rect| acc.union(rect))
}

/// Appends each source texture only when the destination does not already hold
/// the same `Arc`, and returns the source-slot -> destination-slot mapping.
///
/// Identity is `Arc::ptr_eq`, so two independently loaded textures that happen
/// to look the same stay separate slots while the exact same asset is reused.
#[cfg(any(feature = "assets", test))]
fn merge_texture_slots<T>(
    destination: &mut Vec<std::sync::Arc<T>>,
    source: impl IntoIterator<Item = std::sync::Arc<T>>,
) -> Vec<u32> {
    source
        .into_iter()
        .map(|texture| {
            if let Some(position) = destination
                .iter()
                .position(|candidate| std::sync::Arc::ptr_eq(candidate, &texture))
            {
                return position as u32;
            }
            destination.push(texture);
            (destination.len() - 1) as u32
        })
        .collect()
}

/// Rewrites textured quads from source-batch slots to `remap`'s slots.
///
/// Quads without [`TEXTURED_BIT`] are solids (or gradient/border/line
/// primitives) and keep `tex_index = 0`.  An out-of-range index falls back to
/// `0` instead of panicking, matching the "solids stay 0" contract.
#[cfg(any(feature = "assets", test))]
fn remap_texture_indices(quads: &mut [GuiQuad], remap: &[u32]) {
    if remap.is_empty() {
        return;
    }
    for quad in quads {
        if quad.flags & TEXTURED_BIT != 0 {
            quad.tex_index = remap.get(quad.tex_index as usize).copied().unwrap_or(0);
        }
    }
}

/// Union of the screenspace rects of `quads`, or `None` when empty.
fn quad_damage(quads: &[GuiQuad]) -> Option<Rect> {
    union_rects(quads.iter().map(|quad| rect_of(quad.pos, quad.size)))
}

/// Union of the screenspace rects of `glyphs`, or `None` when empty.
fn glyph_damage(glyphs: &[TextQuad]) -> Option<Rect> {
    union_rects(glyphs.iter().map(|glyph| rect_of(glyph.pos, glyph.size)))
}

/// Union of the screenspace bounds of `commands`, or `None` when empty.
fn svg_damage(commands: &[SvgCommand]) -> Option<Rect> {
    union_rects(commands.iter().filter_map(svg_bounds))
}

/// Bounds of an SVG command after applying its translated origin.
fn svg_bounds(cmd: &SvgCommand) -> Option<Rect> {
    if cmd.mesh.vertices.is_empty() {
        return None;
    }
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for v in &cmd.mesh.vertices {
        let x = v.position[0] + cmd.pos[0];
        let y = v.position[1] + cmd.pos[1];
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    Some(Rect {
        x: min_x,
        y: min_y,
        width: (max_x - min_x).max(0.0),
        height: (max_y - min_y).max(0.0),
    })
}

impl Default for GuiBatch {
    fn default() -> Self {
        Self::new()
    }
}

impl GuiBatch {
    fn add_damage_rect(&mut self, mut rect: ferrous_ui_core::Rect) {
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }

        if let Some(scissor) = self.current_scissor {
            rect = rect.intersect(&scissor);
            if rect.width <= 0.0 || rect.height <= 0.0 {
                return;
            }
        }

        self.damage_union = match self.damage_union {
            Some(current) => Some(current.union(rect)),
            None => Some(rect),
        };
    }

    pub fn new() -> Self {
        Self {
            quads: Vec::new(),
            text_quads: Vec::new(),
            icon_quads: Vec::new(),
            svg_commands: Vec::new(),
            segments: Vec::new(),
            current_scissor: None,
            scissor_stack: Vec::new(),
            #[cfg(feature = "assets")]
            textures: Vec::new(),
            damage_union: None,
            retained: HashMap::new(),
            retained_epoch: 0,
            retained_frame: 0,
            #[cfg(test)]
            retained_rebuilds: 0,
        }
    }

    pub fn clear(&mut self) {
        self.quads.clear();
        self.text_quads.clear();
        self.icon_quads.clear();
        self.svg_commands.clear();
        self.segments.clear();
        self.current_scissor = None;
        self.scissor_stack.clear();
        self.damage_union = None;
        #[cfg(feature = "assets")]
        {
            self.textures.clear();
        }
    }

    pub fn push_quad(&mut self, quad: GuiQuad) {
        self.add_damage_rect(ferrous_ui_core::Rect {
            x: quad.pos[0],
            y: quad.pos[1],
            width: quad.size[0],
            height: quad.size[1],
        });
        self.ensure_segment();
        self.quads.push(quad);
        self.update_last_segment();
    }

    pub fn push_text(&mut self, quad: TextQuad) {
        self.add_damage_rect(ferrous_ui_core::Rect {
            x: quad.pos[0],
            y: quad.pos[1],
            width: quad.size[0],
            height: quad.size[1],
        });
        self.ensure_segment();
        self.text_quads.push(quad);
        self.update_last_segment();
    }

    pub fn push_icon(&mut self, quad: TextQuad) {
        self.add_damage_rect(ferrous_ui_core::Rect {
            x: quad.pos[0],
            y: quad.pos[1],
            width: quad.size[0],
            height: quad.size[1],
        });
        self.ensure_segment();
        self.icon_quads.push(quad);
        self.update_last_segment();
    }

    pub fn push_svg(&mut self, cmd: SvgCommand) {
        if let Some(bounds) = svg_bounds(&cmd) {
            self.add_damage_rect(bounds);
        }
        self.ensure_segment();
        self.svg_commands.push(cmd);
        self.update_last_segment();
    }

    /// Appends another batch, remapping its texture slots into this one.
    ///
    /// Quads keep painting order, segments are rebased and the damage union is
    /// merged.  Textured quads are rewritten from the source batch's slot
    /// numbering to the destination's: each texture is appended only when it is
    /// not already present (`Arc::ptr_eq`), so extending the same cached panel
    /// twice does not grow `textures`.  Solid quads keep `tex_index = 0`.
    pub fn extend(&mut self, mut other: GuiBatch) {
        let q_offset = self.quads.len() as u32;
        let t_offset = self.text_quads.len() as u32;
        let i_offset = self.icon_quads.len() as u32;
        let s_offset = self.svg_commands.len() as u32;

        #[cfg(feature = "assets")]
        {
            // Reuse the destination's slots and append only new arcs.  The
            // mapping is indexed by the *source* slot, so every quad that
            // sampled source slot N now samples the destination slot for the
            // same texture.
            let remap = merge_texture_slots(&mut self.textures, other.textures.drain(..));
            remap_texture_indices(&mut other.quads, &remap);
        }

        for seg in &mut other.segments {
            seg.quad_range.start += q_offset;
            seg.quad_range.end += q_offset;
            seg.text_range.start += t_offset;
            seg.text_range.end += t_offset;
            seg.icon_range.start += i_offset;
            seg.icon_range.end += i_offset;
            seg.svg_range.start += s_offset;
            seg.svg_range.end += s_offset;
        }

        self.quads.extend(other.quads);
        self.text_quads.extend(other.text_quads);
        self.icon_quads.extend(other.icon_quads);
        self.svg_commands.extend(other.svg_commands);
        self.segments.extend(other.segments);

        if let Some(other_damage) = other.damage_union {
            self.damage_union = match self.damage_union {
                Some(current) => Some(current.union(other_damage)),
                None => Some(other_damage),
            };
        }
    }

    /// Appends a borrowed batch, remapping its texture slots into this one.
    ///
    /// The retained-panel path in an immediate-mode client composes cached
    /// panel batches every frame; cloning the cached batch first would copy the
    /// primitives twice and allocate.  This appends the same data as
    /// [`Self::extend`] without taking ownership: only new texture arcs are
    /// cloned (an `Arc` bump), the primitives are copied once.
    pub fn extend_from(&mut self, other: &GuiBatch) {
        let q_offset = self.quads.len() as u32;
        let t_offset = self.text_quads.len() as u32;
        let i_offset = self.icon_quads.len() as u32;
        let s_offset = self.svg_commands.len() as u32;

        #[cfg(feature = "assets")]
        let remap = merge_texture_slots(&mut self.textures, other.textures.iter().cloned());

        for seg in &other.segments {
            let mut segment = seg.clone();
            segment.quad_range.start += q_offset;
            segment.quad_range.end += q_offset;
            segment.text_range.start += t_offset;
            segment.text_range.end += t_offset;
            segment.icon_range.start += i_offset;
            segment.icon_range.end += i_offset;
            segment.svg_range.start += s_offset;
            segment.svg_range.end += s_offset;
            self.segments.push(segment);
        }
        self.quads.extend_from_slice(&other.quads);
        self.text_quads.extend_from_slice(&other.text_quads);
        self.icon_quads.extend_from_slice(&other.icon_quads);
        self.svg_commands.extend_from_slice(&other.svg_commands);

        #[cfg(feature = "assets")]
        remap_texture_indices(&mut self.quads[q_offset as usize..], &remap);

        if let Some(other_damage) = other.damage_union {
            self.damage_union = match self.damage_union {
                Some(current) => Some(current.union(other_damage)),
                None => Some(other_damage),
            };
        }
    }

    /// Starts a new **layer**: everything drawn after this call is painted
    /// above everything drawn before it, text included.
    ///
    /// Inside one segment the renderer draws all quads, then all text, then
    /// icons and SVG, so text drawn early (e.g. canvas labels) would otherwise
    /// end up on top of panels drawn later. A layer is simply a new segment
    /// with the same scissor. No-op when the current segment is empty.
    pub fn begin_layer(&mut self) {
        let empty = self.segments.last().map_or(true, |s| {
            s.quad_range.is_empty()
                && s.text_range.is_empty()
                && s.icon_range.is_empty()
                && s.svg_range.is_empty()
        });
        if empty {
            return;
        }
        let q = self.quads.len() as u32;
        let t = self.text_quads.len() as u32;
        let i = self.icon_quads.len() as u32;
        let s = self.svg_commands.len() as u32;
        self.segments.push(DrawSegment {
            quad_range: q..q,
            text_range: t..t,
            icon_range: i..i,
            svg_range: s..s,
            scissor: self.current_scissor,
        });
    }

    /// Ensures there is an active segment for the current scissor.
    pub fn ensure_segment(&mut self) {
        let needs_new = match self.segments.last() {
            Some(last) => last.scissor != self.current_scissor,
            None => true,
        };

        if needs_new {
            let q_start = self.quads.len() as u32;
            let t_start = self.text_quads.len() as u32;
            let i_start = self.icon_quads.len() as u32;
            let s_start = self.svg_commands.len() as u32;
            self.segments.push(DrawSegment {
                quad_range: q_start..q_start,
                text_range: t_start..t_start,
                icon_range: i_start..i_start,
                svg_range: s_start..s_start,
                scissor: self.current_scissor,
            });
        }
    }

    pub fn update_last_segment(&mut self) {
        self.ensure_segment();
        if let Some(last) = self.segments.last_mut() {
            last.quad_range.end = self.quads.len() as u32;
            last.text_range.end = self.text_quads.len() as u32;
            last.icon_range.end = self.icon_quads.len() as u32;
            last.svg_range.end = self.svg_commands.len() as u32;
        }
    }

    /// Registers a texture in the current batch and returns its slot index.
    #[cfg(feature = "assets")]
    pub fn reserve_texture_slot(
        &mut self,
        texture: std::sync::Arc<ferrous_assets::Texture2d>,
    ) -> u32 {
        if let Some(pos) = self
            .textures
            .iter()
            .position(|t| std::sync::Arc::ptr_eq(t, &texture))
        {
            return pos as u32;
        }
        if self.textures.len() as u32 >= crate::MAX_TEXTURE_SLOTS {
            panic!(
                "Exceeded texture limit per UI batch (max={})",
                crate::MAX_TEXTURE_SLOTS
            );
        }
        let idx = self.textures.len() as u32;
        self.textures.push(texture);
        idx
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
        self.rect_radii(x, y, w, h, color, [0.0; 4]);
    }

    pub fn rect_r(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4], radius: f32) {
        self.rect_radii(x, y, w, h, color, [radius; 4]);
    }

    pub fn rect_radii(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4], radii: [f32; 4]) {
        self.push_quad(GuiQuad {
            pos: [x, y],
            size: [w, h],
            uv0: [0.0, 0.0],
            uv1: [1.0, 1.0],
            color,
            color_b: [0.0; 4],
            radii,
            tex_index: 0,
            flags: 0,
            z_order: 0.0,
            node_id: 0,
        });
        self.update_last_segment();
    }

    /// Pushes an outline quad: the rounded-rect border is drawn inside `w`×`h`
    /// with `width` physical pixels of stroke.
    ///
    /// Packing follows `assets/shaders/gui.wgsl`, which reads the border width
    /// from `uv0.x`; `uv1` is unused by the border path (it is set to
    /// `[0, 0]` so the interpolated `uv` cannot leak gradient data).
    pub fn border(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: [f32; 4],
        radii: [f32; 4],
        width: f32,
    ) {
        self.push_quad(GuiQuad {
            pos: [x, y],
            size: [w, h],
            uv0: [width, 0.0], // gui.wgsl: border width (uv0.x)
            uv1: [0.0, 0.0],
            color,
            color_b: [0.0; 4],
            radii,
            tex_index: 0,
            flags: crate::BORDER_BIT,
            z_order: 0.0,
            node_id: 0,
        });
        self.update_last_segment();
    }

    /// Pushes a soft drop shadow for the rectangle at `(x, y, w, h)`.
    ///
    /// `gui.wgsl` computes the shadow SDF from the **unexpanded** rectangle
    /// size in `color_b.xy` and the blur radius in `uv0.x`, centred on the
    /// quad.  The quad is therefore expanded by `blur * 2 + |spread| + the
    /// larger offset` around the *offset* rectangle, so the unexpanded rect
    /// lands where the shadow belongs.  `uv1` keeps the offset for debugging;
    /// the shader does not read it.
    pub fn shadow(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: [f32; 4],
        blur_radius: f32,
        spread: f32,
        offset: [f32; 2],
        radii: [f32; 4],
    ) {
        // To include the shadow in the SDF, the quad must be expanded around
        // the offset rectangle.
        let expand = blur_radius * 2.0 + spread.abs() + offset[0].abs().max(offset[1].abs());

        self.push_quad(GuiQuad {
            pos: [x + offset[0] - expand, y + offset[1] - expand],
            size: [w + expand * 2.0, h + expand * 2.0],
            uv0: [blur_radius, 0.0],     // gui.wgsl: blur radius (uv0.x)
            uv1: [offset[0], offset[1]], // informational only
            color,
            color_b: [w, h, 0.0, 0.0], // gui.wgsl: unexpanded size (color_b.xy)
            radii,
            tex_index: 0,
            flags: crate::SHADOW_BIT,
            z_order: 0.0,
            node_id: 0,
        });
        self.update_last_segment();
    }

    pub fn shadow_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        // Helper MD default shadow
        self.shadow(
            x,
            y,
            w,
            h,
            [0.0, 0.0, 0.0, 0.35],
            8.0,
            0.0,
            [0.0, 4.0],
            [0.0; 4],
        );
    }

    /// Pushes one anti-aliased line segment.
    ///
    /// The quad is the segment's bounding box expanded by `width / 2 + 1`
    /// pixels on every side; `uv0`/`uv1` carry the endpoints in pixels
    /// relative to that box and `radii.x` carries the half width. `gui.wgsl`
    /// computes the exact distance to the segment per fragment, so diagonals
    /// are anti-aliased and both ends are round-capped. The packing is
    /// documented on [`crate::LINE_BIT`].
    ///
    /// A non-positive width or a transparent colour is ignored. `color` is
    /// straight RGBA, like every other batch quad.
    pub fn line(&mut self, from: [f32; 2], to: [f32; 2], width: f32, color: [f32; 4]) {
        if !(width > 0.0) || color[3] <= 0.0 {
            return;
        }
        let half = (width * 0.5).max(0.5);
        let pad = half + 1.0;
        let min = [from[0].min(to[0]) - pad, from[1].min(to[1]) - pad];
        let max = [from[0].max(to[0]) + pad, from[1].max(to[1]) + pad];
        self.push_quad(GuiQuad {
            pos: min,
            size: [max[0] - min[0], max[1] - min[1]],
            uv0: [from[0] - min[0], from[1] - min[1]],
            uv1: [to[0] - min[0], to[1] - min[1]],
            color,
            color_b: [0.0; 4],
            radii: [half, 0.0, 0.0, 0.0],
            tex_index: 0,
            flags: crate::LINE_BIT,
            z_order: 0.0,
            node_id: 0,
        });
        self.update_last_segment();
    }

    /// Pushes a connected polyline as one anti-aliased [`Self::line`] per
    /// segment. Adjacent segments share their endpoint; round caps make the
    /// joint continuous. Fewer than two points draw nothing.
    ///
    /// # Translucent joints
    ///
    /// Each segment is an independent quad with a round cap at both ends, so
    /// the caps overlap at every shared point. With an opaque colour this is
    /// invisible; with a translucent one the joint is composited twice and
    /// reads darker (coverage `1 - (1 - a)^2` instead of `a`). Draw the pieces
    /// as separate strokes when exact coverage matters, or accept the joint
    /// accent. The geometry of the overlap is pinned by
    /// `polyline_joints_overlap_at_the_shared_endpoint`.
    pub fn polyline(&mut self, points: &[[f32; 2]], width: f32, color: [f32; 4]) {
        for pair in points.windows(2) {
            self.line(pair[0], pair[1], width, color);
        }
    }

    pub fn rect_textured(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: [f32; 4],
        uv0: [f32; 2],
        uv1: [f32; 2],
        tex_index: u32,
    ) {
        self.push_quad(GuiQuad {
            pos: [x, y],
            size: [w, h],
            uv0,
            uv1,
            color,
            color_b: [0.0; 4],
            radii: [0.0; 4],
            tex_index,
            flags: TEXTURED_BIT,
            z_order: 0.0,
            node_id: 0,
        });
        self.update_last_segment();
    }

    #[cfg(feature = "assets")]
    pub fn image(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        texture: std::sync::Arc<ferrous_assets::Texture2d>,
        uv0: [f32; 2],
        uv1: [f32; 2],
        color: [f32; 4],
    ) {
        let idx = self.reserve_texture_slot(texture);
        self.rect_textured(x, y, w, h, color, uv0, uv1, idx);
    }

    pub fn len(&self) -> usize {
        self.quads.len()
    }

    pub fn is_empty(&self) -> bool {
        self.quads.is_empty()
            && self.text_quads.is_empty()
            && self.icon_quads.is_empty()
            && self.svg_commands.is_empty()
    }

    /// Byte view of the live quad instances, ready for `write_buffer`.
    ///
    /// The slice covers exactly `quads.len()` instances (no spare capacity),
    /// so the renderer never uploads the unused tail of its GPU buffer.
    pub fn as_quad_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.quads)
    }

    /// Byte view of the live text glyph instances, ready for `write_buffer`.
    ///
    /// Covers exactly `text_quads.len()` instances, like [`Self::as_quad_bytes`].
    pub fn as_text_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.text_quads)
    }

    /// Byte view of the live icon glyph instances, ready for `write_buffer`.
    ///
    /// Covers exactly `icon_quads.len()` instances, like [`Self::as_quad_bytes`].
    pub fn as_icon_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.icon_quads)
    }

    pub fn push_clip(&mut self, rect: ferrous_ui_core::Rect) {
        let new_rect = if let Some(current) = self.current_scissor {
            current.intersect(&rect)
        } else {
            rect
        };
        self.scissor_stack.push(new_rect);
        self.current_scissor = Some(new_rect);
        self.ensure_segment();
    }

    pub fn pop_clip(&mut self) {
        self.scissor_stack.pop();
        self.current_scissor = self.scissor_stack.last().copied();
        self.ensure_segment();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrous_ui_core::render_collector::{NodeVisit, RenderCollector};
    use ferrous_ui_core::{DrawContext, NodeId, Overflow, Style, UiTree, Widget};
    use std::cell::Cell;
    use std::rc::Rc;

    /// Solid-colour widget whose colour can be changed from the test.
    struct Solid {
        color: Rc<Cell<[f32; 4]>>,
    }

    impl Widget<()> for Solid {
        fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
            cmds.push(RenderCommand::Quad {
                rect: ctx.rect,
                color: self.color.get(),
                radii: [0.0; 4],
                flags: 0,
            });
        }
    }

    fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    /// Root at `(0, 0, 200, 200)` with one child at `(10, 10, 50, 50)`.
    fn two_node_tree() -> (UiTree<()>, NodeId, Rc<Cell<[f32; 4]>>, Rc<Cell<[f32; 4]>>) {
        let root_color = Rc::new(Cell::new([0.1, 0.2, 0.3, 1.0]));
        let child_color = Rc::new(Cell::new([0.4, 0.5, 0.6, 1.0]));
        let mut tree = UiTree::<()>::new();
        let root = tree.add_node(
            Box::new(Solid {
                color: root_color.clone(),
            }),
            None,
        );
        tree.set_node_rect(root, rect(0.0, 0.0, 200.0, 200.0));
        let child = tree.add_node(
            Box::new(Solid {
                color: child_color.clone(),
            }),
            Some(root),
        );
        tree.set_node_rect(child, rect(10.0, 10.0, 50.0, 50.0));
        (tree, child, child_color, root_color)
    }

    #[cfg(feature = "text")]
    fn push(batch: &mut GuiBatch, node: NodeBatch<'_>) {
        batch.push_node(node, None);
    }

    #[cfg(not(feature = "text"))]
    fn push(batch: &mut GuiBatch, node: NodeBatch<'_>) {
        batch.push_node(node);
    }

    fn render(tree: &mut UiTree<()>, batch: &mut GuiBatch, viewport: Rect, epoch: u64) {
        batch.begin_retained_frame(epoch);
        RenderCollector::collect_with(tree, viewport, |visit| match visit {
            NodeVisit::Body(node) => push(&mut *batch, node),
            NodeVisit::PopClip { .. } => batch.pop_clip(),
        });
        batch.end_retained_frame();
    }

    fn quad_of(build: impl FnOnce(&mut GuiBatch)) -> GuiQuad {
        let mut batch = GuiBatch::new();
        build(&mut batch);
        assert_eq!(batch.quads.len(), 1);
        batch.quads[0]
    }

    /// Widget with an animated (procedural) background.
    struct ProceduralBackground;

    impl Widget<()> for ProceduralBackground {
        fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
            cmds.push(RenderCommand::GradientQuad {
                rect: ctx.rect,
                background: Background::Procedural(std::sync::Arc::new(|u, v| [u, v, 0.0, 1.0])),
                radii: [0.0; 4],
                raster_resolution: (4, 4),
            });
        }
    }

    #[test]
    fn a_procedural_background_is_rebuilt_every_frame() {
        // The closure is evaluated at batch time, so replaying the previous
        // slices would freeze the animation.
        let mut tree = UiTree::<()>::new();
        let root = tree.add_node(Box::new(ProceduralBackground), None);
        tree.set_node_rect(root, rect(0.0, 0.0, 100.0, 100.0));
        let viewport = rect(0.0, 0.0, 800.0, 600.0);
        let mut batch = GuiBatch::new();

        render(&mut tree, &mut batch, viewport, 3);
        let first = batch.quads.len();
        batch.clear();
        render(&mut tree, &mut batch, viewport, 3);

        assert_eq!(batch.retained_rebuild_count(), 1, "no replay");
        assert_eq!(batch.quads.len(), first);
    }

    #[test]
    fn border_puts_width_in_uv0_where_the_shader_reads_it() {
        let quad = quad_of(|b| {
            b.border(10.0, 20.0, 100.0, 40.0, [1.0; 4], [4.0; 4], 2.5);
        });
        assert_eq!(
            quad.uv0,
            [2.5, 0.0],
            "gui.wgsl reads border_width from uv0.x"
        );
        assert_eq!(quad.uv1, [0.0, 0.0], "uv1 must not carry the width");
        assert_eq!(quad.flags, crate::BORDER_BIT);
        assert_eq!(quad.pos, [10.0, 20.0]);
        assert_eq!(quad.size, [100.0, 40.0]);
    }

    #[test]
    fn line_packs_endpoints_and_bounds_for_the_shader() {
        let quad = quad_of(|b| b.line([10.0, 20.0], [110.0, 60.0], 2.0, [1.0; 4]));
        assert_eq!(quad.flags, crate::LINE_BIT);
        // pad = half width (1) + 1 AA pixel; the endpoints are stored local
        // to the expanded box.
        assert_eq!(quad.pos, [8.0, 18.0]);
        assert_eq!(quad.size, [104.0, 44.0]);
        assert_eq!(quad.uv0, [2.0, 2.0]);
        assert_eq!(quad.uv1, [102.0, 42.0]);
        assert_eq!(quad.radii[0], 1.0);
    }

    #[test]
    fn a_polyline_emits_one_quad_per_segment() {
        let mut batch = GuiBatch::new();
        batch.polyline(
            &[[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
            1.0,
            [1.0; 4],
        );
        assert_eq!(batch.quads.len(), 3);
        assert!(batch.quads.iter().all(|q| q.flags == crate::LINE_BIT));
        // A single point (and a degenerate list) draws nothing.
        batch.polyline(&[[0.0, 0.0]], 1.0, [1.0; 4]);
        assert_eq!(batch.quads.len(), 3);
    }

    #[test]
    fn polyline_joints_overlap_at_the_shared_endpoint() {
        // Half-alpha segments: the two round caps cover the same pixels at the
        // joint, which is why a translucent polyline composites that point
        // twice. The test pins the geometry behind the documented limitation.
        let mut batch = GuiBatch::new();
        batch.polyline(
            &[[10.0, 10.0], [60.0, 10.0], [60.0, 60.0]],
            8.0,
            [1.0, 0.0, 0.0, 0.5],
        );
        assert_eq!(batch.quads.len(), 2);
        let joint = [60.0_f32, 10.0];
        let contains = |quad: &GuiQuad| {
            joint[0] >= quad.pos[0]
                && joint[0] <= quad.pos[0] + quad.size[0]
                && joint[1] >= quad.pos[1]
                && joint[1] <= quad.pos[1] + quad.size[1]
        };
        assert!(
            batch.quads.iter().all(contains),
            "both caps must cover the shared endpoint"
        );
        let [first, second] = [&batch.quads[0], &batch.quads[1]];
        let overlap_x = (first.pos[0] + first.size[0]).min(second.pos[0] + second.size[0])
            - first.pos[0].max(second.pos[0]);
        let overlap_y = (first.pos[1] + first.size[1]).min(second.pos[1] + second.size[1])
            - first.pos[1].max(second.pos[1]);
        assert!(
            overlap_x > 0.0 && overlap_y > 0.0,
            "the joint quads must overlap (x {overlap_x}, y {overlap_y})"
        );
    }

    #[test]
    fn texture_slots_remap_into_the_destination_and_dedup() {
        let texture_a = std::sync::Arc::new(1u32);
        let texture_b = std::sync::Arc::new(2u32);

        // Destination already has B; source slots are [A, B].
        let mut destination = vec![texture_b.clone()];
        let remap =
            merge_texture_slots(&mut destination, vec![texture_a.clone(), texture_b.clone()]);
        assert_eq!(remap, vec![1, 0], "A is new, B reuses destination slot 0");
        assert_eq!(destination.len(), 2, "one different texture was added");

        // Extending again must not grow the texture list.
        let remap_again = merge_texture_slots(&mut destination, vec![texture_a, texture_b]);
        assert_eq!(remap_again, vec![1, 0]);
        assert_eq!(destination.len(), 2, "the same arcs are reused");
    }

    #[test]
    fn remapping_rewrites_textured_quads_and_leaves_solids_at_slot_zero() {
        let solid = GuiQuad::solid([0.0; 2], [1.0; 2], [1.0; 4], [0.0; 4], 0, 0.0, 0);
        let mut textured = solid;
        textured.flags = TEXTURED_BIT;
        textured.tex_index = 1;
        let mut quads = [solid, textured];

        remap_texture_indices(&mut quads, &[4, 7]);

        assert_eq!(quads[0].tex_index, 0, "a solid keeps slot 0");
        assert_eq!(quads[1].tex_index, 7, "source slot 1 -> destination slot 7");
    }

    #[test]
    fn extending_merges_damage_and_rebases_segments() {
        let mut destination = GuiBatch::new();
        destination.rect(0.0, 0.0, 10.0, 10.0, [1.0; 4]);

        let mut source = GuiBatch::new();
        source.push_clip(rect(40.0, 40.0, 100.0, 100.0));
        source.rect(50.0, 50.0, 10.0, 10.0, [0.0, 0.0, 1.0, 1.0]);

        destination.extend(source);

        assert_eq!(
            destination.damage_union,
            Some(rect(0.0, 0.0, 60.0, 60.0)),
            "the two rects union into the bounding box"
        );
        let last = destination.segments.last().expect("a segment");
        assert_eq!(last.quad_range, 1..2, "the appended quad is rebased");
        assert_eq!(last.scissor, Some(rect(40.0, 40.0, 100.0, 100.0)));
    }

    #[test]
    fn extend_from_appends_like_extend_without_consuming_the_source() {
        let mut destination = GuiBatch::new();
        destination.rect(0.0, 0.0, 10.0, 10.0, [1.0; 4]);

        let mut source = GuiBatch::new();
        source.push_clip(rect(40.0, 40.0, 100.0, 100.0));
        source.rect(50.0, 50.0, 10.0, 10.0, [0.0, 0.0, 1.0, 1.0]);

        destination.extend_from(&source);
        destination.extend_from(&source);

        assert_eq!(
            destination.quads.len(),
            3,
            "one destination quad plus the source twice"
        );
        assert_eq!(
            source.quads.len(),
            1,
            "the borrowed source keeps its primitives"
        );
        let last = destination.segments.last().expect("a segment");
        assert_eq!(last.quad_range, 2..3, "the second append is rebased");
        assert_eq!(last.scissor, Some(rect(40.0, 40.0, 100.0, 100.0)));
    }

    #[test]
    fn byte_views_cover_exactly_the_live_instances() {
        let mut batch = GuiBatch::new();
        assert!(batch.as_quad_bytes().is_empty());
        assert!(batch.as_text_bytes().is_empty());
        assert!(batch.as_icon_bytes().is_empty());

        batch.rect(0.0, 0.0, 10.0, 10.0, [1.0; 4]);
        let glyph = TextQuad {
            pos: [0.0, 0.0],
            size: [8.0, 8.0],
            uv0: [0.0, 0.0],
            uv1: [1.0, 1.0],
            color: [1.0; 4],
            z_order: 0.0,
            node_id: 0,
        };
        batch.push_text(glyph);
        batch.push_icon(glyph);

        assert_eq!(
            batch.as_quad_bytes().len(),
            batch.quads.len() * std::mem::size_of::<GuiQuad>(),
            "uploaded bytes must match the live instances, not the capacity"
        );
        assert_eq!(
            batch.as_text_bytes().len(),
            batch.text_quads.len() * std::mem::size_of::<TextQuad>()
        );
        assert_eq!(
            batch.as_icon_bytes().len(),
            batch.icon_quads.len() * std::mem::size_of::<TextQuad>()
        );
    }

    #[test]
    fn shadow_packs_blur_and_unexpanded_size_where_the_shader_reads_them() {
        let quad = quad_of(|b| {
            b.shadow(
                100.0,
                200.0,
                300.0,
                150.0,
                [0.0, 0.0, 0.0, 0.5],
                8.0,
                0.0,
                [0.0, 4.0],
                [6.0; 4],
            );
        });
        assert_eq!(quad.uv0[0], 8.0, "gui.wgsl reads the blur from uv0.x");
        assert_eq!(quad.color_b[0], 300.0, "unexpanded width in color_b.x");
        assert_eq!(quad.color_b[1], 150.0, "unexpanded height in color_b.y");
        // The quad is expanded by blur * 2 + |offset| and moved by the offset,
        // so the shader's centred, unexpanded rect lands on the offset rect.
        let expand = 8.0 * 2.0 + 4.0;
        assert!((quad.pos[0] - (100.0 + 0.0 - expand)).abs() < 1e-4);
        assert!((quad.pos[1] - (200.0 + 4.0 - expand)).abs() < 1e-4);
        assert!((quad.size[0] - (300.0 + expand * 2.0)).abs() < 1e-4);
        assert_eq!(quad.flags, crate::SHADOW_BIT);
    }

    #[test]
    fn a_clean_second_render_is_byte_identical_to_the_first() {
        let (mut tree, _, _, _) = two_node_tree();
        let viewport = rect(0.0, 0.0, 800.0, 600.0);
        let mut batch = GuiBatch::new();

        render(&mut tree, &mut batch, viewport, 7);
        assert_eq!(batch.quads.len(), 2);
        let quads = batch.as_quad_bytes().to_vec();
        let texts = batch.as_text_bytes().to_vec();
        let icons = batch.as_icon_bytes().to_vec();
        let segments = batch.segments.clone();

        batch.clear();
        render(&mut tree, &mut batch, viewport, 7);

        assert_eq!(batch.as_quad_bytes(), quads.as_slice());
        assert_eq!(batch.as_text_bytes(), texts.as_slice());
        assert_eq!(batch.as_icon_bytes(), icons.as_slice());
        assert_eq!(
            batch.segments, segments,
            "replay must reproduce the same segment/scissor layout"
        );
        assert_eq!(
            batch.retained_rebuild_count(),
            0,
            "a frame with no redraws must replay every node"
        );
    }

    #[test]
    fn a_single_node_repaint_rebuilds_only_that_node() {
        let (mut tree, child, child_color, _) = two_node_tree();
        let viewport = rect(0.0, 0.0, 800.0, 600.0);
        let mut batch = GuiBatch::new();

        render(&mut tree, &mut batch, viewport, 7);
        batch.clear();
        render(&mut tree, &mut batch, viewport, 7);
        assert_eq!(batch.retained_rebuild_count(), 0);

        // A hover change marks exactly one node paint-dirty.
        child_color.set([1.0, 0.0, 0.0, 1.0]);
        tree.mark_paint_dirty(child);
        batch.clear();
        render(&mut tree, &mut batch, viewport, 7);

        assert_eq!(
            batch.retained_rebuild_count(),
            1,
            "only the hovered node may be converted again"
        );
        assert_eq!(batch.quads.len(), 2);
        assert!(
            batch
                .quads
                .iter()
                .any(|quad| quad.color == [1.0, 0.0, 0.0, 1.0]),
            "the hovered node's new colour must be emitted"
        );
        assert!(
            batch
                .quads
                .iter()
                .any(|quad| quad.color == [0.1, 0.2, 0.3, 1.0]),
            "the untouched node must come from its retained ranges"
        );
    }

    #[cfg(feature = "text")]
    #[test]
    fn a_changed_font_identity_invalidates_the_retained_ranges() {
        let (mut tree, _, _, _) = two_node_tree();
        let viewport = rect(0.0, 0.0, 800.0, 600.0);
        let mut batch = GuiBatch::new();

        let mut render_with_font = |batch: &mut GuiBatch, font_identity: u64| {
            batch.begin_retained_frame(7);
            RenderCollector::collect_with(&mut tree, viewport, |visit| match visit {
                NodeVisit::Body(node) => batch.push_node_with_font_identity(node, font_identity),
                NodeVisit::PopClip { .. } => batch.pop_clip(),
            });
            batch.end_retained_frame();
        };

        render_with_font(&mut batch, 11);
        batch.clear();
        render_with_font(&mut batch, 11);
        assert_eq!(batch.retained_rebuild_count(), 0);

        batch.clear();
        render_with_font(&mut batch, 12);
        assert_eq!(
            batch.retained_rebuild_count(),
            2,
            "a different atlas identity must invalidate every node"
        );
    }

    #[test]
    fn removed_nodes_drop_their_retained_ranges() {
        let (mut tree, child, _, _) = two_node_tree();
        let viewport = rect(0.0, 0.0, 800.0, 600.0);
        let mut batch = GuiBatch::new();

        render(&mut tree, &mut batch, viewport, 3);
        assert_eq!(batch.retained.len(), 2);

        tree.remove_node(child);
        batch.clear();
        render(&mut tree, &mut batch, viewport, 3);
        assert_eq!(
            batch.retained.len(),
            1,
            "a node absent from the tree must not keep its retained ranges"
        );
    }

    #[test]
    fn a_subtree_outside_its_overflow_clip_is_not_batched() {
        let root_color = Rc::new(Cell::new([1.0; 4]));
        let child_color = Rc::new(Cell::new([0.0, 1.0, 0.0, 1.0]));
        let mut tree = UiTree::<()>::new();
        let root = tree.add_node(
            Box::new(Solid {
                color: root_color.clone(),
            }),
            None,
        );
        tree.set_node_rect(root, rect(0.0, 0.0, 100.0, 100.0));
        let child = tree.add_node(
            Box::new(Solid {
                color: child_color.clone(),
            }),
            Some(root),
        );
        tree.set_node_rect(child, rect(200.0, 200.0, 50.0, 50.0));
        let style = Style {
            overflow: Overflow::Hidden,
            ..Style::default()
        };
        tree.set_node_style(root, style);

        let viewport = rect(0.0, 0.0, 800.0, 600.0);
        let mut batch = GuiBatch::new();
        render(&mut tree, &mut batch, viewport, 9);

        assert_eq!(
            batch.quads.len(),
            1,
            "the off-clip child must be skipped, not batched"
        );
    }
}
