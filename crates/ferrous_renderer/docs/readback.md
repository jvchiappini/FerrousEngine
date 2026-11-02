# Headless readback and the Pure2D harness

`ferrous_renderer::headless` renders Pure2D scenes into an off-screen texture
without a window or `Surface`. It is used by integration tests, the `divan`
benches, and consumers that export video frames.

## Formats

Only `Rgba8Unorm` and `Rgba8UnormSrgb` are readable. `Pure2dHarness::new`
rejects any other format with `HarnessError::UnsupportedFormat`; readback of a
hand-built `OffscreenTarget` with another format returns
`ReadbackError::UnsupportedFormat` instead of panicking.

## Typed, allocation-reusing reads

```rust
let mut out = Vec::new();
harness.read_pixels_into(&mut out)?;      // OffscreenTarget::read_rgba8_into
harness.read_pixel_checked(x, y)?;        // 1x1 copy into a small buffer
```

* `OffscreenTarget` owns a persistent, row-padded staging buffer created once.
  `read_rgba8_into` reuses it and the caller's `Vec` allocation (`resize`, then
  a row copy that strips the 256-byte padding).
* Errors are typed: `UnsupportedFormat`, `MapFailed { message }`,
  `ChannelClosed` (device lost), `SizeMismatch`, `OutOfBounds` and `Busy`.
* The legacy `read_rgba8` / `read_pixel` methods are thin panicking wrappers,
  kept so existing callers compile. New code should use the typed methods.

## Pipelined readback

```rust
harness.render_frame();
harness.submit_readback()?;   // records a copy into the next of two slots
// ... render frame N+1 ...
if harness.take_readback(&mut out)? { /* frame N's bytes */ }
```

The harness keeps a two-slot ring of persistent staging buffers. Frame N's copy
overlaps frame N+1's rendering and results come out in submission order with one
frame of latency. At most two copies may be in flight; a third `submit_readback`
returns `ReadbackError::Busy`. Both `submit_readback` and `take_readback` reuse
the slot buffers and the caller's `Vec`, so steady-state frames allocate no GPU
buffer and do not grow the output.

## Harness configuration

* `Pure2dHarness::new(w, h, format)` defaults to an opaque-black clear colour.
* `Pure2dHarness::with_clear_color(w, h, format, color)` and
  `set_clear_color` choose the background.
* `HarnessError` replaces the old `String` error (device creation or
  unsupported format).
* `register_font` / `draw_text` mirror the renderer's Pure2D text API;
  `measure_text` returns `ferrous_font::LineMetrics` without drawing.

GPU tests skip cleanly with a printed reason when no adapter is available, so
CPU-only CI passes without `#[ignore]`.

## Text

See `crates/ferrous_2d/docs/text.md` for the Pure2D text coordinate contract,
ordering and API.
