# Gallery, screenshots and goldens

`guimaker_gallery` is the application layer of GUIMaker. It has no drawing
policy of its own: it builds a story with `guimaker`, maps the display list
with `guimaker_ferrous`, and borrows the engine's off-screen target and
readback.

## The CLI

```text
cargo run -p guimaker_gallery -- <command> [options]
```

| Command | Purpose |
|---|---|
| `stories` | list the story names |
| `shot <story>` | render a PNG and print its absolute path |
| `golden <story>` | compare with a committed golden, or rewrite it |
| `help` | usage |

Options (shared):

| Option | Default | Meaning |
|---|---|---|
| `--size WxH` | `1280x800` | logical viewport |
| `--scale F` | `1` | logical-to-physical factor |
| `--theme light\|dark\|high-contrast` | `dark` | built-in theme |
| `--out PATH` | see below | output PNG |
| `--check PATH` | - | compare against this PNG (`shot`) or compare path (`golden`) |
| `--update` | - | rewrite the golden (`golden` only) |
| `--reason TEXT` | - | required with `--update`, recorded in `goldens/UPDATE_LOG.md` |

The physical PNG is `round(logical * scale)` per axis. `shot` with
`--size 1280x800 --scale 1.5` writes 1920x1200 pixels.

Default output paths:

- `shot`: `out/<story>-<theme>-<W>x<H>-<scale>.png`
- `golden`: `crates/guimaker_gallery/goldens/<story>-<theme>-<W>x<H>-<scale>.png`

Examples:

```text
cargo run -p guimaker_gallery -- shot start --size 1280x800 --scale 1.5 \
    --theme light --out out/p5-start-light.png

cargo run -p guimaker_gallery -- shot start --size 1280x800 --scale 1.5 \
    --theme dark --out out/p5-start-dark.png

cargo run -p guimaker_gallery -- shot components --size 1280x800 --scale 1.5 \
    --theme light --out out/p5-components-light.png
```

## The screenshot pipeline

1. `Story::display_list(theme, logical, scale)` builds `guimaker::Ui`,
   calls `set_viewport(logical, scale)`, `set_time_ms(0)`, paints the view with
   `FerrousTextSystem`, and returns the flat display list.
2. `shot::render_story` loads the bundled `Roboto-Regular` atlas and maps the
   list with `guimaker_ferrous::encode` into a `ferrous_ui_render::GuiBatch`.
3. A `ferrous_ui_render::GuiRenderer` draws the batch into the
   `Rgba8UnormSrgb` texture of a `ferrous_renderer::headless::Pure2dHarness`.
4. The harness reads the texture back into tightly packed RGBA8 and
   `shot::write_png` stores it as an 8-bit RGBA PNG.

The harness is used for its device, queue, off-screen target and persistent
readback staging buffer; the Pure2D renderer itself is not drawn. Nothing in
the path creates a window or a surface, so it works on a headless machine.

The clear colour is the theme's `Bg` role converted with `Color::to_linear`,
which is the same linear value the shader receives; `Rgba8UnormSrgb` performs
the sRGB transfer on store, so the cleared pixels match the painted backdrop.

## Stories

| Story | Source |
|---|---|
| `start` | `guimaker::recipe::start_screen()` |
| `components` | a local board exercising actions, tones, surfaces and dividers |

A story is pure in `(theme, viewport, scale)` and touches no GPU. The same
function is used by the CLI, the golden test and the windowed host example.

## The `FerrousApp` host adapter

`guimaker_gallery::host::GuiHost<A>` is the engine glue. The app implements one
trait:

```rust
pub trait GuiApp {
    type Msg: Clone;
    fn view(&self) -> Element<Self::Msg>;
    fn update(&mut self, message: Self::Msg);
    fn theme(&self) -> Theme { Theme::dark() }
    fn time_ms(&self) -> Option<f64> { None }
}
```

Then:

```rust
App::new(GuiHost::new(MyModel::default()))
    .with_size(1280, 800)
    .with_mode(AppMode::Flat2D)
    .with_font_bytes(include_bytes!("../../../assets/fonts/Roboto-Regular.ttf"))
    .run();
```

`GuiHost` implements every relevant `FerrousApp` callback:

- `setup` and `update` derive the logical viewport from
  `ctx.window_size / ctx.window.scale_factor()` and call
  `Ui::set_viewport`. The theme is applied only when `GuiApp::theme` changes,
  so an unchanged theme does not force a relayout.
- `update` sets the injected clock (`GuiApp::time_ms`, or
  `ctx.time.elapsed * 1000` when the app returns `None`), paints `view()` into
  an owned `Frame`, then feeds every routed message back to
  `GuiApp::update`.
- `draw_ui` calls
  `guimaker_ferrous::encode(&frame.display_list, viewport, scale, dc.gui, dc.font)`.
  The engine owns the font atlas; the adapter never uploads anything.
- `on_resize` recomputes the logical viewport immediately.
- `on_window_event` translates winit input; `GuiHost::translate_event` is
  public so a host can unit-test the mapping.

### Input mapping

| winit event | `guimaker::InputEvent` |
|---|---|
| `CursorMoved` | `PointerMoved`, physical divided by scale |
| `CursorLeft` | `PointerLeft` |
| `MouseInput` (left/right/middle) | `PointerDown` / `PointerUp` at the last pointer |
| `MouseWheel` line delta | `Wheel { kind: Line }` |
| `MouseWheel` pixel delta | `Wheel { kind: Pixel }`, scaled to logical |
| `KeyboardInput` named or character | `KeyDown` / `KeyUp` (`repeat` preserved) |
| `KeyboardInput` text | `Text`, control characters dropped |
| `Ime::Commit` | `Text` |
| `ModifiersChanged` | `Modifiers` |
| `Focused(false)` | `FocusLost` |
| `ScaleFactorChanged` | updates the adapter's scale; the next `update` resizes the UI |

## Goldens

Goldens under `crates/guimaker_gallery/goldens/`:

- `start-light-1280x800-1.5.png`
- `start-dark-1280x800-1.5.png`
- `components-light-1280x800-1.5.png`

Comparison is tolerance based:

| Field | Gallery value |
|---|---|
| `max_channel_delta` | `12` 8-bit steps, per channel |
| `max_diff_pixels` | `ceil(0.002 * pixels)`, floor `1` (0.2 %) |
| For 1920x1200 | `4608` pixels |

A pixel counts as different only when its **largest** R/G/B/A delta exceeds
`max_channel_delta`; the reported `max_channel_delta` is the largest delta over
the whole image. The report also includes the differing pixel count and the
fraction. Dimensions must match exactly.

Update a golden with a reason (the command refuses an empty reason):

```text
cargo run -p guimaker_gallery -- golden start --size 1280x800 --scale 1.5 \
    --theme light --update --reason "intentional spacing change"
```

Check one without writing:

```text
cargo run -p guimaker_gallery -- golden start --size 1280x800 --scale 1.5 \
    --theme light
```

`cargo test -p guimaker_gallery` runs
`tests/goldens.rs::committed_goldens_match_the_current_render`, which
re-renders the three cases and applies the same tolerance. Without a headless
GPU adapter the test prints a skip message; with one it is a real comparison.

## Fallbacks and limits

- `guimaker_ferrous::encode` is the stateless entry point and skips `Image`
  commands without a registered texture. Stories use no images; a host that
  needs them builds a `GuiMakerRenderer` with a `TextureMap`.
- Rounded clips are rectangular scissors; `BackdropBlur` is skipped by the
  pinned shader. The stories avoid both.
- Text is the bundled Roboto for layout and atlas, so a glyph's metrics and UV
  always come from the same face.
- The headless path needs a working wgpu adapter (hardware or a software
  fallback). If none exists, `shot` and `golden` return a GPU error and the
  golden test skips.
