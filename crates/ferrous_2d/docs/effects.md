# Layer effects: blur, drop shadow and glow

Effects are a property of an offscreen [`Layer2d`](blend.md) — never of a
single draw.  The layer renders its draws into its texture as usual, the effect
chain runs in order over that texture, and the result composites into the
parent with the layer's group opacity and `CompositeMode2d`.

```rust,ignore
renderer.push_layer_2d_with_effects(
    Layer2d::new(1.0, CompositeMode2d::Normal, depth),
    &[Effect2d::DropShadow {
        color: [0.0, 0.0, 0.0, 1.0],
        offset: [8.0, 10.0],
        blur: 8.0,
        opacity: 0.6,
    }],
)?;
// draws between push and pop are affected as one group
renderer.pop_layer_2d()?;
```

`push_layer_2d` is `push_layer_2d_with_effects` with an empty chain.  A layer
carries at most `MAX_EFFECTS_PER_LAYER` (8) effects; more is
`LayerError::TooManyEffects`, and a non-finite or negative parameter is
`LayerError::InvalidEffect` with the offending index.

## Chain semantics

Each effect reads the previous image and produces the next one, so the order
matters.  All textures are linear and premultiplied.

| effect | what it does |
|---|---|
| `Blur { radius }` | replaces the image with its Gaussian blur |
| `DropShadow { color, offset, blur, opacity }` | blurs the image, tints it, offsets it and puts it behind the image |
| `Glow { color, radius, intensity }` | blurs the image, tints it and adds it as light around the image (outer glow) |

For a drop shadow, with coverage `As` of the blurred image:

```text
shadow.rgb = color.rgb * color.a * opacity * As
shadow.a   = color.a * opacity * As
out        = content + shadow * (1 - content.a)
```

`offset` is in pixels, Y down; a positive X moves the shadow right.  For a
glow:

```text
glow.rgb = color.rgb * color.a * intensity * As
out      = content + vec4(glow.rgb, 0) * (1 - content.a)
```

The glow is an outer glow: it adds light only where the content is not
opaque, so an opaque shape keeps its own colour and gets a halo.  It does not
change coverage and can exceed 1 on the `Rgba16Float` layer target; the final
composite into an 8-bit target clamps.

## The blur kernel

`BlurKernel2d::new(radius)` (or the `gaussian_kernel` shorthand) is a pure,
deterministic function of the radius:

- `sigma = radius / 2`, the convention of CSS `box-shadow` and canvas
  `shadowBlur` (a radius of 10 looks like `box-shadow: 0 0 10px`);
- `taps = ceil(1.5 * radius) + 1` one-sided samples (three standard
  deviations), clamped to `MAX_BLUR_TAPS` (76); radii clamp at
  `MAX_BLUR_RADIUS` (48);
- weights `exp(-i^2 / (2*sigma^2))`, normalized so the centre plus both wings
  sum to 1;
- `radius = 0` (or a non-finite radius) is the identity: one tap of weight 1.

The kernel is uploaded as a uniform; the GPU never recomputes it.  Blurring is
separable: one horizontal and one vertical full-target pass, so the cost is
`2 * (2*taps - 1)` samples per pixel instead of the square of that.

## Render passes and memory

With `n` effects the renderer runs at most `3n` passes (two blur passes plus
one composite for a shadow or glow) plus one full-target copy at the end, and
reuses two full-target scratch textures per layer.  The layer's own texture is
the chain's starting point and its final destination, so the existing composite
path and its cached bind group are unchanged.

## Determinism

The kernel is fixed for a given radius, the pass order is fixed, and there is
no randomness or wall clock anywhere in the path.  The same document and frame
produce the same pixels on the same adapter; GPU differences are bounded by the
project's golden tolerance (no channel differs by more than 2, at most 0.1 % of
pixels above 2).
