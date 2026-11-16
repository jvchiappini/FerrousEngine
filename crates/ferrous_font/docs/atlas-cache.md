# Baked atlases and the disk cache

Turning a font into an MSDF atlas is CPU work that depends only on the font
bytes and the character set: it does not need a device, a queue or a window.
`ferrous_font` splits that half out so it can be reused, shared and cached.

## The two halves

| Type | Needs a GPU | Holds |
|---|---|---|
| `BakedAtlas` | no | MSDF pixels, per-glyph metrics, glyph indices, `kern`/GPOS kerning, vertical metrics, the cache key |
| `FontAtlas` | yes | the texture, view and sampler, plus the data of the `BakedAtlas` it was uploaded from |

```rust
use ferrous_font::{BakedAtlas, FontParser};

let parser = FontParser::new(font_bytes.to_vec())?;
let baked = BakedAtlas::build(&parser, &ferrous_font::charset::latin_western())?;
let atlas = baked.upload(&device, &queue);
```

`FontAtlas::new(device, queue, parser, chars)` is exactly those two calls, so
existing code keeps working. `FontAtlas::key` identifies the inputs:
two atlases with the same key have the same pixels and tables.

`BakedAtlas::encode` / `BakedAtlas::decode` are the versioned binary form the
cache stores. The layout is deterministic (maps are written in key order), a
different `ATLAS_FORMAT_VERSION` or `ATLAS_GENERATOR_VERSION` is rejected, and
every read is bounds-checked: a truncated or corrupt file is an
`AtlasDecodeError`, never a panic.

## The disk cache

```rust
use ferrous_font::{AtlasCache, BakedAtlas};

let cache = AtlasCache::new(cache_dir);
let baked = BakedAtlas::build_cached(Some(&cache), font_bytes, &chars)?;
```

- **Key**: a 128-bit FNV-1a hash of the font bytes, the character set (in atlas
  order), the cell size, the pixel range, the format version and the generator
  version. The file is `<key>.atlas` in the cache directory.
- **Atomic**: the file is written to a temporary name and renamed into place, so
  a killed process never leaves a partial atlas under the final name.
- **Validated on read**: a missing, truncated, corrupt or outdated file is
  deleted and the atlas is baked again. The stored key is compared with the
  requested one, so a hash collision cannot serve the wrong pixels.
- **Bounded**: `max_bytes` (64 MiB by default) is enforced by deleting the least
  recently used files; the file just written is never the one evicted. A hit
  updates the file's modification time, which is what "recently used" means.
- **Optional**: failures are silent. The cache is an optimisation; a read-only
  or full disk just means every launch bakes.

`FontCacheConfig` describes the location for `App`, and
`default_cache_directory(app_name)` resolves the per-OS default:

| Platform | Directory |
|---|---|
| Windows | `%LOCALAPPDATA%\<app>\cache\fonts` |
| macOS | `~/Library/Caches/<app>/fonts` |
| Linux | `$XDG_CACHE_HOME/<app>/fonts`, else `~/.cache/<app>/fonts` |
| wasm32 | none (no filesystem) |

```rust
App::new(MyApp)
    .with_font_cache(ferrous_font::FontCacheConfig::for_app("MyApp"))
    .run();
```

The application decides the name (the engine cannot know it); `with_directory`
overrides the location and `FontCacheConfig::disabled()` turns the cache off.
`Font::try_load_bytes_cached(..., Some(&cache))` is the equivalent for code that
loads a font outside the app runner.

## Dedupe in the renderer

`Renderer::register_font_2d` keys its registry by the same `FontKey`:
registering the same bytes and character set twice returns the **same**
`FontId2d` and neither bakes nor uploads a second atlas. The engine registers
its UI font through that path too, so an application that draws the same face
(for example the bundled Roboto) simply gets the UI font's handle. Install the
cache with `Renderer::set_font_cache`; `Renderer::font_2d` hands back the shared
`Arc<Font>` when an application needs the metrics.

## Fast cold bakes

`msdf_gen` culls work per pixel tile: every flattened edge is binned into the
8x8 pixel tiles its bounding box (grown by the saturation radius) can influence,
and winding only looks at edges whose y range straddles the row. An edge that is
skipped either contributes zero winding or encodes an already-saturated channel
value, so the output is **byte-identical** to the un-culled reference — the
`msdf_gen` tests compare against that reference for squares and real glyphs at
16 and 64 px.

`cargo run --release -p ferrous_font --example msdf_bench -- 5 64` prints the
CPU cost of baking `latin_western` (305 glyphs) on the current machine.

## Layout data

`LayoutData` remains the device-free layout source. `BakedAtlas` keeps the same
tables, so a cache hit measures text exactly like a cold bake.
