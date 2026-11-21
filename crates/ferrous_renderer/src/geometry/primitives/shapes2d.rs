//! Vectorial 2D shapes tessellated into 3D meshes using `lyon_tessellation`.
//!
//! Philosophy: "2D in Scalar is flat geometry in 3D world space."
//! Every vertex is placed in the XY plane with `z = 0.0` and `normal = [0, 0, 1]`.
//! This means the shapes:
//!   - Share the Z-buffer with 3D objects
//!   - Rotate with the camera
//!   - Can project shadows (via the `ShadowCaster` marker component)
//!   - Work with `set_position`, `set_color`, etc. without any special casing
//!
//! The rendered color comes from `MaterialComponent::descriptor::base_color`.
//! For pure "unlit" / solid-color rendering, assign a `Material` with
//! `style_override: Some(RenderStyle::FlatShaded)` — this disables PBR
//! lighting while still allowing the entity to participate in the shadow pass.
//!
//! ## Caching model
//!
//! [`Shape2dCache`] splits tessellation from upload:
//!
//! * **Geometry cache** — keyed by a 128-bit hash of the geometry inputs only
//!   (path points/commands, radius/width/height, fill/stroke *presence and
//!   width*, cap and join).  Colour, transform and entity id are deliberately
//!   **not** part of the key: a colour change reuses the tessellation, and a
//!   transform-only change reuses it too (the model matrix is uploaded as
//!   per-instance data by `frame_builder`, never baked into vertices).
//! * **Mesh cache** — keyed by `(geometry key, colour key)`, so an unchanged
//!   scene reuses the same GPU buffers frame after frame.  Colour still needs
//!   a separate entry because the flat shader multiplies the vertex colour
//!   (`assets/shaders/flat.wgsl`: `material.base_color * in.color`); moving
//!   colour to instance data is a cross-lane shader/vertex-format change.
//!
//! Both maps are bounded LRUs (see [`GEOMETRY_CACHE_CAPACITY`] and
//! [`MESH_CACHE_CAPACITY`]), so a scene cannot grow the cache without limit.
//! Evicting only drops the cache's `Arc`; geometry already referenced by a
//! draw command stays alive.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::Arc;

use lyon_tessellation::{
    math::{point, Box2D},
    BuffersBuilder, FillOptions, FillTessellator, FillVertex, StrokeOptions, StrokeTessellator,
    StrokeVertex, VertexBuffers,
};

use crate::geometry::{Mesh, Vertex};
use crate::resources::buffer;

// ── Internal vertices ──────────────────────────────────────────────────────

/// Adapter: maps a lyon fill vertex to our `Vertex` type.
///
/// The cached geometry always carries opaque white; the requested colour is
/// written when the GPU mesh is built (`recolour`), so the tessellation itself
/// does not depend on colour.
struct FillCtor {
    color: [f32; 4],
}

impl lyon_tessellation::FillVertexConstructor<Vertex> for FillCtor {
    fn new_vertex(&mut self, v: FillVertex) -> Vertex {
        let p = v.position();
        Vertex {
            position: [p.x, p.y, 0.0],
            normal: [0.0, 0.0, 1.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            color: self.color,
            uv: [(p.x + 1.0) * 0.5, (p.y + 1.0) * 0.5],
        }
    }
}

/// Adapter: maps a lyon stroke vertex to our `Vertex` type.
struct StrokeCtor {
    color: [f32; 4],
}

impl lyon_tessellation::StrokeVertexConstructor<Vertex> for StrokeCtor {
    fn new_vertex(&mut self, v: StrokeVertex) -> Vertex {
        let p = v.position();
        Vertex {
            position: [p.x, p.y, 0.0],
            normal: [0.0, 0.0, 1.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            color: self.color,
            uv: [p.x, p.y],
        }
    }
}

/// Colour written into the cached geometry.  Any value would do as long as it
/// is constant; white is the natural neutral because the mesh path overwrites
/// it with the caller's colour before upload.
const NEUTRAL: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Engine numeric line cap (0 = Flat/Butt, 1 = Round, 2 = Square) → lyon `LineCap`.
pub fn into_line_cap(style: u8) -> lyon_tessellation::LineCap {
    match style {
        1 => lyon_tessellation::LineCap::Round,
        2 => lyon_tessellation::LineCap::Square,
        _ => lyon_tessellation::LineCap::Butt,
    }
}

/// Engine numeric line join (0 = Miter, 1 = Bevel, 2 = Round) → lyon `LineJoin`.
pub fn into_line_join(style: u8) -> lyon_tessellation::LineJoin {
    match style {
        1 => lyon_tessellation::LineJoin::Bevel,
        2 => lyon_tessellation::LineJoin::Round,
        _ => lyon_tessellation::LineJoin::Miter,
    }
}

/// Stable numeric encoding of a lyon cap, used in cache keys.  `LineCap` is a
/// foreign, non-`Hash` enum, so the key stores this instead.
fn cap_code(cap: lyon_tessellation::LineCap) -> u8 {
    match cap {
        lyon_tessellation::LineCap::Butt => 0,
        lyon_tessellation::LineCap::Round => 1,
        lyon_tessellation::LineCap::Square => 2,
    }
}

/// Stable numeric encoding of a lyon join, used in cache keys.
fn join_code(join: lyon_tessellation::LineJoin) -> u8 {
    match join {
        lyon_tessellation::LineJoin::Miter => 0,
        lyon_tessellation::LineJoin::Round => 1,
        lyon_tessellation::LineJoin::Bevel => 2,
        lyon_tessellation::LineJoin::MiterClip => 3,
    }
}

/// Clamp applied by [`path_to_mesh`] before stroking.  Factored out so the
/// cache key hashes exactly the width the tessellator will see.
fn path_stroke_width(stroke_thickness: f32) -> f32 {
    if stroke_thickness > 0.0 {
        stroke_thickness.max(1e-3)
    } else {
        0.0
    }
}

/// Clamp applied by [`line_2d`] before stroking.
fn line_stroke_width(thickness: f32) -> f32 {
    thickness.max(1e-4)
}

/// Convert a `VertexBuffers<Vertex, u32>` to a GPU `Mesh`.
fn buffers_to_mesh(
    device: &wgpu::Device,
    label: &str,
    buffers: VertexBuffers<Vertex, u32>,
) -> Mesh {
    let indices = buffers.indices;
    let vertices = buffers.vertices;

    if indices.is_empty() || vertices.is_empty() {
        return Mesh::empty(device);
    }

    // Calculate AABB from vertices
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    for v in &vertices {
        for i in 0..3 {
            min[i] = min[i].min(v.position[i]);
            max[i] = max[i].max(v.position[i]);
        }
    }

    Mesh {
        vertex_buffer: buffer::create_vertex(device, &format!("{label} VB"), &vertices),
        index_buffer: buffer::create_index(device, &format!("{label} IB"), &indices),
        index_count: indices.len() as u32,
        vertex_count: vertices.len() as u32,
        index_format: wgpu::IndexFormat::Uint32,
        aabb: crate::scene::culling::Aabb::new(glam::Vec3::from(min), glam::Vec3::from(max)),
    }
}

// ── Cache keys ──────────────────────────────────────────────────────────────

/// Bumped whenever the key derivation changes, so a hot-reloaded renderer
/// cannot reuse a tessellation produced by an older key scheme.
const CACHE_KEY_VERSION: u8 = 1;

/// 128-bit FNV-1a accumulator for geometry / colour keys.
///
/// A cache key only needs to be stable and cheap; 128 bits keeps the
/// collision probability of content hashing negligible (the same reasoning
/// [`ferrous_font::FontKey`] uses).
struct KeyHash(u128);

const FNV128_OFFSET: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
const FNV128_PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;

impl KeyHash {
    fn new(tag: u8) -> Self {
        let mut hash = Self(FNV128_OFFSET);
        hash.byte(tag);
        hash.byte(CACHE_KEY_VERSION);
        hash
    }

    fn byte(&mut self, byte: u8) {
        self.0 ^= u128::from(byte);
        self.0 = self.0.wrapping_mul(FNV128_PRIME);
    }

    fn u32(&mut self, value: u32) {
        for byte in value.to_le_bytes() {
            self.byte(byte);
        }
    }

    fn f32(&mut self, value: f32) {
        self.u32(value.to_bits());
    }

    fn colour(&mut self, colour: Option<[f32; 4]>) {
        match colour {
            Some(c) => {
                self.byte(1);
                for channel in c {
                    self.f32(channel);
                }
            }
            None => self.byte(0),
        }
    }

    fn finish(self) -> u128 {
        self.0
    }
}

/// Key of the colour-independent tessellation for a [`PathData`].
fn path_geometry_key(
    path: &PathData,
    do_fill: bool,
    stroke_thickness: f32,
    cap: lyon_tessellation::LineCap,
    join: lyon_tessellation::LineJoin,
) -> u128 {
    let mut hash = KeyHash::new(3);
    hash.u32(path.commands.len() as u32);
    for command in &path.commands {
        match command {
            PathCommand::MoveTo(p) => {
                hash.byte(0);
                hash.f32(p.x);
                hash.f32(p.y);
            }
            PathCommand::LineTo(p) => {
                hash.byte(1);
                hash.f32(p.x);
                hash.f32(p.y);
            }
            PathCommand::CubicTo(c1, c2, p) => {
                hash.byte(2);
                for point in [c1, c2, p] {
                    hash.f32(point.x);
                    hash.f32(point.y);
                }
            }
            PathCommand::Close => hash.byte(3),
        }
    }
    hash.byte(u8::from(do_fill));
    hash.f32(path_stroke_width(stroke_thickness));
    hash.byte(cap_code(cap));
    hash.byte(join_code(join));
    hash.finish()
}

/// Key of the geometry-only tessellation for a circle.
fn circle_geometry_key(radius: f32, resolution: u32, do_fill: bool, thickness: f32) -> u128 {
    let mut hash = KeyHash::new(0);
    hash.f32(radius);
    hash.u32(resolution);
    hash.byte(u8::from(do_fill));
    hash.f32(thickness);
    hash.finish()
}

/// Key of the geometry-only tessellation for a rectangle.
fn rect_geometry_key(width: f32, height: f32, do_fill: bool, thickness: f32) -> u128 {
    let mut hash = KeyHash::new(1);
    hash.f32(width);
    hash.f32(height);
    hash.byte(u8::from(do_fill));
    hash.f32(thickness);
    hash.finish()
}

/// Key of the geometry-only tessellation for a line segment.
#[allow(clippy::too_many_arguments)]
fn line_geometry_key(
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    thickness: f32,
    cap: lyon_tessellation::LineCap,
    join: lyon_tessellation::LineJoin,
) -> u128 {
    let mut hash = KeyHash::new(2);
    for value in [x0, y0, x1, y1, line_stroke_width(thickness)] {
        hash.f32(value);
    }
    hash.byte(cap_code(cap));
    hash.byte(join_code(join));
    hash.finish()
}

/// Key of the vertex colours baked into an uploaded mesh.  Colour lives in the
/// vertex stream until per-instance colour exists (cross-lane request), so the
/// GPU mesh is cached by the pair of colours it carries.
fn colour_key(fill: Option<[f32; 4]>, stroke: Option<[f32; 4]>) -> u128 {
    let mut hash = KeyHash::new(0xF0);
    hash.colour(fill);
    hash.colour(stroke);
    hash.finish()
}

// ── Bounded cache ───────────────────────────────────────────────────────────

/// Maximum number of tessellations kept in [`Shape2dCache`].
///
/// Each entry holds the CPU-side vertices/indices (not the GPU copy): a path
/// of `n` triangles costs roughly `n * 3 * 64` bytes of vertex data plus
/// `n * 3 * 4` index bytes.  The cap bounds the *number* of retained
/// tessellations; one very large path still counts as a single entry, so the
/// memory is bounded by `GEOMETRY_CACHE_CAPACITY ×` the largest path the scene
/// requests.  LRU eviction makes an animation with more distinct paths than
/// this reuse the entry count instead of growing forever.
pub const GEOMETRY_CACHE_CAPACITY: usize = 256;

/// Maximum number of uploaded meshes kept in [`Shape2dCache`].
///
/// These are `Arc` handles to GPU buffers.  A scene with more unique
/// `(geometry, colour)` pairs than this will thrash, but the cap makes the
/// growth of the cache provably finite.
pub const MESH_CACHE_CAPACITY: usize = 512;

/// FNV-1a `Hasher` for the cache maps.  The keys are already content hashes,
/// so the cheaper mixer is enough; it also keeps lookups deterministic.
struct FnvHasher(u64);

impl Default for FnvHasher {
    fn default() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
}

impl Hasher for FnvHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 ^= u64::from(byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
}

type FnvBuildHasher = BuildHasherDefault<FnvHasher>;
type FnvMap<K, V> = HashMap<K, V, FnvBuildHasher>;

/// Small LRU map: `O(capacity)` eviction only runs on a miss after the cap is
/// exceeded, which is rare compared with hits.
struct Lru<K, V> {
    entries: FnvMap<K, (V, u64)>,
    tick: u64,
    capacity: usize,
}

impl<K: std::hash::Hash + Eq + Clone, V: Clone> Lru<K, V> {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: FnvMap::default(),
            tick: 0,
            capacity,
        }
    }

    fn get(&mut self, key: &K) -> Option<V> {
        self.tick = self.tick.wrapping_add(1);
        let tick = self.tick;
        let entry = self.entries.get_mut(key)?;
        entry.1 = tick;
        Some(entry.0.clone())
    }

    fn insert(&mut self, key: K, value: V) {
        if self.capacity == 0 {
            return;
        }
        self.tick = self.tick.wrapping_add(1);
        self.entries.insert(key, (value, self.tick));
        while self.entries.len() > self.capacity {
            let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, (_, used))| *used)
                .map(|(key, _)| (*key).clone())
            else {
                break;
            };
            self.entries.remove(&oldest);
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }
}

// ── Tessellation (CPU only) ─────────────────────────────────────────────────

/// Colour-independent tessellation of one shape.
///
/// `vertices[..fill_vertex_count]` are the fill triangles (when the shape was
/// filled); the remainder are stroke triangles.  Every vertex carries
/// [`NEUTRAL`]; [`mesh_from_tessellation`] writes the real colours.
#[derive(Clone)]
struct Tessellation {
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
    fill_vertex_count: usize,
}

impl Tessellation {
    fn push_into(&self, fill: [f32; 4], stroke: [f32; 4]) -> Vec<Vertex> {
        let mut vertices = self.vertices.clone();
        let fill_count = self.fill_vertex_count.min(vertices.len());
        for vertex in &mut vertices[..fill_count] {
            vertex.color = fill;
        }
        for vertex in &mut vertices[fill_count..] {
            vertex.color = stroke;
        }
        vertices
    }
}

/// Builds the GPU mesh for a tessellation, applying the requested colours.
fn mesh_from_tessellation(
    device: &wgpu::Device,
    label: &str,
    tessellation: &Tessellation,
    fill: [f32; 4],
    stroke: [f32; 4],
) -> Mesh {
    if tessellation.indices.is_empty() || tessellation.vertices.is_empty() {
        return Mesh::empty(device);
    }
    let vertices = tessellation.push_into(fill, stroke);
    buffers_to_mesh(
        device,
        label,
        VertexBuffers {
            vertices,
            indices: tessellation.indices.clone(),
        },
    )
}

/// Tessellates a [`PathData`] without touching the GPU.
#[allow(clippy::too_many_arguments)]
fn tessellate_path(
    path_data: &PathData,
    do_fill: bool,
    stroke_thickness: f32,
    cap: lyon_tessellation::LineCap,
    join: lyon_tessellation::LineJoin,
) -> Tessellation {
    let path = build_lyon_path(path_data);
    let mut buffers: VertexBuffers<Vertex, u32> = VertexBuffers::new();

    if do_fill {
        let mut tessellator = FillTessellator::new();
        // Use NonZero fill rule (required for TTF composite paths) and a safe 0.01 tolerance
        // to avoid LYON subdivision budget exhausts that cause straight paths or silent failures.
        let options =
            FillOptions::tolerance(0.01).with_fill_rule(lyon_tessellation::FillRule::NonZero);
        tessellator
            .tessellate_path(
                &path,
                &options,
                &mut BuffersBuilder::new(&mut buffers, FillCtor { color: NEUTRAL }),
            )
            .ok();
    }
    let fill_vertex_count = buffers.vertices.len();

    // Clamp to a minimum visible thickness (mirrored from line_2d).
    // Prevents 0.05-unit strokes from rounding to <1px and being discarded
    // when MSAA is absent.
    let effective_thickness = path_stroke_width(stroke_thickness);

    if effective_thickness > 0.0 {
        let mut tessellator = StrokeTessellator::new();
        let options = StrokeOptions::default()
            .with_line_width(effective_thickness)
            .with_line_cap(cap)
            .with_line_join(join)
            .with_tolerance(0.01);
        tessellator
            .tessellate_path(
                &path,
                &options,
                &mut BuffersBuilder::new(&mut buffers, StrokeCtor { color: NEUTRAL }),
            )
            .ok();
    }

    Tessellation {
        vertices: buffers.vertices,
        indices: buffers.indices,
        fill_vertex_count,
    }
}

/// Tessellates a circle without touching the GPU.
fn tessellate_circle(radius: f32, do_fill: bool, stroke_thickness: f32) -> Tessellation {
    use lyon_tessellation::path::Winding;
    let mut builder = lyon_tessellation::path::Builder::new();
    builder.add_circle(point(0.0, 0.0), radius, Winding::Positive);
    let path = builder.build();

    let mut buffers: VertexBuffers<Vertex, u32> = VertexBuffers::new();

    if do_fill {
        let mut tessellator = FillTessellator::new();
        tessellator
            .tessellate_path(
                &path,
                &FillOptions::tolerance(0.001),
                &mut BuffersBuilder::new(&mut buffers, FillCtor { color: NEUTRAL }),
            )
            .ok();
    }
    let fill_vertex_count = buffers.vertices.len();

    if stroke_thickness > 0.0 {
        let mut tessellator = StrokeTessellator::new();
        let opts = StrokeOptions::default()
            .with_line_width(stroke_thickness)
            .with_line_cap(lyon_tessellation::LineCap::Round)
            .with_line_join(lyon_tessellation::LineJoin::Round);
        tessellator
            .tessellate_path(
                &path,
                &opts,
                &mut BuffersBuilder::new(&mut buffers, StrokeCtor { color: NEUTRAL }),
            )
            .ok();
    }

    Tessellation {
        vertices: buffers.vertices,
        indices: buffers.indices,
        fill_vertex_count,
    }
}

/// Tessellates a rectangle without touching the GPU.
fn tessellate_rect(width: f32, height: f32, do_fill: bool, stroke_thickness: f32) -> Tessellation {
    let hw = width * 0.5;
    let hh = height * 0.5;

    use lyon_tessellation::path::Builder;
    let mut builder = Builder::new();
    builder.add_rectangle(
        &Box2D::new(point(-hw, -hh), point(hw, hh)),
        lyon_tessellation::path::Winding::Positive,
    );
    let path = builder.build();

    let mut buffers: VertexBuffers<Vertex, u32> = VertexBuffers::new();

    if do_fill {
        let mut tessellator = FillTessellator::new();
        tessellator
            .tessellate_path(
                &path,
                &FillOptions::tolerance(0.001),
                &mut BuffersBuilder::new(&mut buffers, FillCtor { color: NEUTRAL }),
            )
            .ok();
    }
    let fill_vertex_count = buffers.vertices.len();

    if stroke_thickness > 0.0 {
        let mut tessellator = StrokeTessellator::new();
        let opts = StrokeOptions::default()
            .with_line_width(stroke_thickness)
            .with_line_cap(lyon_tessellation::LineCap::Round)
            .with_line_join(lyon_tessellation::LineJoin::Round);
        tessellator
            .tessellate_path(
                &path,
                &opts,
                &mut BuffersBuilder::new(&mut buffers, StrokeCtor { color: NEUTRAL }),
            )
            .ok();
    }

    Tessellation {
        vertices: buffers.vertices,
        indices: buffers.indices,
        fill_vertex_count,
    }
}

/// Tessellates a line segment without touching the GPU.
fn tessellate_line(
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    thickness: f32,
    cap: lyon_tessellation::LineCap,
    join: lyon_tessellation::LineJoin,
) -> Tessellation {
    let thickness = line_stroke_width(thickness);

    let mut builder = lyon_tessellation::path::Path::builder();
    builder.begin(point(x0, y0));
    builder.line_to(point(x1, y1));
    builder.end(false);
    let path = builder.build();

    let opts = StrokeOptions::default()
        .with_line_width(thickness)
        .with_line_cap(cap)
        .with_line_join(join)
        .with_tolerance(0.01);

    let mut buffers: VertexBuffers<Vertex, u32> = VertexBuffers::new();
    let mut tessellator = StrokeTessellator::new();
    // A degenerate line (two identical endpoints, non-finite input) makes lyon
    // return an error. The frame path must not panic: an empty mesh draws
    // nothing, which is the same visible result.
    if tessellator
        .tessellate_path(
            &path,
            &opts,
            &mut BuffersBuilder::new(&mut buffers, StrokeCtor { color: NEUTRAL }),
        )
        .is_err()
    {
        buffers.vertices.clear();
        buffers.indices.clear();
    }

    Tessellation {
        vertices: buffers.vertices,
        indices: buffers.indices,
        fill_vertex_count: 0,
    }
}

// ── Cache ───────────────────────────────────────────────────────────────────

/// Counters used by the cache tests.  Never compiled into a release build and
/// never read on the frame path.
#[cfg(test)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheStats {
    /// Tessellations actually run (geometry-cache misses).
    pub tessellations: u64,
    /// Geometry-cache hits.
    pub geometry_hits: u64,
    /// GPU meshes built (mesh-cache misses).
    pub mesh_uploads: u64,
    /// Mesh-cache hits.
    pub mesh_hits: u64,
}

/// Bounded tessellation + upload cache for the `*2D` primitives and `Path`.
///
/// See the module docs for the key split and the bound.  Create one per
/// [`FrameBuilder`](crate::frame_builder::FrameBuilder).
pub struct Shape2dCache {
    geometry: Lru<u128, Arc<Tessellation>>,
    meshes: Lru<(u128, u128), Mesh>,
    #[cfg(test)]
    stats: CacheStats,
}

impl Default for Shape2dCache {
    fn default() -> Self {
        Self::new()
    }
}

impl Shape2dCache {
    /// Creates an empty cache with the documented capacities.
    pub fn new() -> Self {
        Self {
            geometry: Lru::with_capacity(GEOMETRY_CACHE_CAPACITY),
            meshes: Lru::with_capacity(MESH_CACHE_CAPACITY),
            #[cfg(test)]
            stats: CacheStats::default(),
        }
    }

    /// Reuses the tessellation for `key`, or runs `build` on a miss.
    fn geometry_for(
        &mut self,
        key: u128,
        build: impl FnOnce() -> Tessellation,
    ) -> Arc<Tessellation> {
        if let Some(geometry) = self.geometry.get(&key) {
            #[cfg(test)]
            {
                self.stats.geometry_hits += 1;
            }
            return geometry;
        }
        #[cfg(test)]
        {
            self.stats.tessellations += 1;
        }
        let geometry = Arc::new(build());
        self.geometry.insert(key, geometry.clone());
        geometry
    }

    /// Reuses the uploaded mesh for `key`, or builds and caches one.
    fn mesh_for(&mut self, key: (u128, u128), build: impl FnOnce() -> Mesh) -> Mesh {
        if let Some(mesh) = self.meshes.get(&key) {
            #[cfg(test)]
            {
                self.stats.mesh_hits += 1;
            }
            return mesh;
        }
        #[cfg(test)]
        {
            self.stats.mesh_uploads += 1;
        }
        let mesh = build();
        self.meshes.insert(key, mesh.clone());
        mesh
    }

    /// Mesh handle for a `Path`, reusing the cached tessellation.
    ///
    /// The transform is intentionally absent from the key: the model matrix is
    /// per-instance data in the vertex shader, so moving, rotating or scaling
    /// the element never re-tessellates it.  Colour is not part of the
    /// *geometry* key either; it only selects the (recoloured) GPU mesh.
    #[allow(clippy::too_many_arguments)]
    pub fn path_mesh(
        &mut self,
        device: &wgpu::Device,
        path: &PathData,
        do_fill: bool,
        stroke_thickness: f32,
        fill: [f32; 4],
        stroke: [f32; 4],
        cap: lyon_tessellation::LineCap,
        join: lyon_tessellation::LineJoin,
    ) -> Mesh {
        let geometry_key = path_geometry_key(path, do_fill, stroke_thickness, cap, join);
        let geometry = self.geometry_for(geometry_key, || {
            tessellate_path(path, do_fill, stroke_thickness, cap, join)
        });
        let mesh_key = (geometry_key, colour_key(Some(fill), Some(stroke)));
        self.mesh_for(mesh_key, || {
            mesh_from_tessellation(device, "Path2D", &geometry, fill, stroke)
        })
    }

    /// Mesh handle for a `Circle2D`, reusing the cached tessellation.
    ///
    /// `resolution` is part of the key for API completeness even though
    /// [`circle_2d`] currently ignores it; that keeps every documented input
    /// covered if the tessellator starts honouring it.
    #[allow(clippy::too_many_arguments)]
    pub fn circle_mesh(
        &mut self,
        device: &wgpu::Device,
        radius: f32,
        resolution: u32,
        do_fill: bool,
        stroke_thickness: f32,
        fill: [f32; 4],
        stroke: [f32; 4],
    ) -> Mesh {
        let geometry_key = circle_geometry_key(radius, resolution, do_fill, stroke_thickness);
        let geometry = self.geometry_for(geometry_key, || {
            tessellate_circle(radius, do_fill, stroke_thickness)
        });
        let mesh_key = (geometry_key, colour_key(Some(fill), Some(stroke)));
        self.mesh_for(mesh_key, || {
            mesh_from_tessellation(device, "Circle2D", &geometry, fill, stroke)
        })
    }

    /// Mesh handle for a `Rect2D`, reusing the cached tessellation.
    #[allow(clippy::too_many_arguments)]
    pub fn rect_mesh(
        &mut self,
        device: &wgpu::Device,
        width: f32,
        height: f32,
        do_fill: bool,
        stroke_thickness: f32,
        fill: [f32; 4],
        stroke: [f32; 4],
    ) -> Mesh {
        let geometry_key = rect_geometry_key(width, height, do_fill, stroke_thickness);
        let geometry = self.geometry_for(geometry_key, || {
            tessellate_rect(width, height, do_fill, stroke_thickness)
        });
        let mesh_key = (geometry_key, colour_key(Some(fill), Some(stroke)));
        self.mesh_for(mesh_key, || {
            mesh_from_tessellation(device, "Rect2D", &geometry, fill, stroke)
        })
    }

    /// Mesh handle for a `Line2D`, reusing the cached tessellation.
    #[allow(clippy::too_many_arguments)]
    pub fn line_mesh(
        &mut self,
        device: &wgpu::Device,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        thickness: f32,
        colour: [f32; 4],
        cap: lyon_tessellation::LineCap,
        join: lyon_tessellation::LineJoin,
    ) -> Mesh {
        let geometry_key = line_geometry_key(x0, y0, x1, y1, thickness, cap, join);
        let geometry = self.geometry_for(geometry_key, || {
            tessellate_line(x0, y0, x1, y1, thickness, cap, join)
        });
        let mesh_key = (geometry_key, colour_key(None, Some(colour)));
        self.mesh_for(mesh_key, || {
            mesh_from_tessellation(device, "Line2D", &geometry, colour, colour)
        })
    }

    /// Cache counters (test builds only).
    #[cfg(test)]
    pub fn stats(&self) -> CacheStats {
        self.stats
    }

    /// Test-only entry point that exercises the geometry LRU without a GPU.
    #[cfg(test)]
    fn geometry_for_test(
        &mut self,
        key: u128,
        build: impl FnOnce() -> Tessellation,
    ) -> Arc<Tessellation> {
        self.geometry_for(key, build)
    }

    #[cfg(test)]
    fn cached_geometry_len(&self) -> usize {
        self.geometry.len()
    }
}

// ── Public API ───────────────────────────────────────────────────────────────

use ferrous_core::scene::world::types::{PathCommand, PathData};
use lyon_tessellation::path::Path;

fn build_lyon_path(path_data: &PathData) -> Path {
    use lyon_tessellation::path::Builder;
    let mut builder = Builder::new();
    let mut contour_open = false;
    for cmd in &path_data.commands {
        match cmd {
            PathCommand::MoveTo(p) => {
                // If a previous contour was open (no Close command), end it explicitly.
                if contour_open {
                    builder.end(false);
                }
                builder.begin(point(p.x, p.y));
                contour_open = true;
            }
            PathCommand::LineTo(p) => {
                builder.line_to(point(p.x, p.y));
            }
            PathCommand::CubicTo(c1, c2, p) => {
                builder.cubic_bezier_to(point(c1.x, c1.y), point(c2.x, c2.y), point(p.x, p.y));
            }
            PathCommand::Close => {
                builder.end(true);
                contour_open = false;
            }
        }
    }
    // End any remaining open contour as an open path (not closed).
    if contour_open {
        builder.end(false);
    }
    builder.build()
}

/// Dynamic tessellation of any [`PathData`].
/// Supports simultaneous fill and stroke.
#[allow(clippy::too_many_arguments)]
// Tessellation entry point; every field is a caller-facing knob.
pub fn path_to_mesh(
    device: &wgpu::Device,
    path_data: &PathData,
    do_fill: bool,
    stroke_thickness: f32,
    fill_color: [f32; 4],
    stroke_color: [f32; 4],
    cap: lyon_tessellation::LineCap,
    join: lyon_tessellation::LineJoin,
) -> Mesh {
    let tessellation = tessellate_path(path_data, do_fill, stroke_thickness, cap, join);
    mesh_from_tessellation(device, "Path2D", &tessellation, fill_color, stroke_color)
}

/// Filled circle disc in the XY plane (z = 0), centred at the origin.
pub fn circle_2d(
    device: &wgpu::Device,
    radius: f32,
    _resolution: u32,
    do_fill: bool,
    stroke_thickness: f32,
    fill_c: [f32; 4],
    stroke_c: [f32; 4],
) -> Mesh {
    let tessellation = tessellate_circle(radius, do_fill, stroke_thickness);
    mesh_from_tessellation(device, "Circle2D", &tessellation, fill_c, stroke_c)
}

/// Filled axis-aligned rectangle in the XY plane (z = 0), centred at the origin.
pub fn rect_2d(
    device: &wgpu::Device,
    width: f32,
    height: f32,
    do_fill: bool,
    stroke_thickness: f32,
    fill_c: [f32; 4],
    stroke_c: [f32; 4],
) -> Mesh {
    let tessellation = tessellate_rect(width, height, do_fill, stroke_thickness);
    mesh_from_tessellation(device, "Rect2D", &tessellation, fill_c, stroke_c)
}

/// Thick line segment in the XY plane (z = 0).
#[allow(clippy::too_many_arguments)]
// Tessellation entry point; every field is a caller-facing knob.
pub fn line_2d(
    device: &wgpu::Device,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    thickness: f32,
    color: [f32; 4],
    cap: lyon_tessellation::LineCap,
    join: lyon_tessellation::LineJoin,
) -> Mesh {
    let tessellation = tessellate_line(x0, y0, x1, y1, thickness, cap, join);
    mesh_from_tessellation(device, "Line2D", &tessellation, color, color)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_path(x: f32) -> PathData {
        PathData {
            commands: vec![
                PathCommand::MoveTo(glam::Vec2::new(x, 0.0)),
                PathCommand::LineTo(glam::Vec2::new(x + 10.0, 0.0)),
                PathCommand::LineTo(glam::Vec2::new(x + 10.0, 10.0)),
                PathCommand::Close,
            ],
        }
    }

    fn red() -> [f32; 4] {
        [1.0, 0.0, 0.0, 1.0]
    }

    fn blue() -> [f32; 4] {
        [0.0, 0.0, 1.0, 1.0]
    }

    #[test]
    fn colour_is_not_part_of_the_geometry_key_but_is_part_of_the_mesh_key() {
        let path = test_path(0.0);
        let key = path_geometry_key(
            &path,
            true,
            2.0,
            lyon_tessellation::LineCap::Butt,
            lyon_tessellation::LineJoin::Miter,
        );
        // The geometry key does not take a colour argument: identical geometry
        // always resolves to the same tessellation.  The colour only selects
        // the uploaded (recoloured) mesh.
        let same = path_geometry_key(
            &path,
            true,
            2.0,
            lyon_tessellation::LineCap::Butt,
            lyon_tessellation::LineJoin::Miter,
        );
        assert_eq!(key, same);
        assert_ne!(
            colour_key(Some(red()), None),
            colour_key(Some(blue()), None)
        );
        assert_eq!(
            (key, colour_key(Some(red()), None)),
            (same, colour_key(Some(red()), None))
        );
    }

    #[test]
    fn a_colour_only_change_does_not_tessellate_again() {
        let path = test_path(0.0);
        let mut cache = Shape2dCache::new();
        let key = path_geometry_key(
            &path,
            true,
            2.0,
            lyon_tessellation::LineCap::Butt,
            lyon_tessellation::LineJoin::Miter,
        );
        cache.geometry_for_test(key, || Tessellation {
            vertices: Vec::new(),
            indices: Vec::new(),
            fill_vertex_count: 0,
        });
        // The second "colour" is not part of this key, exactly as the real
        // `path_mesh` path works: only `colour_key` would change.
        cache.geometry_for_test(key, || panic!("a colour change must not re-tessellate"));
        assert_eq!(cache.stats().tessellations, 1);
        assert_eq!(cache.stats().geometry_hits, 1);
    }

    #[test]
    fn a_point_change_tessellates_again() {
        let mut cache = Shape2dCache::new();
        let first = path_geometry_key(
            &test_path(0.0),
            true,
            2.0,
            lyon_tessellation::LineCap::Butt,
            lyon_tessellation::LineJoin::Miter,
        );
        let second = path_geometry_key(
            &test_path(1.0),
            true,
            2.0,
            lyon_tessellation::LineCap::Butt,
            lyon_tessellation::LineJoin::Miter,
        );
        assert_ne!(first, second);
        for key in [first, second] {
            cache.geometry_for_test(key, || Tessellation {
                vertices: Vec::new(),
                indices: Vec::new(),
                fill_vertex_count: 0,
            });
        }
        assert_eq!(cache.stats().tessellations, 2);
        assert_eq!(cache.stats().geometry_hits, 0);
    }

    #[test]
    fn the_geometry_cache_hits_misses_and_evicts_at_its_cap() {
        let mut cache = Shape2dCache::new();
        let empty = || Tessellation {
            vertices: Vec::new(),
            indices: Vec::new(),
            fill_vertex_count: 0,
        };
        cache.geometry_for_test(1, empty);
        cache.geometry_for_test(1, || panic!("a repeated key must hit"));
        assert_eq!(cache.stats().tessellations, 1);
        assert_eq!(cache.stats().geometry_hits, 1);

        for key in 2..=(GEOMETRY_CACHE_CAPACITY as u128 + 8) {
            cache.geometry_for_test(key, empty);
        }
        assert_eq!(cache.cached_geometry_len(), GEOMETRY_CACHE_CAPACITY);
        // Key 1 was the least recently used and must have been evicted.
        let before = cache.stats().tessellations;
        cache.geometry_for_test(1, empty);
        assert_eq!(cache.stats().tessellations, before + 1);
    }

    #[test]
    fn the_mesh_lru_hits_and_evicts_like_the_geometry_one() {
        // Exercised with plain `u32` values: the LRU is generic and the mesh
        // cache only differs in its value type.
        let mut lru: Lru<u128, u32> = Lru::with_capacity(2);
        lru.insert(1, 10);
        lru.insert(2, 20);
        assert_eq!(lru.get(&1), Some(10));
        lru.insert(3, 30); // evicts 2, the least recently used
        assert_eq!(lru.get(&2), None);
        assert_eq!(lru.get(&1), Some(10));
        assert_eq!(lru.get(&3), Some(30));
        assert_eq!(lru.len(), 2);
    }

    #[test]
    fn stroke_width_is_clamped_before_hashing() {
        let path = test_path(0.0);
        let zero = path_geometry_key(
            &path,
            false,
            0.0,
            lyon_tessellation::LineCap::Butt,
            lyon_tessellation::LineJoin::Miter,
        );
        let tiny = path_geometry_key(
            &path,
            false,
            1e-9,
            lyon_tessellation::LineCap::Butt,
            lyon_tessellation::LineJoin::Miter,
        );
        let clamped = path_geometry_key(
            &path,
            false,
            1e-3,
            lyon_tessellation::LineCap::Butt,
            lyon_tessellation::LineJoin::Miter,
        );
        assert_eq!(tiny, clamped, "widths under the clamp hash the same");
        assert_ne!(zero, tiny, "zero width means no stroke, not a hairline");
    }

    #[test]
    fn geometry_vertices_are_neutral_until_recoloured() {
        let tessellation = tessellate_rect(10.0, 4.0, true, 0.0);
        assert!(!tessellation.vertices.is_empty());
        assert!(tessellation.vertices.iter().all(|v| v.color == NEUTRAL));
        assert_eq!(tessellation.fill_vertex_count, tessellation.vertices.len());
        let recoloured = tessellation.push_into(red(), blue());
        assert!(recoloured.iter().all(|v| v.color == red()));
    }
}
