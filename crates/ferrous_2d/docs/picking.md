# Picking

`ferrous_2d::picking` answers the questions an editor or a game asks about what
it drew: *what is under this point*, *what does this marquee select*, *what is
the nearest edge to snap to*. It is generic engine code, not editor code; what a
hit means (selection, a gizmo handle, a snap target) stays in the application.

```rust
use ferrous_2d::picking::{MarqueeMode, PickCamera, PickItem, PickScene, PickShape};
use glam::{Affine2, Vec2};

let mut scene = PickScene::new();
scene.add(PickItem::new(7, PickShape::rounded_rect(Vec2::new(240.0, 120.0), 12.0)));
scene.rebuild();

let camera = PickCamera::new(Vec2::new(1280.0, 800.0));
let world = camera.screen_to_world(Vec2::new(640.0, 400.0));
if let Some(hit) = scene.pick(world, 2.0) {
    println!("selected {}", hit.id);
}
```

## What an item is

A `PickItem` has a caller id, a **full 2D affine transform** (translation,
rotation, scale and skew), a local shape, a `z`, an optional clip chain, and
`visible` / `opacity` / `pickable` flags. Shapes:

| Shape | Exact hit test |
|---|---|
| `Rect { size, corner_radius }` | rounded-rectangle inside test in local space |
| `Circle { radius }` | distance to the centre |
| `Path { points, closed, filled, fill_rule, stroke }` | non-zero or even-odd fill, and/or distance to the polyline with a width and caps |
| `Text { bounds }` | the measured run's local box |
| `Image { bounds, alpha }` | the box plus an optional RGBA8 alpha threshold |

The local box of a rectangle and a circle is centred on the origin, matching
`ShapeInstance::rect` / `circle`. Text bounds are supplied by the caller because
only the host can shape the run (`ferrous_font`).

Clips are `ClipNode { rect, transform, parent, enabled }` and form a chain. A
point is pickable only when every ancestor clip contains it; the item's indexed
bounds are intersected with every clip's world box, so a clipped item never
appears in a marquee outside its clips.

## The index

`PickScene::rebuild` sorts items by `(z, insertion)` and inserts their world
AABBs into `ferrous_core::spatial::SpatialIndex`, the **same** uniform grid the
GUIMaker input router uses (`guimaker::spatial` is a thin adapter over it). A
point query is `O(cell)`, a marquee query is `O(cells + candidates)`, and
neither walks every shape. Rebuild only when geometry changes: `add`,
`items_mut` and `mark_dirty` set the dirty flag; the next query rebuilds once.

Queries and their buffers:

| Query | Cost at 10 000 shapes (release, this machine) | Allocations |
|---|---|---|
| `pick(point)` (10 000 queries, 61 ns each) | 61 ns per query | 0 |
| `marquee(Intersect)` | 5.4 µs | 0 with a warm buffer |
| `marquee(Contain)` | 93 µs | 0 |
| `lasso` | 14.9 µs | 0 |
| `nearest_point` / `nearest_edge` | 174 ns per query | 0 |
| `rebuild` | 462 µs | rebuild only |

`pick_all`, `marquee`, `lasso` write into a `Vec<PickHit>` the caller reuses.
`benches/picking.rs` prints the numbers above; `tests/picking_alloc.rs` asserts
zero allocations over 90 warm queries.

## Tolerance, rotation and zoom

`pick(point, tolerance)` takes the tolerance in **screen pixels**; it is
converted to local units by the item's transform (the inverse of the square root
of the absolute determinant). That keeps a stroke's grab band constant on screen
at every zoom, which is what an editor wants. Fills are exact.

## Marquee and lasso semantics

- `MarqueeMode::Intersect`: the shape's AABB overlaps, one of its outline
  points is inside the area, an edge crosses the area, or an area corner is
  inside the shape (for a rotated rectangle this is the exact overlap).
- `MarqueeMode::Contain`: every outline point and the whole AABB are inside.

The lasso uses the same outline tests with a point-in-polygon area. Both return
the topmost item first.

## Nearest point and edge

`nearest_point(point, radius)` and `nearest_edge` search the index within the
radius and return the closest point in world space (and, for an edge, its index)
on any pickable shape. Rectangles and text/images return the closest point on
their four edges; circles the radial point; paths the closest segment. This is
what snapping uses; the snap *rule* (grid, guides, other layers) stays in the
application.

## Camera

`PickCamera { center, zoom, viewport }` is the inverse of Pure2D's 2D mapping:
one world unit at zoom 1 is one logical pixel, `y` down, top-left screen origin.
It provides `screen_to_world`, `world_to_screen`, `visible_world`,
`zoom_to_cursor` (keeps the point under the cursor fixed), `pan`, `fit` and the
screen-to-world affine matrix. No host writes its own screen/world maths again.

## GPU ID-buffer picking

The GPU path is implemented in `picking::GpuPicker` and documented with its
measured numbers in [picking-gpu.md](picking-gpu.md). It renders the same items
into a small `R32Uint` target (`scene_id_2d.wgsl`) with the same geometry, clips,
tolerance and `(z, insertion)` order, reads back one pixel or a region, and
decodes it with the integer counterpart of `IdBuffer` (`encode_u32` /
`decode_u32`, since `R32Uint` has no alpha channel and reserves `0` for clear).

When it wins:

- **spatial index** - interactive picking, hover and drag, 10 000-100 000
  shapes: tens to hundreds of nanoseconds per query, already built.
- **id buffer** - one pass over a very dense scene, a large region (a brush or
  a whole-canvas id map), or when the scene is already being drawn into an
  offscreen target. It costs a render pass plus a `map_async` readback (hundreds
  of microseconds at best), so never call it once per pointer move.

On the reference host the GPU pass wins from about 256x256 sampled pixels
(2.0 ms CPU reference vs 0.8 ms GPU), 3.7x at 512x512 and 4.3x at 1024x1024. A
point pick stays on the CPU (63 ns vs 399 us), and lasso, contain, nearest and
image-alpha queries have no GPU version.

`IdBuffer::rasterize` remains the CPU reference (one pick per pixel): 117 us for
64x64 and 33 ms for 1024x1024 on a 10 000-shape scene here. Use the GPU pass on
demand only, and verify its output against the reference.

## Migrating a hand-written picker

Motioner's canvas is the first consumer (Phase 10; not changed by this phase).
It currently computes, by hand: each item's local box (rect/image, circle
square, path bounds, text advance × ascender/descender), the affine mapping and
its inverse, oriented-box/circle/path containment, stroke tolerance, clip and
opacity filtering, topmost ordering, group union bounds, handle sizes divided by
zoom, marquee/contain semantics and snap distances. All of that is the same
geometry this module now exposes; the mapping from a Motioner `RenderItem` to a
`PickItem` is the only new code, and "what a hit means" stays in Motioner.
