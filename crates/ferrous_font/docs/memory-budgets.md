# Font and texture memory budgets

Every cache that can grow at runtime has a byte budget, an eviction rule and a
label in the engine memory inventory. The layout cache is per atlas; the atlas
glyph tables are bounded by the requested character set; the on-disk atlas cache
and the SVG raster cache are process-global and least-recently-used.

## Budgets at a glance

| Cache | Default budget | Eviction | Where |
|---|---:|---|---|
| Layout glyph cache (per `FontAtlas`) | 16 MiB | LRU entries, oldest first | `src/budget.rs:13`, `src/layout.rs:399-601` |
| Atlas glyph tables (per bake) | 16 MiB | none: a bake over budget is a typed error | `src/budget.rs:20`, `src/baked.rs:263-269` |
| Baked atlas file cache | 64 MiB | oldest file modification time, newest file kept | `src/cache.rs:21`, `src/cache.rs:251-284` |
| SVG raster cache | 64 MiB | oldest inserted pixel entry | `../../ferrous_assets/src/texture.rs:352`, `:433-459` |

The atlas table budget is deliberately not an LRU: those maps are the atlas the
caller asked for, not a cache. Evicting a glyph would silently change layout.
A character set whose metrics, indices, `kern` and GPOS tables exceed 16 MiB is
rejected with `FontError::Atlas` instead of letting the process grow without
bound (`src/baked.rs:263-269`).

## Layout glyph cache

`FontAtlas::layout_block` keeps the glyph records and metrics it produced in a
per-atlas LRU (`src/atlas.rs:144-232`). The key is the text hash plus the four
style values and the alignment; the value keeps the exact text so a hash
collision is a miss, never a wrong layout. A hit copies the cached records into
the caller's `Vec`, so the output is bit-for-bit the same as an uncached call
and a reused buffer stays allocation-free.

The cache is bounded by `LAYOUT_CACHE_BUDGET_BYTES` (16 MiB,
`src/budget.rs:13`). An entry larger than the whole budget is not cached, which
avoids a single very long string thrashing every other entry
(`src/layout.rs:544-601`). `FontAtlas::layout_cache_bytes` reports the current
charge (`src/atlas.rs:234`).

Lookups are constant time (prompt 18 P4): one map probe, an exact-text compare
and a constant number of pointer updates in an intrusive LRU over an entry
arena (`src/layout.rs:383-601`). A hit allocates nothing; eviction recycles the
slot and the entry keeps its glyph allocation.

The ASCII fast path is a 128-entry metric/indices table derived from the atlas
maps and built on first use, stored on the `FontAtlas` itself
(`src/atlas.rs:249`). It is charged to the inventory under `ascii tables`.

## Atlas occupancy

`BakedAtlas::occupancy()` and `IconAtlas::occupancy()` report
`ferrous_font::AtlasOccupancy`: used cells, cell area, allocated area, atlas
area, allocation occupancy and ink occupancy (texels that are not the neutral
background), plus a `Display` form for the bake log (`src/baked.rs:99-170`).
Cells are a fixed 64-px grid, so allocation occupancy is `ceil(sqrt(n))^2`
cells and cannot be improved by a tighter packer without changing cell size,
which would change MSDF rasterisation and UVs. A best-fit packer was measured
and rejected for golden safety in prompt 18 P4; the report is diagnostic, not a
new packing policy.

## Inventory labels

`ferrous_font` has no dependency on `ferrous_core`. The first
`GuiRenderer::new` installs a process-global sink
(`src/gui_renderer.rs:23-61`, `:135-136`); the sink forwards every registration
to `ferrous_core::memory::{register_cpu, register_gpu}`. Without a sink the calls
are no-ops, so tools and headless tests still work.

`FontAtlas` can own these labels, all prefixed with
`font:<FontKey hex>#<instance>` (the instance suffix keeps two atlases with the
same key from unregistering each other):

| Label suffix | Side | Bytes |
|---|---|---|
| `texture` | GPU | width x height x 4 (`src/atlas.rs:176-182`) |
| `glyph metrics` | CPU | estimated map bytes (`src/atlas.rs:184-200`) |
| `glyph indices` | CPU | estimated map bytes |
| `kern pairs` | CPU | estimated map bytes |
| `gpos kerning` | CPU | explicit pairs plus class tables (`src/parser.rs:111-124`) |
| `layout cache` | CPU | current LRU charge (`src/atlas.rs:234-237`) |
| `ascii tables` | CPU | 0 until the first ASCII layout, then one 128-entry table (`src/atlas.rs:249-252`) |

`BakedAtlas::upload` registers the four CPU table labels automatically; the
`texture` line is exposed for the renderer that owns the GPU bind group, so it is
not counted twice. `FontAtlas::inventory_entries` returns the whole set for a
caller that wants to forward it manually (`src/atlas.rs:222-246`), and `Drop`
removes every label it knows about (`src/atlas.rs:258-280`). A `BakedAtlas` that
is kept before upload owns the
CPU half under `font:<key> baked pixels` and the four table labels
(`src/baked.rs:306-324`); `BakedAtlas::upload` consumes it and drops the pixels,
so the long-lived copy is the `FontAtlas` texture plus its tables.

`LayoutData` exposes its tables with a caller-provided prefix, for example
`layout:<key> glyph metrics` (`src/layout_data.rs:167-200`). `ImageData` exposes
`cpu_bytes()` for the decoded RGBA8 copy (`../../ferrous_assets/src/image_importer.rs:76-84`),
and `Texture2d` exposes `gpu_bytes()` (`../../ferrous_assets/src/texture.rs:26-37`).
The SVG cache registers `asset:svg:<hash>:<width>x<height> pixels` and
`asset:svg:<hash>:<width>x<height> texture` and removes both when an entry is
evicted or cleared (`../../ferrous_assets/src/texture.rs:374-416`, `:502-590`).

## Mip policy

The MSDF atlas keeps **one** mip level and samples it with linear minification
(`src/baked.rs:365-388`). This is intentional:

- The cells store signed distances, not colours. Averaging neighbouring texels
  across a cell boundary averages unrelated distance fields and blurs the ink
  contour; a mip chain would change sampled pixels, so it cannot be added while
  the engine and Motioner goldens must stay byte-identical.
- UI text samples the atlas at roughly 1:1 (`size_px` drives the quad), so a
  mip chain would be unused in the normal path.
- The texture format stays `Rgba8Unorm` because three channels carry the MSDF
  distance and the fourth carries coverage; a lossy format change would alter
  the sampled values.

`Texture2d::from_rgba8` and `from_hdr` also use one mip
(`../../ferrous_assets/src/texture.rs:53`, `:159`). General images that are
minified go through the renderer's `TextureRegistry`, which builds the mip chain
on the CPU (`../../ferrous_renderer/src/resources/texture_registry.rs:126-170`);
that is the correct place for image mips because the atlas policy above does not
apply to colour images. The HDR helper feeds environment precomputation, and
adding mips there would change the prefiltered result, so it keeps one level.

## Duplicate textures

- `AssetServer` deduplicates by `(TypeId, canonical path)`: a second `load` of
  the same file returns the same handle and starts no second import
  (`../../ferrous_assets/src/server.rs:173-185`).
- The SVG cache deduplicates by `(content hash, width, height)` and can share a
  single `Arc<Texture2d>` (`../../ferrous_assets/src/texture.rs:361-372`,
  `:502-590`).
- `Renderer::register_font_2d` deduplicates atlases by `FontKey`: the same font
  bytes and character set return the same `FontId2d` and upload one texture
  (`../../ferrous_renderer/src/renderer_core.rs:4931-4940`).
- `TextureRegistry` in the renderer does not deduplicate: every
  `register_rgba8`/`register_from_path` creates a texture
  (`../../ferrous_renderer/src/resources/texture_registry.rs:84-225`). A caller
  that loads the same bitmap twice through that registry still uploads it twice;
  the content-hash index belongs there and is listed as a cross-lane request in
  the phase hand-off.
