# Temporal accumulation

`Accumulation2d` averages several completed sub-frames on the GPU in **linear
light**. It is the generic primitive behind motion blur, supersampling and any
other "render N times and average" effect; nothing in the API knows what the
sub-frames mean.

## Why not average on the CPU

Reading each sub-frame back as 8-bit sRGB and averaging on the CPU loses
precision twice: the per-sub-frame quantisation and the average itself. It also
stalls the GPU once per sub-frame. The accumulator keeps premultiplied linear
values in `Rgba16Float` (the core WebGPU render-attachment format), adds each
sub-frame with a weighted additive blend and resolves once at the end.

## API

```rust,ignore
renderer.begin_accumulation_2d()?;

for weight in weights {
    // draw the sub-frame normally
    renderer.render_to_texture_view(&mut encoder, &target, &view, None);
    queue.submit(Some(encoder.finish()));
    renderer.accumulate_frame_2d(weight)?;
}

renderer.resolve_accumulation_2d()?;
// read_pixels now returns the weighted average
```

`Pure2dHarness` mirrors the three calls and resolves into its own offscreen
target, so `read_pixels_into` returns the result. `RenderContext` mirrors them
for the windowed facade.

The sequence is explicit:

| Call | Effect |
|---|---|
| `begin_accumulation_2d` | Clears the accumulator to zero, resets the weight sum. |
| `accumulate_frame_2d(weight)` | Adds the current output target, scaled by `weight`. |
| `resolve_accumulation_2d` | Writes `sum / total_weight` into the normal output target and closes the sequence. |

## Semantics

- The source is sampled with `textureSample`; an sRGB source is decoded to
  linear light by the sampler. The accumulator therefore stores premultiplied
  **linear** values, and the resolve writes linear values into the target (wgpu
  re-encodes an sRGB target).
- Weights are normalised by their exact sum. `0.5/0.5` of black and white is
  linear 0.5, which encodes to sRGB 188, not 128.
- The order of operations is fixed: clear, then one full-screen triangle per
  sub-frame in call order, then one resolve triangle. Identical calls produce
  byte-identical output on the same adapter.
- A static scene accumulated N times is identical to one render: every sample
  has the same value and the normalisation divides by the same total.
- A weight of zero contributes nothing but is allowed; the total must stay
  positive by resolve time.

## Errors

`AccumulationError` is typed; the API never panics on misuse:

| Variant | Cause |
|---|---|
| `NotBegun` | `accumulate` or `resolve` before `begin`. |
| `ZeroWeight` | The weights sum to zero at resolve. |
| `InvalidWeight` | A negative or non-finite weight. |
| `TooLarge` | The target exceeds `max_texture_dimension_2d`. |
| `Unsupported` | The renderer is not in `RendererMode::Pure2D`. |

## Tests

`ferrous_renderer/tests/pure2d_accumulation.rs` covers the exact linear average
(black/white, 4 sub-frames, uneven weights), determinism, static-scene equality
and every error case.
