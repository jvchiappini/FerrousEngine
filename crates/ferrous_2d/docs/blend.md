# Blending and offscreen layers

Pure2D renders in **linear light with premultiplied alpha**.  Two independent
mechanisms decide how a draw meets the pixels already in the target:

- a **per-draw blend mode** (`BlendMode2d`), applied by fixed-function GPU blend
  state;
- an **offscreen layer** (`Layer2d`) that renders a group of draws into a
  texture and then composites the finished texture with `CompositeMode2d`.

## Per-draw modes (`BlendMode2d`)

| mode | colour (premultiplied) | alpha |
|---|---|---|
| `Normal` | `Cs + Cd*(1-As)` | `As + Ad*(1-As)` |
| `Add` | `Cs + Cd` (clamped by the target) | `As + Ad*(1-As)` |
| `Multiply` | `Cs*Cd + Cd*(1-As)` | `As + Ad*(1-As)` |
| `Screen` | `Cs + Cd*(1-Cs)` | `As + Ad*(1-As)` |

Set it with `Renderer::set_blend_mode_2d`; it applies to every following 2D draw
until changed and resets to `Normal` at the start of each frame.

## Layers (`Layer2d`)

```rust,ignore
renderer.push_layer_2d(Layer2d::new(0.5, CompositeMode2d::Multiply, depth))?;
// draws between push and pop are z-sorted among themselves
renderer.pop_layer_2d()?;
```

A layer renders into a transparent, target-sized offscreen texture and is
composited into its parent as one command at `depth`, with group opacity and
`CompositeMode2d`.

Group opacity is applied once to the finished layer, so two overlapping opaque
children inside a 50 % layer show no internal overlap.

## Composite modes (`CompositeMode2d`)

| mode | blend `B(b, s)` | needs backdrop |
|---|---|---|
| `Normal` | `s` | no |
| `Add` | `s + b` | no |
| `Multiply` | `s * b` | no |
| `Screen` | `s + b - s*b` | no |
| `Overlay` | `s < 0.5 ? 2*b*s : 1 - 2*(1-b)*(1-s)` | **yes** |
| `Darken` | `min(b, s)` | **yes** |
| `Lighten` | `max(b, s)` | **yes** |

`b` is the straight backdrop colour, `s` the straight source colour.  The full
composite is

```text
Co = Cs*(1-Ad) + As*Ad*B(b, s) + Cd*(1-As)
Ao = As + Ad*(1-As)
```

The first four modes use the same fixed-function states as per-draw blending.
The backdrop-aware modes run a dedicated full-target pass whose fragment shader
samples both the layer and a **snapshot of the destination**.  A render pass
cannot sample its own attachment, so the renderer:

1. draws every plan segment up to the composite;
2. copies the destination into a scratch texture (`COPY_SRC` on the destination
   is required);
3. runs the composite pass with the snapshot bound and blending disabled.

This split costs one full-target copy and one full-target pass per
backdrop-aware composite and happens only when such a mode is used.  Layers
nest up to `MAX_LAYER_DEPTH_2D` (8).

### Requirements and limits

- The render destination must be a texture with `COPY_SRC` (headless targets
  already have it; the engine configures surfaces with it when the adapter
  allows).  Rendering with `Renderer::render_to_view` (a view without its
  texture) falls back to `Normal` for backdrop-aware composites; use
  `Renderer::render_to_texture_view` when they are needed.
- The target must be single-sampled.  Pure2D targets are.
- Layer textures use `Rgba16Float` when the adapter can blend it and fall back
  to the target format otherwise.

## Choosing between the two

Use a per-draw mode for a self-contained effect (a glow that adds, a shadow
that multiplies).  Use a layer when the blend must apply to a **group** of
draws, when group opacity is needed, or when the mode is backdrop-aware.
