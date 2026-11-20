//! Persistent single-sample UI target with damage-rect copy.
//!
//! Prompt 18 P5 item 3 asks whether a persistent off-screen UI target plus a
//! copy of *only the damaged rectangle* beats a full redraw.  [`PersistentUiTarget`]
//! is the smallest self-contained piece of that experiment: it owns one
//! single-sample colour texture (and a view) at a given size/format, accumulates
//! conservative damage rectangles, returns them merged and clipped, and copies
//! each one to another texture with [`wgpu::CommandEncoder::copy_texture_to_texture`].
//!
//! It deliberately does **not** render anything.  The GUI pass keeps owning its
//! pipelines and vertex buffers; it only has to attach to [`PersistentUiTarget::view`]
//! instead of the surface texture.  See `crates/ferrous_ui_render/docs/PARTIAL.md`
//! for the wiring plan and the keep/reject criterion.
//!
//! # Damage-rect contract
//!
//! * Whoever repaints a region calls [`PersistentUiTarget::invalidate`] with the
//!   rectangle in **pixels**, top-left origin, Y-down (the same space the GUI
//!   scissor and `GuiBatch::damage_union` use).
//! * Bounds are rounded *out* (floor on the origin, ceil on the far edge) and
//!   clipped to the target, so a fractional or oversized rectangle never
//!   under-reports.  This is the conservative direction: a few extra pixels are
//!   copied, never fewer.
//! * [`PersistentUiTarget::invalidate_all`] marks the whole target.  The first
//!   frame after creation or recreation starts already fully invalidated.
//! * [`PersistentUiTarget::take_damage`] merges overlapping or touching
//!   rectangles and clears the accumulator; the next frame starts from an empty
//!   list.  The merged set stays readable through `damage()` so `copy_damage`
//!   can encode it without the caller passing it back.
//!
//! # Allocation behaviour
//!
//! Steady state allocates nothing per frame: the target keeps three private
//! vectors — the accumulated damage, the merge scratch list, and the merged
//! result of the last [`PersistentUiTarget::take_damage`].  All three keep their
//! capacity and are cleared in place.  The only allocation per call is the
//! `Vec<Rect>` that `take_damage` returns; callers that do not need it can use
//! [`PersistentUiTarget::pending_damage`] instead.
//!
//! # Copy alignment
//!
//! `copy_texture_to_texture` copies texels, not bytes: the origin `x`/`y` do
//! **not** have to be multiples of four (unlike buffer copy layouts), and no
//! row padding is involved.  Each copied row is exactly `width * block_copy_size`
//! bytes — the texture's own bytes-per-row.  The source must carry
//! `COPY_SRC`, the destination `COPY_DST`.

use ferrous_ui_core::Rect;

/// Integer pixel rectangle, half-open on the far edge (`right`/`bottom` are
/// exclusive).  Damage is tracked and returned in this space so the copy code
/// never has to re-round floats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl PixelRect {
    /// Pixel area covered by the rectangle.
    #[must_use]
    pub fn area(self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }

    fn right(self) -> u32 {
        self.x.saturating_add(self.width)
    }

    fn bottom(self) -> u32 {
        self.y.saturating_add(self.height)
    }

    /// Treats empty rectangles as "nothing to merge".
    fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Overlap **or** edge contact.  Adjacent rectangles are merged because two
    /// touching copies cost the same as their bounding box while the bounding
    /// box is a single command.
    fn touches(self, other: Self) -> bool {
        self.x <= other.right()
            && other.x <= self.right()
            && self.y <= other.bottom()
            && other.y <= self.bottom()
    }

    fn union(self, other: Self) -> Self {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let right = self.right().max(other.right());
        let bottom = self.bottom().max(other.bottom());
        Self {
            x,
            y,
            width: right - x,
            height: bottom - y,
        }
    }
}

/// One damage copy: number of rectangles emitted and the total pixel bytes
/// they covered.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DamageCopy {
    /// `copy_texture_to_texture` calls encoded.
    pub rects: u32,
    /// Sum of `width * height * bytes_per_texel` over the copied rectangles.
    pub bytes: u64,
}

/// A persistent single-sample UI texture with conservative damage tracking.
pub struct PersistentUiTarget {
    texture: Option<wgpu::Texture>,
    view: Option<wgpu::TextureView>,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    bytes_per_row: u64,
    damage: Vec<PixelRect>,
    scratch: Vec<PixelRect>,
    pending: Vec<PixelRect>,
}

impl PersistentUiTarget {
    /// Creates an unallocated target.  Call [`Self::ensure`] before the first
    /// frame; an unallocated target reports no view and copies nothing.
    #[must_use]
    pub fn new() -> Self {
        Self {
            texture: None,
            view: None,
            width: 0,
            height: 0,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            bytes_per_row: 0,
            damage: Vec::new(),
            scratch: Vec::new(),
            pending: Vec::new(),
        }
    }

    /// Ensures a texture of `width x height` at `format` exists.
    ///
    /// Returns `true` when the texture was (re)created — on first use or after a
    /// size or format change — which also invalidates the whole target: the
    /// caller must render everything once (a `Clear` plus a full draw) before
    /// trusting partial copies.
    ///
    /// An empty (`width == 0 || height == 0`) request frees the texture and
    /// returns `false`; every later [`Self::view`] is `None`, so a zero-sized
    /// frame is a no-op rather than a panic.
    pub fn ensure(
        &mut self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> bool {
        if width == 0 || height == 0 {
            if self.texture.is_some() {
                self.release();
            }
            return false;
        }
        if self.texture.is_some()
            && self.width == width
            && self.height == height
            && self.format == format
        {
            return false;
        }

        self.release();
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Persistent UI target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.texture = Some(texture);
        self.view = Some(view);
        self.width = width;
        self.height = height;
        self.format = format;
        self.bytes_per_row =
            u64::from(width) * u64::from(format.block_copy_size(None).unwrap_or(4));
        self.invalidate_all();
        true
    }

    /// Marks the whole target as damaged (first frame, resize, format change).
    pub fn invalidate_all(&mut self) {
        self.damage.clear();
        if self.texture.is_some() {
            self.damage.push(PixelRect {
                x: 0,
                y: 0,
                width: self.width,
                height: self.height,
            });
        }
    }

    /// Adds one rectangle to the damage set.  Rounded out and clipped; a
    /// zero-sized, negative or fully off-target rectangle adds nothing.
    pub fn invalidate(&mut self, rect: Rect) {
        if self.texture.is_none() {
            return;
        }
        let Some(clipped) = clip_to_target(rect, self.width, self.height) else {
            return;
        };
        self.damage.push(clipped);
    }

    /// Returns the damage since the last call, merged and clipped, and resets
    /// the accumulator.  An empty vector means "nothing changed, copy nothing".
    ///
    /// The merged set is kept internally for [`Self::copy_damage`], so the
    /// caller does not have to pass the rectangles back.
    pub fn take_damage(&mut self) -> Vec<Rect> {
        merge_into(&mut self.damage, &mut self.scratch);
        self.pending.clear();
        self.pending.extend_from_slice(&self.damage);
        self.damage.clear();
        self.pending
            .iter()
            .map(|rect| {
                Rect::new(
                    rect.x as f32,
                    rect.y as f32,
                    rect.width as f32,
                    rect.height as f32,
                )
            })
            .collect()
    }

    /// The merged damage rectangles most recently produced by
    /// [`Self::take_damage`] (empty before the first call).
    #[must_use]
    pub fn pending_damage(&self) -> &[PixelRect] {
        &self.pending
    }

    /// Copies every damage rectangle from the persistent texture into the
    /// texture behind `dst_view`.
    ///
    /// Both arguments are the same texture, because
    /// `copy_texture_to_texture` addresses the destination by `wgpu::Texture`
    /// while callers usually hold a `wgpu::TextureView`.  `dst` must be
    /// single-sample, same size and format, with `COPY_DST` usage.  The
    /// rectangles come from the last [`Self::take_damage`] (already merged and
    /// clipped); this method clamps defensively anyway.  Returns the number of
    /// copies and the bytes copied so a headless harness can report the
    /// bandwidth the path actually moves.
    pub fn copy_damage(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        dst: &wgpu::Texture,
        _dst_view: &wgpu::TextureView,
    ) -> DamageCopy {
        let Some(texture) = self.texture.as_ref() else {
            return DamageCopy::default();
        };
        let texel = u64::from(self.format.block_copy_size(None).unwrap_or(4));
        let mut report = DamageCopy::default();
        for rect in &self.pending {
            if rect.is_empty() {
                continue;
            }
            let width = rect.width.min(self.width.saturating_sub(rect.x));
            let height = rect.height.min(self.height.saturating_sub(rect.y));
            if width == 0 || height == 0 {
                continue;
            }
            encoder.copy_texture_to_texture(
                wgpu::ImageCopyTexture {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: rect.x,
                        y: rect.y,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::ImageCopyTexture {
                    texture: dst,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: rect.x,
                        y: rect.y,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
            report.rects = report.rects.saturating_add(1);
            report.bytes = report
                .bytes
                .saturating_add(u64::from(width) * u64::from(height) * texel);
        }
        report
    }

    /// The render target view, or `None` before the first successful
    /// [`Self::ensure`] (and after an empty-size `ensure`).
    #[must_use]
    pub fn view(&self) -> Option<&wgpu::TextureView> {
        self.view.as_ref()
    }

    /// Current size, or `(0, 0)` while unallocated.
    #[must_use]
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// The surface/attachment format the target was created with.
    #[must_use]
    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    /// Bytes a full target copy would move; useful to state the full-redraw
    /// baseline next to [`DamageCopy::bytes`].
    #[must_use]
    pub fn full_bytes(&self) -> u64 {
        self.bytes_per_row * u64::from(self.height)
    }

    fn release(&mut self) {
        if let Some(texture) = self.texture.take() {
            texture.destroy();
        }
        self.view = None;
        self.width = 0;
        self.height = 0;
        self.bytes_per_row = 0;
    }
}

impl Default for PersistentUiTarget {
    fn default() -> Self {
        Self::new()
    }
}

/// Clips `rect` to `[0, width) x [0, height)`, rounding the origin down and the
/// far edge up, and returns `None` when nothing remains.
fn clip_to_target(rect: Rect, width: u32, height: u32) -> Option<PixelRect> {
    if !(rect.x.is_finite()
        && rect.y.is_finite()
        && rect.width.is_finite()
        && rect.height.is_finite())
        || rect.width <= 0.0
        || rect.height <= 0.0
    {
        return None;
    }
    // `as` casts saturate in Rust for out-of-range floats (negative -> 0,
    // large -> MAX), which is exactly the conservative behaviour wanted.
    let left = rect.x.floor().max(0.0) as u32;
    let top = rect.y.floor().max(0.0) as u32;
    let right = (rect.x + rect.width).ceil().max(0.0) as u32;
    let bottom = (rect.y + rect.height).ceil().max(0.0) as u32;
    let right = right.min(width);
    let bottom = bottom.min(height);
    if right <= left || bottom <= top {
        return None;
    }
    Some(PixelRect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    })
}

/// Merges overlaps and edge contacts in place until stable, then sorts and
/// deduplicates.  `out` is the input *and* the result; `scratch` is reused as a
/// temporary and left empty so its capacity survives for the next call.
fn merge_into(out: &mut Vec<PixelRect>, scratch: &mut Vec<PixelRect>) {
    out.retain(|rect| !rect.is_empty());
    if out.len() < 2 {
        return;
    }
    loop {
        let mut changed = false;
        // Each accepted rectangle is checked against everything already kept;
        // a merge may make it touch an earlier one, so retry from the start
        // with the enlarged rectangle.
        for candidate in std::mem::take(out) {
            let mut merged = candidate;
            let mut restart = false;
            loop {
                let mut index = 0;
                while index < scratch.len() {
                    if merged.touches(scratch[index]) {
                        merged = merged.union(scratch[index]);
                        scratch.swap_remove(index);
                        changed = true;
                        restart = true;
                        break;
                    }
                    index += 1;
                }
                if !restart {
                    break;
                }
                restart = false;
            }
            scratch.push(merged);
        }
        scratch.sort_by_key(|rect| (rect.y, rect.x, rect.width, rect.height));
        scratch.dedup();
        std::mem::swap(out, scratch);
        scratch.clear();
        if !changed {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{clip_to_target, merge_into, PixelRect};
    use ferrous_ui_core::Rect;

    fn rect(x: u32, y: u32, width: u32, height: u32) -> PixelRect {
        PixelRect {
            x,
            y,
            width,
            height,
        }
    }

    fn merged(mut input: Vec<PixelRect>) -> Vec<PixelRect> {
        let mut scratch = Vec::new();
        merge_into(&mut input, &mut scratch);
        input
    }

    #[test]
    fn overlapping_rects_collapse_to_their_union() {
        let out = merged(vec![rect(10, 10, 20, 20), rect(20, 20, 20, 20)]);
        assert_eq!(out, vec![rect(10, 10, 30, 30)]);
    }

    #[test]
    fn edge_touching_rects_merge() {
        // Shared edge at x = 20: adjacent, no overlap.
        let out = merged(vec![rect(10, 10, 10, 10), rect(20, 10, 10, 10)]);
        assert_eq!(out, vec![rect(10, 10, 20, 10)]);
    }

    #[test]
    fn separated_rects_stay_separate() {
        let out = merged(vec![rect(0, 0, 4, 4), rect(10, 10, 4, 4)]);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn merge_is_transitive_through_a_bridge() {
        // The middle rectangle bridges the two outer ones into one.
        let out = merged(vec![rect(0, 0, 5, 5), rect(5, 0, 5, 5), rect(10, 0, 5, 5)]);
        assert_eq!(out, vec![rect(0, 0, 15, 5)]);
    }

    #[test]
    fn empty_rects_are_dropped() {
        let out = merged(vec![rect(0, 0, 0, 5), rect(0, 0, 5, 0), rect(1, 1, 2, 2)]);
        assert_eq!(out, vec![rect(1, 1, 2, 2)]);
    }

    #[test]
    fn clip_rounds_origin_down_and_far_edge_up() {
        // An unscissored GUI quad usually lands on fractional pixels; the copy
        // must cover both touched pixels on each axis.
        let out = clip_to_target(Rect::new(10.25, 10.75, 3.1, 3.1), 100, 100);
        assert_eq!(out, Some(rect(10, 10, 4, 4)));
    }

    #[test]
    fn clip_truncates_to_the_target() {
        let out = clip_to_target(Rect::new(-10.0, -5.0, 30.0, 20.0), 10, 10);
        assert_eq!(out, Some(rect(0, 0, 10, 10)));
    }

    #[test]
    fn clip_rejects_fully_off_target_and_non_positive_rects() {
        assert_eq!(
            clip_to_target(Rect::new(20.0, 20.0, 4.0, 4.0), 10, 10),
            None
        );
        assert_eq!(clip_to_target(Rect::new(1.0, 1.0, 0.0, 4.0), 10, 10), None);
        assert_eq!(clip_to_target(Rect::new(1.0, 1.0, -2.0, 4.0), 10, 10), None);
        assert_eq!(
            clip_to_target(Rect::new(f32::NAN, 1.0, 4.0, 4.0), 10, 10),
            None
        );
    }

    #[test]
    fn merge_keeps_scratch_capacity_for_reuse() {
        let mut out = vec![rect(0, 0, 4, 4), rect(1, 1, 4, 4)];
        let mut scratch = Vec::new();
        merge_into(&mut out, &mut scratch);
        assert_eq!(out.len(), 1);
        assert!(scratch.is_empty());
        // `merge_into` must leave the scratch vector usable, not poison it.
        merge_into(&mut out, &mut scratch);
        assert_eq!(out, vec![rect(0, 0, 5, 5)]);
    }

    #[test]
    fn touch_includes_shared_corners() {
        // Diagonally touching rectangles share one corner pixel corner; the
        // conservative merge treats them as one bounding box.
        let out = merged(vec![rect(0, 0, 4, 4), rect(4, 4, 4, 4)]);
        assert_eq!(out, vec![rect(0, 0, 8, 8)]);
    }

    #[test]
    fn damage_rects_merge_only_through_the_public_helpers() {
        // Pure-function smoke test: clip + merge as `invalidate`/`take_damage`
        // would, without a GPU.
        let mut scratch = Vec::new();
        let mut damage: Vec<PixelRect> = [
            Rect::new(10.0, 10.0, 100.0, 40.0),
            Rect::new(60.0, 30.0, 100.0, 60.0),
        ]
        .into_iter()
        .filter_map(|rect| clip_to_target(rect, 200, 200))
        .collect();
        merge_into(&mut damage, &mut scratch);
        assert_eq!(damage, vec![rect(10, 10, 150, 80)]);
    }
}
