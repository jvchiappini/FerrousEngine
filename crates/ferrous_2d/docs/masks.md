# Layer masks: alpha and luma

A mask multiplies a finished offscreen layer by a per-pixel coverage before the
layer composites into its parent.  It is the second consumer of the layer
machinery after [effects](effects.md), and it composes with both effects and
blend modes.

```rust,ignore
renderer.push_layer_2d_masked(
    Layer2d::new(1.0, CompositeMode2d::Normal, depth),
    &[],                                   // optional effect chain
    Mask2d::new(MaskMode2d::Alpha),
)?;
// content drawn here is the masked picture
renderer.begin_mask_2d()?;
// the mask source is drawn here, into its own offscreen layer
renderer.end_mask_2d()?;
renderer.pop_layer_2d()?;
```

The mask source is not drawn by itself, and it may contain layers.  That is how
a feathered mask is built: draw the source inside a child layer that carries a
`Blur` effect, exactly like any other layer effect:

```rust,ignore
renderer.begin_mask_2d()?;
renderer.push_layer_2d_with_effects(Layer2d::default(), &[Effect2d::Blur { radius: 6.0 }])?;
// hard-edged mask geometry
renderer.pop_layer_2d()?;
renderer.end_mask_2d()?;
```

## Formulas

All layer textures are linear and premultiplied.  With content `c` and mask `m`:

```text
coverage = mode == alpha ? m.a : dot(m.rgb, vec3(0.2126, 0.7152, 0.0722))
coverage = invert ? 1 - coverage : coverage
out      = c * coverage
```

Scaling all four channels is the correct premultiplied multiply: the masked
image stays premultiplied, and the layer's group opacity and
[`CompositeMode2d`](blend.md) apply afterwards.  The order is therefore
**content → effect chain → mask → opacity/blend**, which means a drop shadow is
clipped by the mask, not the other way round.

Luma mode reads the premultiplied colour, so a fully transparent mask pixel
reads as black: coverage 0, or 1 when the mask is inverted.

## Errors

| caller mistake | error |
|---|---|
| `begin_mask_2d` with no masked layer waiting for a source | `MaskSourceNotAllowed` |
| `end_mask_2d` with no mask source open | `MaskSourceMismatch` |
| `pop_layer_2d` while a mask source is open | `MaskSourceNotClosed` |
| popping a masked layer before drawing its source | `MaskWithoutSource` (the layer composites unmasked so its content is not lost) |

Mask sources count towards `MAX_LAYER_DEPTH_2D`, and a mask composite needs one
scratch full-target texture per masked layer (the effect scratch pool), so the
cost of a masked layer is a bit above an effect-free one.
