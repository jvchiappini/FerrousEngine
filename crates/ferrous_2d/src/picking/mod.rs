//! Canvas and scene picking for anything the engine draws.
//!
//! A caller submits a [`PickScene`]: pickable items with an affine transform, a
//! shape, a z index and an optional clip node. `PickScene::rebuild` indexes the
//! items by their world-space bounds with the shared
//! [`ferrous_core::spatial::SpatialIndex`] (the same structure GUIMaker routes
//! UI pointers with), so a point query is `O(cell)` and a marquee is
//! `O(cells + candidates)`, never a walk of every shape.
//!
//! Exact tests run in each item's local space through the inverse transform:
//! rounded rectangles, circles, filled and stroked paths (non-zero or even-odd),
//! text and image bounds with an optional alpha threshold. Rotation, scale and
//! skew are supported because the transform is a full 2D affine. Clips are a
//! chain of transformed rectangles; an item is only pickable where every
//! ancestor clip contains the point.
//!
//! A query allocates nothing: the index, the candidate buffer and the result
//! buffers are reused. A warm `pick` is a grid-cell lookup plus a handful of
//! exact tests.
//!
//! # GPU ID-buffer alternative
//!
//! The engine already ships an id-buffer pass for the UI (`id_buffer.wgsl`,
//! `ferrous_ui_render::GuiRenderer::hit_test_gpu`). For a 2D scene the same
//! technique applies: draw each item in a colour that encodes its id into a
//! small off-screen target on demand, read back one pixel (or a region for a
//! marquee) and map the value back. [`IdBuffer`] is the CPU reference for that
//! encoding; `benches/picking.rs` compares it with the spatial index. The
//! index wins at this phase's budgets (a 10 000-shape point pick is
//! microseconds; a GPU submit + `map_async` + readback is hundreds of
//! microseconds at best) and the id buffer wins when the scene is so dense that
//! one pass is cheaper than many indexed queries, or when the picks are
//! screen-space and the draw already exists. Use it on demand only, never once
//! per pointer move.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::too_many_lines
)] // reason: picking is bounded pixel/index arithmetic and one linear exact-test pass

pub mod camera;

use std::sync::Arc;

use glam::{Affine2, Vec2};

use ferrous_core::spatial::SpatialIndex;

pub use ferrous_core::spatial::Aabb;

pub use camera::PickCamera;

/// An application-defined id for a pickable item.
pub type PickId = u32;

/// Which part of a shape accepts a hit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillRule {
    /// Non-zero winding (the SVG default).
    NonZero,
    /// Even-odd.
    EvenOdd,
}

/// The cap of a stroked path for hit testing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrokeCap {
    /// The stroke stops exactly at the endpoint.
    Butt,
    /// A half-disc at the endpoint.
    Round,
}

/// A stroke that participates in picking.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PickStroke {
    /// Stroke width in local units.
    pub width: f32,
    /// The cap style.
    pub cap: StrokeCap,
}

impl PickStroke {
    /// A round-capped stroke.
    #[must_use]
    pub const fn new(width: f32) -> Self {
        Self {
            width,
            cap: StrokeCap::Round,
        }
    }
}

/// Optional alpha mask for images: a hit requires the sampled alpha to reach
/// `threshold` (0 = any pixel, 255 = fully opaque only).
#[derive(Clone, Debug, PartialEq)]
pub struct ImageAlpha {
    /// Row-major RGBA8 bytes.
    pub data: Arc<[u8]>,
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// Minimum alpha that counts as a hit.
    pub threshold: u8,
    /// The image's local rectangle (the mask is mapped onto it).
    pub bounds: Aabb,
}

/// The local shape of an item.
#[derive(Clone, Debug, PartialEq)]
pub enum PickShape {
    /// A rectangle centred on the origin.
    Rect {
        /// Full width and height.
        size: Vec2,
        /// Corner radius, clamped to half the smaller side.
        corner_radius: f32,
    },
    /// A circle centred on the origin.
    Circle {
        /// Radius in local units.
        radius: f32,
    },
    /// A polyline or polygon.
    Path {
        /// Points in local units.
        points: Vec<Vec2>,
        /// Whether the last point connects back to the first.
        closed: bool,
        /// Whether the interior is filled.
        filled: bool,
        /// The fill rule.
        fill_rule: FillRule,
        /// Whether the outline is stroked and accepts hits.
        stroke: Option<PickStroke>,
    },
    /// Text bounds in local units (the caller measured the run).
    Text {
        /// The local bounds of the shaped text.
        bounds: Aabb,
    },
    /// An image with optional alpha testing.
    Image {
        /// The local bounds of the image.
        bounds: Aabb,
        /// The optional alpha mask.
        alpha: Option<ImageAlpha>,
    },
}

impl PickShape {
    /// A rectangle centred on the origin.
    #[must_use]
    pub fn rect(size: Vec2) -> Self {
        Self::Rect {
            size,
            corner_radius: 0.0,
        }
    }

    /// A rounded rectangle centred on the origin.
    #[must_use]
    pub fn rounded_rect(size: Vec2, corner_radius: f32) -> Self {
        Self::Rect {
            size,
            corner_radius,
        }
    }

    /// A circle centred on the origin.
    #[must_use]
    pub fn circle(radius: f32) -> Self {
        Self::Circle { radius }
    }

    /// A filled polygon in local units.
    #[must_use]
    pub fn polygon(points: Vec<Vec2>) -> Self {
        Self::Path {
            points,
            closed: true,
            filled: true,
            fill_rule: FillRule::NonZero,
            stroke: None,
        }
    }

    /// A stroked polyline in local units.
    #[must_use]
    pub fn polyline(points: Vec<Vec2>, width: f32) -> Self {
        Self::Path {
            points,
            closed: false,
            filled: false,
            fill_rule: FillRule::NonZero,
            stroke: Some(PickStroke::new(width)),
        }
    }

    /// The local bounding box, or `None` for an empty shape.
    #[must_use]
    pub fn local_bounds(&self) -> Option<Aabb> {
        match self {
            Self::Rect { size, .. } => (*size != Vec2::ZERO).then(|| Aabb {
                min: -*size * 0.5,
                max: *size * 0.5,
            }),
            Self::Circle { radius } => (*radius > 0.0).then(|| Aabb {
                min: Vec2::splat(-*radius),
                max: Vec2::splat(*radius),
            }),
            Self::Path { points, stroke, .. } => {
                if points.len() < 2 && stroke.is_none() {
                    return None;
                }
                let bounds = Aabb::from_points(points)
                    .expand(stroke.map_or(0.0, |stroke| stroke.width * 0.5));
                (!bounds.is_empty()).then_some(bounds)
            }
            Self::Text { bounds } | Self::Image { bounds, .. } => {
                (!bounds.is_empty()).then_some(*bounds)
            }
        }
    }
}

/// One clip rectangle in world space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipNode {
    /// The clip rectangle in the node's local space.
    pub rect: Aabb,
    /// The clip's world transform.
    pub transform: Affine2,
    /// The parent clip, when these are nested.
    pub parent: Option<ClipId>,
    /// A disabled clip is ignored.
    pub enabled: bool,
}

/// An index into a scene's clip chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ClipId(pub u32);

/// One pickable item.
#[derive(Clone, Debug, PartialEq)]
pub struct PickItem {
    /// The caller's id for this item (selection target).
    pub id: PickId,
    /// The local-to-world transform (rotation, scale and skew included).
    pub transform: Affine2,
    /// The shape in local space.
    pub shape: PickShape,
    /// Draw order within the scene; larger is on top.
    pub z: i32,
    /// The innermost clip that contains the item.
    pub clip: Option<ClipId>,
    /// Opacity; `0.0` is not pickable.
    pub opacity: f32,
    /// Whether the item is drawn at all.
    pub visible: bool,
    /// Whether the item accepts hits (locked layers set this to `false`).
    pub pickable: bool,
}

impl PickItem {
    /// An item with default flags (visible, pickable, opaque).
    #[must_use]
    pub fn new(id: PickId, shape: PickShape) -> Self {
        Self {
            id,
            transform: Affine2::IDENTITY,
            shape,
            z: 0,
            clip: None,
            opacity: 1.0,
            visible: true,
            pickable: true,
        }
    }

    /// Sets the local-to-world transform.
    #[must_use]
    pub fn transform(mut self, transform: Affine2) -> Self {
        self.transform = transform;
        self
    }

    /// Sets the z index.
    #[must_use]
    pub fn z(mut self, z: i32) -> Self {
        self.z = z;
        self
    }

    /// Sets the clip.
    #[must_use]
    pub fn clip(mut self, clip: Option<ClipId>) -> Self {
        self.clip = clip;
        self
    }

    /// Marks a locked item as not pickable.
    #[must_use]
    pub fn pickable(mut self, pickable: bool) -> Self {
        self.pickable = pickable;
        self
    }

    /// The item's world-space bounds, including its stroke.
    #[must_use]
    pub fn world_bounds(&self) -> Option<Aabb> {
        let local = self.shape.local_bounds()?;
        let points = transformed_corners(local, self.transform);
        let bounds = Aabb::from_points(&points);
        (!bounds.is_empty()).then_some(bounds)
    }
}

/// Whether a marquee must intersect or fully contain a shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarqueeMode {
    /// Any overlap counts.
    Intersect,
    /// The whole shape must lie inside the marquee.
    Contain,
}

/// A successful pick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PickHit {
    /// The application id.
    pub id: PickId,
    /// The index in [`PickScene::items`].
    pub item: usize,
}

/// The nearest point or edge result.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NearestHit {
    /// The application id.
    pub id: PickId,
    /// The index in [`PickScene::items`].
    pub item: usize,
    /// The closest point in world space.
    pub point: Vec2,
    /// Distance from the query point to `point`.
    pub distance: f32,
    /// The edge index for `nearest_edge` (ignored by `nearest_point`).
    pub edge: Option<usize>,
}

/// The pick scene: items, clips and the shared spatial index.
#[derive(Default)]
pub struct PickScene {
    items: Vec<PickItem>,
    clips: Vec<ClipNode>,
    order: Vec<usize>,
    order_of: Vec<u32>,
    entries: Vec<(u32, Aabb)>,
    index: SpatialIndex,
    candidates: Vec<u32>,
    dirty: bool,
    rebuilds: u64,
}

impl PickScene {
    /// Builds an empty scene.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an item and returns its index.
    pub fn add(&mut self, item: PickItem) -> usize {
        self.dirty = true;
        self.items.push(item);
        self.items.len() - 1
    }

    /// Adds a clip node and returns its id.
    pub fn add_clip(&mut self, clip: ClipNode) -> ClipId {
        let id = ClipId(self.clips.len() as u32);
        self.clips.push(clip);
        id
    }

    /// Removes every item and clip.
    pub fn clear(&mut self) {
        self.items.clear();
        self.clips.clear();
        self.order.clear();
        self.order_of.clear();
        self.entries.clear();
        self.index.rebuild(&[]);
        self.dirty = false;
    }

    /// Marks the geometry dirty; the next query rebuilds the index.
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// The items, in insertion order.
    #[must_use]
    pub fn items(&self) -> &[PickItem] {
        &self.items
    }

    /// The items, mutably (an app animates transforms between frames).
    pub fn items_mut(&mut self) -> &mut [PickItem] {
        self.dirty = true;
        &mut self.items
    }

    /// The clips.
    #[must_use]
    pub fn clips(&self) -> &[ClipNode] {
        &self.clips
    }

    /// Rebuilds the index in `(z, insertion)` order, so the last entry is the
    /// topmost. `O(n log n + covered cells)`; not per query.
    pub fn rebuild(&mut self) {
        self.dirty = false;
        self.rebuilds += 1;
        self.order.clear();
        self.order.extend(0..self.items.len());
        self.order
            .sort_by_key(|index| (self.items[*index].z, *index));
        self.order_of.clear();
        self.order_of.resize(self.items.len(), 0);
        for (position, &item) in self.order.iter().enumerate() {
            self.order_of[item] = position as u32;
        }
        self.entries.clear();
        self.entries.reserve(self.order.len());
        for &index in &self.order {
            let item = &self.items[index];
            if !item.visible || item.opacity <= 0.0 {
                continue;
            }
            let Some(bounds) = item.world_bounds() else {
                continue;
            };
            let bounds = self.clipped_bounds(item, bounds);
            if bounds.is_empty() {
                continue;
            }
            self.entries.push((index as u32, bounds));
        }
        self.index.rebuild(&self.entries);
    }

    /// The number of index rebuilds.
    #[must_use]
    pub fn rebuilds(&self) -> u64 {
        self.rebuilds
    }

    /// The bounds the index stores for an item.
    #[must_use]
    pub fn item_bounds(&self, item: usize) -> Option<Aabb> {
        self.index.bounds(item as u32)
    }

    /// The topmost item under `point`.
    ///
    /// `tolerance` is in screen pixels and widens stroke and text hits; fills
    /// are exact. It is converted to local units by each item's transform.
    pub fn pick(&mut self, point: Vec2, tolerance: f32) -> Option<PickHit> {
        self.ensure_index();
        for index in self.index.query_point(point) {
            let item_index = index as usize;
            let item = self.items.get(item_index)?;
            if !item.pickable || !self.point_inside(item, point, tolerance) {
                continue;
            }
            return Some(PickHit {
                id: item.id,
                item: item_index,
            });
        }
        None
    }

    /// Every item under `point`, topmost first, into a reused buffer.
    pub fn pick_all(&mut self, point: Vec2, tolerance: f32, out: &mut Vec<PickHit>) {
        out.clear();
        self.ensure_index();
        self.candidates.clear();
        self.candidates.extend(self.index.query_point(point));
        for &index in &self.candidates {
            let item_index = index as usize;
            let Some(item) = self.items.get(item_index) else {
                continue;
            };
            if item.pickable && self.point_inside(item, point, tolerance) {
                out.push(PickHit {
                    id: item.id,
                    item: item_index,
                });
            }
        }
    }

    /// Items that intersect or are contained by an axis-aligned marquee.
    pub fn marquee(
        &mut self,
        area: Aabb,
        mode: MarqueeMode,
        tolerance: f32,
        out: &mut Vec<PickHit>,
    ) {
        out.clear();
        self.ensure_index();
        let area = area.expand(tolerance);
        self.candidates.clear();
        self.index.query_rect_into(area, &mut self.candidates);
        if self.candidates.len() > 1 {
            let order_of = &self.order_of;
            self.candidates.sort_unstable_by_key(|index| {
                std::cmp::Reverse(order_of.get(*index as usize).copied().unwrap_or(0))
            });
        }
        for &index in &self.candidates {
            let item_index = index as usize;
            let Some(item) = self.items.get(item_index) else {
                continue;
            };
            if item.pickable && self.item_in_area(item, area, mode) {
                out.push(PickHit {
                    id: item.id,
                    item: item_index,
                });
            }
        }
    }

    /// Items that intersect or are contained by a lasso polygon.
    pub fn lasso(
        &mut self,
        polygon: &[Vec2],
        mode: MarqueeMode,
        tolerance: f32,
        out: &mut Vec<PickHit>,
    ) {
        if polygon.len() < 3 {
            out.clear();
            return;
        }
        let bounds = Aabb::from_points(polygon).expand(tolerance);
        out.clear();
        self.ensure_index();
        self.candidates.clear();
        self.index.query_rect_into(bounds, &mut self.candidates);
        if self.candidates.len() > 1 {
            let order_of = &self.order_of;
            self.candidates.sort_unstable_by_key(|index| {
                std::cmp::Reverse(order_of.get(*index as usize).copied().unwrap_or(0))
            });
        }
        for &index in &self.candidates {
            let item_index = index as usize;
            let Some(item) = self.items.get(item_index) else {
                continue;
            };
            if item.pickable && self.item_in_polygon(item, polygon, mode) {
                out.push(PickHit {
                    id: item.id,
                    item: item_index,
                });
            }
        }
    }

    /// The nearest point on any pickable shape within `radius` world units.
    pub fn nearest_point(&mut self, point: Vec2, radius: f32) -> Option<NearestHit> {
        self.nearest(point, radius, false)
    }

    /// The nearest edge on any pickable shape within `radius` world units.
    pub fn nearest_edge(&mut self, point: Vec2, radius: f32) -> Option<NearestHit> {
        self.nearest(point, radius, true)
    }

    fn nearest(&mut self, point: Vec2, radius: f32, want_edge: bool) -> Option<NearestHit> {
        self.ensure_index();
        let area = Aabb::from_origin_size(point - Vec2::splat(radius), Vec2::splat(radius * 2.0));
        self.candidates.clear();
        self.index.query_rect_into(area, &mut self.candidates);
        let mut best: Option<NearestHit> = None;
        for &index in &self.candidates {
            let item_index = index as usize;
            let Some(item) = self.items.get(item_index) else {
                continue;
            };
            if !item.pickable || !self.visible_through_clips(item, point) {
                continue;
            }
            let Some(candidate) = nearest_on_item(item, item_index, point, want_edge) else {
                continue;
            };
            if candidate.distance > radius {
                continue;
            }
            if best.is_none_or(|current| candidate.distance < current.distance) {
                best = Some(candidate);
            }
        }
        best
    }

    fn ensure_index(&mut self) {
        if self.dirty || self.index.rebuilds() == 0 {
            self.rebuild();
        }
    }

    fn point_inside(&self, item: &PickItem, point: Vec2, tolerance: f32) -> bool {
        let Some(inverse) = inverse(item.transform) else {
            return false;
        };
        let local = inverse.transform_point2(point);
        shape_contains(&item.shape, local, tolerance * local_scale(item.transform))
            && self.visible_through_clips(item, point)
    }

    fn visible_through_clips(&self, item: &PickItem, world_point: Vec2) -> bool {
        let mut clip = item.clip;
        while let Some(id) = clip {
            let Some(node) = self.clips.get(id.0 as usize) else {
                return false;
            };
            if node.enabled {
                let Some(inverse) = inverse(node.transform) else {
                    return false;
                };
                if !node.rect.contains(inverse.transform_point2(world_point)) {
                    return false;
                }
            }
            clip = node.parent;
        }
        true
    }

    fn clip_bounds(&self, item: &PickItem) -> Option<Aabb> {
        let mut bounds: Option<Aabb> = None;
        let mut clip = item.clip;
        while let Some(id) = clip {
            let node = self.clips.get(id.0 as usize)?;
            if node.enabled {
                let boxed = Aabb::from_points(&transformed_corners(node.rect, node.transform));
                bounds = Some(bounds.map_or(boxed, |current| current.intersect(boxed)));
            }
            clip = node.parent;
        }
        bounds
    }

    fn clipped_bounds(&self, item: &PickItem, bounds: Aabb) -> Aabb {
        self.clip_bounds(item)
            .map_or(bounds, |clip| bounds.intersect(clip))
    }

    fn item_in_area(&self, item: &PickItem, area: Aabb, mode: MarqueeMode) -> bool {
        let Some(world_bounds) = item.world_bounds() else {
            return false;
        };
        let bounds = self.clipped_bounds(item, world_bounds);
        if bounds.is_empty() {
            return false;
        }
        match mode {
            MarqueeMode::Intersect => {
                if !bounds.intersects(area) {
                    return false;
                }
                if area.contains(bounds.min) && area.contains(bounds.max) {
                    return true;
                }
                shape_any_point_in_area(item, area)
                    || shape_edges_intersect_area(item, area)
                    || shape_area_corners_inside(item, area)
            }
            MarqueeMode::Contain => {
                shape_all_points_in_area(item, area)
                    && bounds.min.x >= area.min.x
                    && bounds.min.y >= area.min.y
                    && bounds.max.x <= area.max.x
                    && bounds.max.y <= area.max.y
            }
        }
    }

    fn item_in_polygon(&self, item: &PickItem, polygon: &[Vec2], mode: MarqueeMode) -> bool {
        match mode {
            MarqueeMode::Intersect => {
                shape_any_point_in_polygon(item, polygon)
                    || polygon
                        .iter()
                        .any(|point| self.point_inside(item, *point, 0.0))
                    || shape_edges_intersect_polygon(item, polygon)
            }
            MarqueeMode::Contain => shape_all_points_in_polygon(item, polygon),
        }
    }
}

fn inverse(transform: Affine2) -> Option<Affine2> {
    let determinant = transform.matrix2.determinant();
    if !determinant.is_finite() || determinant.abs() < 1e-12 {
        return None;
    }
    let inverse = transform.inverse();
    inverse.is_finite().then_some(inverse)
}

fn local_scale(transform: Affine2) -> f32 {
    let determinant = transform.matrix2.determinant().abs();
    if determinant <= f32::EPSILON {
        return 1.0;
    }
    1.0 / determinant.sqrt()
}

fn transformed_corners(rect: Aabb, transform: Affine2) -> [Vec2; 4] {
    [
        transform.transform_point2(Vec2::new(rect.min.x, rect.min.y)),
        transform.transform_point2(Vec2::new(rect.max.x, rect.min.y)),
        transform.transform_point2(Vec2::new(rect.max.x, rect.max.y)),
        transform.transform_point2(Vec2::new(rect.min.x, rect.max.y)),
    ]
}

fn outline_len(shape: &PickShape) -> usize {
    match shape {
        PickShape::Path { points, .. } => points.len(),
        _ => 4,
    }
}

fn local_outline_point(shape: &PickShape, index: usize) -> Vec2 {
    match shape {
        PickShape::Rect { size, .. } => {
            let half = *size * 0.5;
            match index % 4 {
                0 => Vec2::new(-half.x, -half.y),
                1 => Vec2::new(half.x, -half.y),
                2 => Vec2::new(half.x, half.y),
                _ => Vec2::new(-half.x, half.y),
            }
        }
        PickShape::Circle { radius } => {
            let radius = *radius;
            match index % 4 {
                0 => Vec2::new(-radius, -radius),
                1 => Vec2::new(radius, -radius),
                2 => Vec2::new(radius, radius),
                _ => Vec2::new(-radius, radius),
            }
        }
        PickShape::Path { points, .. } => {
            if points.is_empty() {
                Vec2::ZERO
            } else {
                points[index % points.len()]
            }
        }
        PickShape::Text { bounds } | PickShape::Image { bounds, .. } => match index % 4 {
            0 => Vec2::new(bounds.min.x, bounds.min.y),
            1 => Vec2::new(bounds.max.x, bounds.min.y),
            2 => Vec2::new(bounds.max.x, bounds.max.y),
            _ => Vec2::new(bounds.min.x, bounds.max.y),
        },
    }
}

fn world_outline_point(item: &PickItem, index: usize) -> Vec2 {
    item.transform
        .transform_point2(local_outline_point(&item.shape, index))
}

fn outline_closed(shape: &PickShape) -> bool {
    match shape {
        PickShape::Path { closed, .. } => *closed,
        _ => true,
    }
}

fn for_each_edge(shape: &PickShape, mut visit: impl FnMut(Vec2, Vec2) -> bool) -> bool {
    let len = outline_len(shape);
    if len < 2 {
        return false;
    }
    let count = if outline_closed(shape) { len } else { len - 1 };
    for index in 0..count {
        let start = local_outline_point(shape, index);
        let end = local_outline_point(shape, (index + 1) % len);
        if visit(start, end) {
            return true;
        }
    }
    false
}

fn shape_any_point_in_area(item: &PickItem, area: Aabb) -> bool {
    (0..outline_len(&item.shape)).any(|index| area.contains(world_outline_point(item, index)))
}

fn shape_all_points_in_area(item: &PickItem, area: Aabb) -> bool {
    let len = outline_len(&item.shape);
    len > 0 && (0..len).all(|index| area.contains(world_outline_point(item, index)))
}

fn shape_any_point_in_polygon(item: &PickItem, polygon: &[Vec2]) -> bool {
    (0..outline_len(&item.shape))
        .any(|index| point_in_polygon(world_outline_point(item, index), polygon))
}

fn shape_all_points_in_polygon(item: &PickItem, polygon: &[Vec2]) -> bool {
    let len = outline_len(&item.shape);
    len > 0 && (0..len).all(|index| point_in_polygon(world_outline_point(item, index), polygon))
}

fn shape_edges_intersect_area(item: &PickItem, area: Aabb) -> bool {
    let corners = [
        Vec2::new(area.min.x, area.min.y),
        Vec2::new(area.max.x, area.min.y),
        Vec2::new(area.max.x, area.max.y),
        Vec2::new(area.min.x, area.max.y),
    ];
    for_each_edge(&item.shape, |start, end| {
        let start = item.transform.transform_point2(start);
        let end = item.transform.transform_point2(end);
        (0..4).any(|corner| {
            segments_intersect(start, end, corners[corner], corners[(corner + 1) % 4])
        })
    })
}

fn shape_edges_intersect_polygon(item: &PickItem, polygon: &[Vec2]) -> bool {
    for_each_edge(&item.shape, |start, end| {
        let start = item.transform.transform_point2(start);
        let end = item.transform.transform_point2(end);
        (0..polygon.len()).any(|second| {
            segments_intersect(
                start,
                end,
                polygon[second],
                polygon[(second + 1) % polygon.len()],
            )
        })
    })
}

fn shape_area_corners_inside(item: &PickItem, area: Aabb) -> bool {
    if outline_len(&item.shape) < 3 || !outline_closed(&item.shape) {
        return false;
    }
    let Some(inverse) = inverse(item.transform) else {
        return false;
    };
    [
        Vec2::new(area.min.x, area.min.y),
        Vec2::new(area.max.x, area.min.y),
        Vec2::new(area.max.x, area.max.y),
        Vec2::new(area.min.x, area.max.y),
    ]
    .iter()
    .any(|corner| shape_contains(&item.shape, inverse.transform_point2(*corner), 0.0))
}

fn shape_contains(shape: &PickShape, point: Vec2, tolerance: f32) -> bool {
    match shape {
        PickShape::Rect {
            size,
            corner_radius,
        } => inside_rounded_rect(point, *size, *corner_radius, tolerance),
        PickShape::Circle { radius } => point.length_squared() <= (*radius + tolerance).powi(2),
        PickShape::Path {
            points,
            closed,
            filled,
            fill_rule,
            stroke,
        } => {
            let mut hit = *filled && *closed && point_in_polygon_rule(point, points, *fill_rule);
            if !hit {
                if let Some(stroke) = stroke {
                    let reach = stroke.width * 0.5 + tolerance;
                    hit = distance_to_polyline(point, points, *closed, stroke.cap) <= reach;
                }
            }
            hit
        }
        PickShape::Text { bounds } => bounds.expand(tolerance).contains(point),
        PickShape::Image { bounds, alpha } => {
            if !bounds.expand(tolerance).contains(point) {
                return false;
            }
            alpha
                .as_ref()
                .is_none_or(|alpha| sample_alpha(alpha, point))
        }
    }
}

fn inside_rounded_rect(point: Vec2, size: Vec2, radius: f32, tolerance: f32) -> bool {
    let half = size * 0.5;
    let radius = radius.min(half.x).min(half.y).max(0.0);
    let inner = Vec2::new((half.x - radius).max(0.0), (half.y - radius).max(0.0));
    let outer = half + Vec2::splat(tolerance);
    let local = point.abs();
    if local.x > outer.x || local.y > outer.y {
        return false;
    }
    if local.x <= inner.x + tolerance || local.y <= inner.y + tolerance {
        return true;
    }
    (local - inner).length() <= radius + tolerance
}

fn sample_alpha(mask: &ImageAlpha, point: Vec2) -> bool {
    if mask.width == 0 || mask.height == 0 {
        return false;
    }
    let size = mask.bounds.size();
    if size.x <= 0.0 || size.y <= 0.0 {
        return false;
    }
    let u = (point.x - mask.bounds.min.x) / size.x;
    let v = (point.y - mask.bounds.min.y) / size.y;
    if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
        return false;
    }
    let x = ((u * mask.width as f32) as u32).min(mask.width - 1);
    let y = ((v * mask.height as f32) as u32).min(mask.height - 1);
    let index = ((y * mask.width + x) * 4 + 3) as usize;
    mask.data
        .get(index)
        .is_some_and(|alpha| *alpha >= mask.threshold)
}

fn point_in_polygon_rule(point: Vec2, polygon: &[Vec2], rule: FillRule) -> bool {
    match rule {
        FillRule::EvenOdd => point_in_polygon(point, polygon),
        FillRule::NonZero => winding_number(point, polygon) != 0,
    }
}

fn point_in_polygon(point: Vec2, polygon: &[Vec2]) -> bool {
    if polygon.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut previous = polygon[polygon.len() - 1];
    for &current in polygon {
        if (current.y > point.y) != (previous.y > point.y) {
            let x = (previous.x - current.x) * (point.y - current.y) / (previous.y - current.y)
                + current.x;
            if point.x < x {
                inside = !inside;
            }
        }
        previous = current;
    }
    inside
}

fn winding_number(point: Vec2, polygon: &[Vec2]) -> i32 {
    if polygon.len() < 3 {
        return 0;
    }
    let mut winding = 0;
    let mut previous = polygon[polygon.len() - 1];
    for &current in polygon {
        if previous.y <= point.y {
            if current.y > point.y && cross(previous, current, point) > 0.0 {
                winding += 1;
            }
        } else if current.y <= point.y && cross(previous, current, point) < 0.0 {
            winding -= 1;
        }
        previous = current;
    }
    winding
}

fn cross(a: Vec2, b: Vec2, point: Vec2) -> f32 {
    (b.x - a.x) * (point.y - a.y) - (point.x - a.x) * (b.y - a.y)
}

fn distance_to_polyline(point: Vec2, points: &[Vec2], closed: bool, cap: StrokeCap) -> f32 {
    if points.is_empty() {
        return f32::INFINITY;
    }
    if points.len() == 1 {
        return point.distance(points[0]);
    }
    let mut best = f32::INFINITY;
    for window in points.windows(2) {
        best = best.min(distance_to_segment(point, window[0], window[1]));
    }
    if closed {
        best = best.min(distance_to_segment(
            point,
            points[points.len() - 1],
            points[0],
        ));
    } else if cap == StrokeCap::Round {
        best = best
            .min(point.distance(points[0]))
            .min(point.distance(points[points.len() - 1]));
    }
    best
}

fn distance_to_segment(point: Vec2, a: Vec2, b: Vec2) -> f32 {
    point.distance(closest_on_segment(point, a, b))
}

fn closest_on_segment(point: Vec2, a: Vec2, b: Vec2) -> Vec2 {
    let direction = b - a;
    let length_squared = direction.length_squared();
    if length_squared <= f32::EPSILON {
        return a;
    }
    let t = ((point - a).dot(direction) / length_squared).clamp(0.0, 1.0);
    a + direction * t
}

fn nearest_on_item(
    item: &PickItem,
    item_index: usize,
    point: Vec2,
    want_edge: bool,
) -> Option<NearestHit> {
    let inverse = inverse(item.transform)?;
    let local = inverse.transform_point2(point);
    let (local_point, edge) = match &item.shape {
        PickShape::Rect { size, .. } => {
            let half = *size * 0.5;
            let corners = [
                Vec2::new(-half.x, -half.y),
                Vec2::new(half.x, -half.y),
                Vec2::new(half.x, half.y),
                Vec2::new(-half.x, half.y),
            ];
            nearest_on_edges(local, &corners)
        }
        PickShape::Circle { radius } => {
            let length = local.length();
            let direction = if length > f32::EPSILON {
                local / length
            } else {
                Vec2::X
            };
            (direction * *radius, 0)
        }
        PickShape::Path {
            points,
            closed,
            stroke,
            ..
        } => {
            if points.len() < 2 {
                return None;
            }
            let closed = *closed || stroke.is_some_and(|stroke| stroke.cap == StrokeCap::Round);
            let edge_count = if closed {
                points.len()
            } else {
                points.len() - 1
            };
            let mut best = (local, 0usize);
            let mut best_distance = f32::INFINITY;
            for edge in 0..edge_count {
                let candidate =
                    closest_on_segment(local, points[edge], points[(edge + 1) % points.len()]);
                let distance = local.distance(candidate);
                if distance < best_distance {
                    best_distance = distance;
                    best = (candidate, edge);
                }
            }
            best
        }
        PickShape::Text { bounds } | PickShape::Image { bounds, .. } => {
            let corners = [
                Vec2::new(bounds.min.x, bounds.min.y),
                Vec2::new(bounds.max.x, bounds.min.y),
                Vec2::new(bounds.max.x, bounds.max.y),
                Vec2::new(bounds.min.x, bounds.max.y),
            ];
            nearest_on_edges(local, &corners)
        }
    };
    let world = item.transform.transform_point2(local_point);
    Some(NearestHit {
        id: item.id,
        item: item_index,
        point: world,
        distance: point.distance(world),
        edge: want_edge.then_some(edge),
    })
}

fn nearest_on_edges(point: Vec2, corners: &[Vec2; 4]) -> (Vec2, usize) {
    let mut best = (point, 0usize);
    let mut best_distance = f32::INFINITY;
    for edge in 0..4 {
        let candidate = closest_on_segment(point, corners[edge], corners[(edge + 1) % 4]);
        let distance = point.distance(candidate);
        if distance < best_distance {
            best_distance = distance;
            best = (candidate, edge);
        }
    }
    best
}

fn segments_intersect(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> bool {
    let denominator = (b.x - a.x) * (d.y - c.y) - (b.y - a.y) * (d.x - c.x);
    if denominator.abs() <= f32::EPSILON {
        return false;
    }
    let t = ((c.x - a.x) * (d.y - c.y) - (c.y - a.y) * (d.x - c.x)) / denominator;
    let u = ((c.x - a.x) * (b.y - a.y) - (c.y - a.y) * (b.x - a.x)) / denominator;
    (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)
}

/// The CPU reference for the GPU id-buffer encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IdBuffer;

impl IdBuffer {
    /// The opaque colour that encodes `id` (alpha is always 255).
    #[must_use]
    pub fn encode(id: PickId) -> [u8; 4] {
        [
            (id & 0xff) as u8,
            ((id >> 8) & 0xff) as u8,
            ((id >> 16) & 0xff) as u8,
            0xff,
        ]
    }

    /// Decodes a colour read back from the target; `None` for the clear value.
    #[must_use]
    pub fn decode(color: [u8; 4]) -> Option<PickId> {
        if color == [0, 0, 0, 0] {
            return None;
        }
        Some(u32::from(color[0]) | u32::from(color[1]) << 8 | u32::from(color[2]) << 16)
    }

    /// Rasterizes a 1:1 id buffer: the item index of the topmost hit per pixel.
    ///
    /// This is the reference the GPU pass must match. It allocates the output
    /// and runs one query per pixel, so it is a correctness/benchmark tool, not
    /// the routing path.
    #[must_use]
    pub fn rasterize(scene: &mut PickScene, width: u32, height: u32) -> Vec<u32> {
        let mut output = vec![u32::MAX; (width as usize) * (height as usize)];
        for y in 0..height {
            for x in 0..width {
                let point = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                if let Some(hit) = scene.pick(point, 0.0) {
                    output[(y as usize) * width as usize + x as usize] = hit.item as u32;
                }
            }
        }
        output
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)] // reason: exact affine values in tests

    use super::*;

    fn rotated(angle: f32, translation: Vec2) -> Affine2 {
        Affine2::from_angle_translation(angle, translation)
    }

    fn square(size: f32) -> PickShape {
        PickShape::rect(Vec2::splat(size))
    }

    #[test]
    fn a_point_picks_the_topmost_item() {
        let mut scene = PickScene::new();
        scene.add(PickItem::new(1, square(100.0)));
        scene.add(PickItem::new(2, square(20.0)).z(1));
        assert_eq!(scene.pick(Vec2::ZERO, 0.0).map(|hit| hit.id), Some(2));
        assert_eq!(
            scene.pick(Vec2::new(40.0, 0.0), 0.0).map(|hit| hit.id),
            Some(1)
        );
        assert!(scene.pick(Vec2::new(200.0, 0.0), 0.0).is_none());
    }

    #[test]
    fn rotation_is_respected() {
        let mut scene = PickScene::new();
        scene.add(
            PickItem::new(1, PickShape::rect(Vec2::new(100.0, 20.0)))
                .transform(rotated(std::f32::consts::FRAC_PI_2, Vec2::ZERO)),
        );
        assert!(scene.pick(Vec2::new(0.0, 40.0), 0.0).is_some());
        assert!(scene.pick(Vec2::new(40.0, 0.0), 0.0).is_none());
    }

    #[test]
    fn scale_and_skew_are_supported() {
        let mut scene = PickScene::new();
        let skew = Affine2::from_cols_array(&[1.0, 0.5, 0.0, 1.0, 0.0, 0.0]);
        scene.add(PickItem::new(1, square(10.0)).transform(skew));
        assert!(scene.pick(Vec2::new(0.0, -4.0), 0.0).is_some());
        assert!(scene.pick(Vec2::new(0.0, 6.0), 0.0).is_none());
    }

    #[test]
    fn circles_are_exact_not_their_box() {
        let mut scene = PickScene::new();
        scene.add(PickItem::new(1, PickShape::circle(10.0)));
        assert!(scene.pick(Vec2::new(6.0, 6.0), 0.0).is_some());
        assert!(scene.pick(Vec2::new(9.0, 9.0), 0.0).is_none());
    }

    #[test]
    fn rounded_rect_corners_cut_the_hit() {
        let mut scene = PickScene::new();
        scene.add(PickItem::new(
            1,
            PickShape::rounded_rect(Vec2::splat(100.0), 20.0),
        ));
        assert!(scene.pick(Vec2::new(45.0, 0.0), 0.0).is_some());
        assert!(scene.pick(Vec2::new(49.0, 49.0), 0.0).is_none());
    }

    #[test]
    fn filled_and_stroked_paths() {
        let square = vec![
            Vec2::new(-10.0, -10.0),
            Vec2::new(10.0, -10.0),
            Vec2::new(10.0, 10.0),
            Vec2::new(-10.0, 10.0),
        ];
        let mut scene = PickScene::new();
        scene.add(PickItem::new(1, PickShape::polygon(square)));
        scene.add(
            PickItem::new(
                2,
                PickShape::polyline(vec![Vec2::new(-40.0, 0.0), Vec2::new(40.0, 0.0)], 4.0),
            )
            .z(1),
        );
        assert_eq!(scene.pick(Vec2::ZERO, 0.0).map(|hit| hit.id), Some(2));
        assert_eq!(
            scene.pick(Vec2::new(0.0, 8.0), 0.0).map(|hit| hit.id),
            Some(1)
        );
        assert_eq!(
            scene.pick(Vec2::new(8.0, 8.0), 0.0).map(|hit| hit.id),
            Some(1)
        );
    }

    #[test]
    fn even_odd_holes_are_not_hit() {
        let mut points = vec![
            Vec2::new(-50.0, -50.0),
            Vec2::new(50.0, -50.0),
            Vec2::new(50.0, 50.0),
            Vec2::new(-50.0, 50.0),
        ];
        points.extend_from_slice(&[
            Vec2::new(-10.0, -10.0),
            Vec2::new(10.0, -10.0),
            Vec2::new(10.0, 10.0),
            Vec2::new(-10.0, 10.0),
        ]);
        let mut scene = PickScene::new();
        scene.add(PickItem::new(
            1,
            PickShape::Path {
                points,
                closed: true,
                filled: true,
                fill_rule: FillRule::EvenOdd,
                stroke: None,
            },
        ));
        assert!(scene.pick(Vec2::ZERO, 0.0).is_none());
        assert!(scene.pick(Vec2::new(30.0, 0.0), 0.0).is_some());
    }

    #[test]
    fn text_and_image_bounds_and_alpha() {
        let mut data = vec![0u8; 2 * 2 * 4];
        data[3 + 12] = 255; // bottom-right pixel's alpha
        let alpha = ImageAlpha {
            data: Arc::from(data),
            width: 2,
            height: 2,
            threshold: 128,
            bounds: Aabb {
                min: Vec2::new(-10.0, -10.0),
                max: Vec2::new(10.0, 10.0),
            },
        };
        let mut scene = PickScene::new();
        scene.add(PickItem::new(
            1,
            PickShape::Text {
                bounds: Aabb {
                    min: Vec2::new(-5.0, -5.0),
                    max: Vec2::new(5.0, 5.0),
                },
            },
        ));
        scene.add(
            PickItem::new(
                2,
                PickShape::Image {
                    bounds: alpha.bounds,
                    alpha: Some(alpha),
                },
            )
            .z(-1),
        );
        assert_eq!(
            scene.pick(Vec2::new(2.0, 2.0), 0.0).map(|hit| hit.id),
            Some(1)
        );
        assert_eq!(
            scene.pick(Vec2::new(6.0, 6.0), 0.0).map(|hit| hit.id),
            Some(2)
        );
        assert!(scene.pick(Vec2::new(-6.0, -6.0), 0.0).is_none());
    }

    #[test]
    fn nested_clips_cut_the_item() {
        let mut scene = PickScene::new();
        let outer = scene.add_clip(ClipNode {
            rect: Aabb {
                min: Vec2::ZERO,
                max: Vec2::new(100.0, 100.0),
            },
            transform: Affine2::IDENTITY,
            parent: None,
            enabled: true,
        });
        let inner = scene.add_clip(ClipNode {
            rect: Aabb {
                min: Vec2::ZERO,
                max: Vec2::new(20.0, 20.0),
            },
            transform: rotated(0.0, Vec2::new(10.0, 10.0)),
            parent: Some(outer),
            enabled: true,
        });
        scene.add(PickItem::new(1, square(300.0)).clip(Some(inner)));
        assert!(scene.pick(Vec2::new(15.0, 15.0), 0.0).is_some());
        assert!(scene.pick(Vec2::new(40.0, 15.0), 0.0).is_none());
        assert!(scene.pick(Vec2::new(5.0, 5.0), 0.0).is_none());
    }

    #[test]
    fn marquee_intersect_and_contain() {
        let mut scene = PickScene::new();
        scene.add(PickItem::new(1, square(10.0)));
        scene.add(PickItem::new(2, square(100.0)).z(1));
        let mut out = Vec::new();
        let area = Aabb {
            min: Vec2::new(-6.0, -6.0),
            max: Vec2::new(6.0, 6.0),
        };
        scene.marquee(area, MarqueeMode::Intersect, 0.0, &mut out);
        assert_eq!(out.iter().map(|hit| hit.id).collect::<Vec<_>>(), vec![2, 1]);
        scene.marquee(area, MarqueeMode::Contain, 0.0, &mut out);
        assert_eq!(out.iter().map(|hit| hit.id).collect::<Vec<_>>(), vec![1]);
    }

    #[test]
    fn marquee_respects_rotation() {
        let mut scene = PickScene::new();
        scene.add(
            PickItem::new(1, PickShape::rect(Vec2::new(100.0, 4.0)))
                .transform(rotated(std::f32::consts::FRAC_PI_2, Vec2::ZERO)),
        );
        let area = Aabb {
            min: Vec2::new(-10.0, 40.0),
            max: Vec2::new(10.0, 60.0),
        };
        let mut out = Vec::new();
        scene.marquee(area, MarqueeMode::Intersect, 0.0, &mut out);
        assert_eq!(out.len(), 1);
        scene.marquee(area, MarqueeMode::Contain, 0.0, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn lasso_selects_and_nearest_snaps() {
        let mut scene = PickScene::new();
        scene.add(PickItem::new(1, square(20.0)));
        let lasso = vec![
            Vec2::new(-30.0, -30.0),
            Vec2::new(30.0, -30.0),
            Vec2::new(30.0, 30.0),
            Vec2::new(-30.0, 30.0),
        ];
        let mut out = Vec::new();
        scene.lasso(&lasso, MarqueeMode::Intersect, 0.0, &mut out);
        assert_eq!(out.len(), 1);
        let nearest = scene.nearest_point(Vec2::new(20.0, 0.0), 10.0).unwrap();
        assert_eq!(nearest.id, 1);
        assert!((nearest.distance - 10.0).abs() < 1e-4);
        let edge = scene.nearest_edge(Vec2::new(20.0, 0.0), 10.0).unwrap();
        assert_eq!(edge.id, 1);
        assert!(edge.edge.is_some());
        assert!(scene.nearest_point(Vec2::new(100.0, 0.0), 5.0).is_none());
    }

    #[test]
    fn invisible_locked_and_transparent_items_are_not_pickable() {
        let mut scene = PickScene::new();
        let mut hidden = PickItem::new(1, square(100.0));
        hidden.visible = false;
        scene.add(hidden);
        scene.add(PickItem::new(2, square(100.0)).pickable(false));
        let mut transparent = PickItem::new(3, square(100.0));
        transparent.opacity = 0.0;
        scene.add(transparent);
        assert!(scene.pick(Vec2::ZERO, 0.0).is_none());
    }

    #[test]
    fn the_index_is_rebuilt_only_when_dirty() {
        let mut scene = PickScene::new();
        scene.add(PickItem::new(1, square(10.0)));
        assert!(scene.pick(Vec2::ZERO, 0.0).is_some());
        let rebuilds = scene.rebuilds();
        assert!(scene.pick(Vec2::ZERO, 0.0).is_some());
        assert_eq!(scene.rebuilds(), rebuilds);
        scene.items_mut()[0].transform = rotated(0.0, Vec2::new(100.0, 0.0));
        assert!(scene.pick(Vec2::new(100.0, 0.0), 0.0).is_some());
        assert_eq!(scene.rebuilds(), rebuilds + 1);
    }

    #[test]
    fn the_id_buffer_round_trips_and_matches_the_cpu_pick() {
        let mut scene = PickScene::new();
        let at = Affine2::from_translation(Vec2::new(10.0, 10.0));
        scene.add(PickItem::new(1, square(16.0)).transform(at));
        scene.add(PickItem::new(2, square(4.0)).transform(at).z(1));
        assert_eq!(IdBuffer::decode(IdBuffer::encode(7)), Some(7));
        assert_eq!(IdBuffer::decode([0, 0, 0, 0]), None);
        let buffer = IdBuffer::rasterize(&mut scene, 20, 20);
        assert_eq!(buffer[10 * 20 + 10], 1);
        assert_eq!(buffer[0], u32::MAX);
    }
}
