# Images and sprite textures

`ferrous_2d` has one textured 2D path: a registered RGBA8 texture drawn as a
unit quad through `SpriteInstance` and the sprite shader.  The legacy
registrations (`register_sprite_texture`, `_linear`, `_premultiplied`) stay as
they are; `register_texture_2d` adds the production image path (mipmaps,
trilinear sampling, premultiplied upload and a byte budget).

## Registration

```rust
let texture = renderer.register_texture_2d(
    width,
    height,
    &rgba8_srgb_straight,
    ferrous_2d::render::TextureOptions2d::photo(),
)?;
renderer.draw_sprite_2d(texture, SpriteInstance { /* ... */ });
```

`TextureOptions2d { mipmaps, filter }`:

| Option | Effect |
|---|---|
| `mipmaps: true` | Builds a full CPU mip chain and samples it with `min_filter = Linear`, `mipmap_filter = Linear` (trilinear). |
| `mipmaps: false` | One level; `mipmap_filter = Nearest`. |
| `filter: Linear` | Bilinear magnification/minification. |
| `filter: Nearest` | Exact texels for pixel art; magnification stays nearest even when mipmaps are on. |

`TextureOptions2d::photo()` is mipmapped + linear; `::pixel_art()` is one level
+ nearest.

## Updating a texture in place

A preview that produces a new frame at the same size must not free and
re-register the sprite every frame: that rebuilds the texture, the sampler and
the bind group.  `Renderer::update_sprite_texture(id, width, height, data)`
rewrites the existing texture with `queue.write_texture` and returns a typed
`TextureUpdateError`:

| Variant | Meaning |
|---|---|
| `ZeroSize` | A dimension is zero. |
| `UnknownTexture` | The id was never registered or was freed. |
| `SizeMismatch` | `data` is not `width * height * 4` bytes. |
| `DimensionsMismatch` | The registered texture has another size; free and register again. |

`data` uses the encoding of the registration call (straight sRGB for
`register_sprite_texture*`, premultiplied for `_premultiplied`).  The id,
bind group and sampler stay valid, so the next draw sees the new texels.

## Colour space and premultiplication

- The texture format is `Rgba8UnormSrgb`: wgpu **decodes sRGB to linear** on
  sample, and the output target is sRGB, so the write encodes back.
- `register_texture_2d` takes **straight-alpha sRGB** bytes.  The upload
  converts them once to **premultiplied linear light** and re-encodes them to
  sRGB for storage; the shader skips the second premultiply (the renderer marks
  the texture as premultiplied).
- Mip levels are filtered **in linear light on premultiplied values**.  A 2×2
  black/white checker becomes linear 0.5 (sRGB ~188), never the sRGB average
  (128).  Filtering the stored bytes directly would darken every downsampled
  image and produce dark fringes on translucent edges.
- The chain is deterministic: exact area weights per axis.  Even sizes average
  exactly 2×2 texels; odd sizes are resampled with the fractional box that
  covers every source texel (3×3 becomes one texel averaging all nine).  Each
  level is `max(1, w / 2)` by `max(1, h / 2)`, the mip count wgpu accepts.
  `ferrous_2d::render::build_mip_chain` is the pure CPU function and is tested
  without a device.

## Sampler choice

- Photos, UI images, anything scaled down: `photo()` (trilinear).
- Pixel art, sprite sheets, exact-texel tests: `pixel_art()` (nearest, no
  mips).
- A mipmapped texture with `filter: Nearest` keeps nearest magnification but
  uses linear minification/trilinear, which is the right choice for
  pixel-art-like images that still need to avoid aliasing when zoomed out.

The sampler clamps to the edge (`ClampToEdge`), so a sprite quad that samples
just outside `[0, 1]` repeats the border texel instead of wrapping.

## Cropping with `uv_rect`

`SpriteInstance::uv_rect` is `[x, y, width, height]` in UV space.  The quad's
`(0,0)` corner samples `(x, y)` and its `(1,1)` corner samples
`(x + width, y + height)`.  Cropping a 2×1 red/blue texture to the right half
is `uv_rect = [0.5, 0.0, 0.5, 1.0]`; with `pixel_art()` the sampled texel is
exactly blue.  The sprite model matrix scales the unit quad to the destination
size, so the crop and the destination rectangle are independent.

## Budget and limits

- `Renderer::texture_bytes_2d()` is the total resident bytes of every
  registered 2D texture, **mip levels included**.  `free_sprite_texture` removes
  the bind group, the batcher bucket and the byte count, and recycles the id.
- `register_texture_2d` returns a typed `TextureError` instead of panicking:
  - `ZeroSize` for a zero width or height;
  - `SizeMismatch { expected, actual }` when the slice is not `w * h * 4`;
  - `TooLarge { width, height, limit }` above the device's
    `max_texture_dimension_2d`.
- The legacy registrations are counted in the same budget; they are not
  mipmapped.

## Layers, masks and effects

Sprites go through the same painter-order command stream as shapes and text.
Inside an offscreen layer they therefore obey group opacity, the layer blend
mode, the effect chain (drop shadow, blur, glow) and alpha/luma masks.  A drop
shadow is built from the **layer's alpha**, so a texture's transparent texels
cast no shadow; a masked layer clips the sprite and its shadow together.

The tests in `crates/ferrous_renderer/tests/pure2d_images.rs` cover the checker
mip, odd-size chains, premultiplied half-transparent texels, exact `uv_rect`
crops, a sprite inside a 50 % layer, a sprite inside a masked layer, a drop
shadow following texture alpha, the byte budget and the oversized-texture
error.
