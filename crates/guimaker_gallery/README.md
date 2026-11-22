# guimaker_gallery - stories, screenshots, goldens and the host adapter

`guimaker_gallery` is the Phase 5 application layer of GUIMaker:

- **stories** - named screens (`start`, `components`) built with the real
  `guimaker::Ui` at a requested viewport and scale;
- **headless screenshots** - a story becomes a PNG through the real GUI
  renderer, with no window and no swapchain;
- **tolerance goldens** - committed PNGs compared with a per-channel and
  pixel-count budget, with a reason-required update command;
- **host adapter** - about one screen of glue that implements
  `ferrous_engine::FerrousApp` around any `GuiApp` model.

## Quick start

```text
cargo run -p guimaker_gallery -- stories

cargo run -p guimaker_gallery -- shot start --size 1280x800 --scale 1.5 \
    --theme light --out out/p5-start-light.png

cargo run -p guimaker_gallery -- shot start --size 1280x800 --scale 1.5 \
    --theme dark --out out/p5-start-dark.png

cargo run -p guimaker_gallery -- golden start --size 1280x800 --scale 1.5 \
    --theme light
```

`shot` writes the PNG and prints its absolute path. `--size` is the **logical**
viewport; the PNG is physical, `round(logical * scale)` per axis. The example
above is 1920x1200.

The windowed demo:

```text
cargo run -p guimaker_gallery --example host
```

It puts the start-screen recipe inside a real `FerrousApp` and prints every
action to stderr.

## The host adapter

`guimaker_gallery::host::GuiHost<A>` wraps a `GuiApp`:

```rust
pub trait GuiApp {
    type Msg: Clone;
    fn view(&self) -> Element<Self::Msg>;
    fn update(&mut self, message: Self::Msg);
    fn theme(&self) -> Theme { Theme::dark() }
    fn time_ms(&self) -> Option<f64> { None }
}
```

`GuiHost` implements `FerrousApp`:

| callback | what it does |
|---|---|
| `setup` | sets the logical viewport and scale from the window, applies the theme |
| `update` | advances the injected clock, paints `view()` into a `Frame`, drains messages into `update` |
| `draw_ui` | runs `guimaker_ferrous::encode` from the frame's display list into `dc.gui` |
| `on_window_event` | maps winit pointer/wheel/key/text/modifier/focus/scale events to `guimaker::InputEvent` |
| `on_resize` | recomputes the logical viewport |

Physical coordinates are divided by the window scale factor for routing;
`draw_ui` multiplies back by the same scale. The adapter owns no GPU resource:
the engine builds and binds the font atlas, and `dc.font` is the atlas the
display list is encoded against.

## Goldens and the exact tolerance

Goldens live in `crates/guimaker_gallery/goldens/`:

- `start-light-1280x800-1.5.png`
- `start-dark-1280x800-1.5.png`
- `components-light-1280x800-1.5.png`

A comparison uses `Tolerance { max_channel_delta, max_diff_pixels }`:

- **max_channel_delta = 12** (8-bit steps, per R/G/B/A channel). A pixel whose
  largest channel delta is at most 12 counts as equal.
- **max_diff_pixels = ceil(0.002 * pixels)**, with a floor of 1. That is
  **0.2 % of pixels**. For 1920x1200 this is **4608 pixels**.

Both the absolute count and the fraction are printed; a size mismatch is an
error, never a pass. The `goldens/UPDATE_LOG.md` file records every rewrite and
the reason. Updating requires the reason:

```text
cargo run -p guimaker_gallery -- golden start --size 1280x800 --scale 1.5 \
    --theme light --update --reason "why this golden changed"
```

`cargo test -p guimaker_gallery` re-renders the same three images and checks
them with the tolerance above. On a machine without a headless GPU adapter the
golden test skips with a message instead of reporting a false regression.

## Fallbacks

- The stateless `guimaker_ferrous::encode` skips `Image` commands without a
  registered texture. Gallery stories use no images; a host that needs images
  uses `GuiMakerRenderer` + `TextureMap` instead.
- Rounded clips are drawn as rectangular scissors and `ClipPush` radii are
  ignored; `BackdropBlur` is skipped. The gallery stories do not rely on
  either. See `crates/guimaker_ferrous/docs/backend.md`.
- If the headless device or readback fails, the CLI reports a GPU error and the
  golden test skips; it never writes a fake pass.
- Text uses the bundled `Roboto-Regular` for both layout
  (`FerrousTextSystem`) and the glyph atlas, so metrics and UVs always agree.
  A host can install richer faces with `FerrousTextSystem::with_source`.
