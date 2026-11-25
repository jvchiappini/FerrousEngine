# Backend contract

GUIMaker is headless by design. The core produces a `DisplayList`; a host maps
that list onto an engine batch, owns the GPU resources and routes input. The
reference adapter is `guimaker_gallery::host::GuiHost<A>`, and
`guimaker_ferrous` is the display-list mapping. This page states what a host
must provide, so a new backend can be written without reading the gallery.

## What a host must implement

A host chooses one of two shapes:

- implement `FerrousApp` directly and call `guimaker_ferrous::encode` from
  `draw_ui`; or
- implement `GuiApp` (`crates/guimaker_gallery/src/host.rs:83`) and wrap it in
  `GuiHost::new` (`host.rs:235`), which implements `FerrousApp` for you.

`GuiApp` is one model:

```text
type Msg: Clone
fn view(&self) -> Element<Msg>
fn update(&mut self, message: Msg)
fn theme(&self) -> Theme            // defaults to Theme::dark()
fn time_ms(&self) -> Option<f64>    // defaults to the engine frame clock
```

The adapter implements these `FerrousApp` callbacks:

| Callback | Contract |
|---|---|
| `setup` | derive `viewport = physical_size / scale`, call `Ui::set_viewport(viewport, scale)` and apply the theme |
| `update` | advance the injected clock, paint `view()` into `Frame`, drain `Frame::messages` into `GuiApp::update`, sync popups and request a redraw when an animation or wake time is pending |
| `draw_ui` | encode the frame's display list into `dc.gui` with `dc.font`; never create or upload the atlas |
| `on_window_event` | translate winit input to `guimaker::InputEvent` and call `Ui::push_event` |
| `on_resize` | recompute the logical viewport immediately |
| `draw_popup` | encode one popup's command slice at the popup's own scale (see *Popups*) |
| `on_popup_event` | translate popup-local input back into main-window coordinates before routing |
| `on_popup_screenshot` / `on_screenshot` | optional evidence hooks; no drawing policy |

The library never reads the wall clock. A host passes the time with
`Ui::set_time_ms(now_ms)`; `GuiHost` uses `GuiApp::time_ms` when it returns
`Some`, otherwise the engine frame clock.

## The display list to `GuiBatch`

`guimaker::Ui::paint` (`crates/guimaker/src/frame.rs:349`) returns a flat list.
The stateless entry point is:

```text
guimaker_ferrous::encode(display_list, viewport, scale, &mut GuiBatch, &Font)
```

`crates/guimaker_ferrous/src/mapper.rs:735` builds a temporary
`GuiMakerRenderer` with an empty texture map, so `Image` commands without a
registered texture are skipped. Use `GuiMakerRenderer::encode`
(`mapper.rs:136`) or `encode_commands` when a frame can contain images or when a
popup draws a translated slice. The mapping is scaled by `scale` exactly once,
text is shaped with `ferrous_font`, and every primitive carries its `NodeId`.

The detailed mapping (rounded rects, borders, shadows, gradients, text, images,
icons, clips, layers and the backdrop-blur request) and every remaining
fallback live in [guimaker_ferrous/backend.md](../guimaker_ferrous/backend.md).
The engine owns the shader and the glyph atlas; the adapter owns no GPU
resource and never uploads a texture it did not receive in the command.

## Popups

Popups are **opt-in and host-driven**. The library returns popup work in
`Frame::popups`; a host decides whether the platform can show a popup window.
`GuiHost` only opens one for an overlay that exceeds the window when the app
reports desktop capabilities, and it keeps that decision inside the host.

The engine side is `ferrous_engine::windows::PopupQueue`:

```text
ctx.popups.open(PopupSpec { .. }) -> PopupId
ctx.popups.show(id, position)
ctx.popups.move_to(id, position)
ctx.popups.resize(id, size)
ctx.popups.hide(id)
ctx.popups.redraw(id)
```

`GuiHost::sync_popups` (`host.rs:328`) diffs this frame's plan against the open
popups, and `draw_popup` (`host.rs:636`) translates the overlay commands to the
popup origin and encodes them with `encode_commands`. The engine calls
`draw_popup` only when a popup asked for a frame, so an idle popup presents zero
frames.

On Windows the default present backend is `PopupPresent::Layered`; the
swapchain path remains the fallback and is forced with
`FERROUS_POPUP_PRESENT=swapchain`. Hidden pooled windows are capped at
`POPUP_POOL_CAP = 2` (`crates/ferrous_engine/src/windows.rs:41`). Pre-warming a
hidden popup is an explicit app choice, off by default; its measured cost,
benefit and pool interaction are in [prewarming.md](prewarming.md).

## DPI

All layout is in logical pixels; the host supplies the physical scale factor.

- `viewport = physical_window_size / scale`; pass it and `scale` to
  `Ui::set_viewport` and to `encode`.
- Geometry, radii, border widths, shadows, font size and clip rectangles are
  multiplied by `scale` in the adapter. Layout snaps final edges to the physical
  grid, so a hairline stays one device pixel at 1x, 1.25x, 1.5x and 2x.
- `ScaleFactorChanged` updates the adapter's scale; the next `update` recomputes
  the viewport and re-lays out.
- A popup uses its **own** window scale, not the main window's: a menu on a
  second monitor must size its text for that monitor. The main window's
  overlay commands are translated to the popup origin first.
- `dc.ctx.scale_factor` and `dc.ctx.window_size` are the values the engine
  passes for the window being drawn; use them rather than caching the main
  window's scale in a popup path.

## Input routing

A host translates platform events; GUIMaker owns gestures, focus, shortcuts,
scrolling and text editing. The gallery mapping (`host.rs:496`) is the
reference:

| winit event | `InputEvent` |
|---|---|
| `CursorMoved` | `PointerMoved`, physical position divided by scale |
| `CursorLeft` | `PointerLeft` |
| `MouseInput` | `PointerDown` / `PointerUp` at the last pointer |
| `MouseWheel` line delta | `Wheel { kind: Line }` |
| `MouseWheel` pixel delta | `Wheel { kind: Pixel }`, scaled to logical |
| `KeyboardInput` named key | `KeyDown` / `KeyUp`, `repeat` preserved |
| `KeyboardInput` text | `Text`, control characters dropped |
| `Ime::Commit` | `Text` |
| `ModifiersChanged` | `Modifiers` |
| `Focused(false)` | `FocusLost` |
| `ScaleFactorChanged` | updates the scale; next frame re-lays out |

Pointer moves are coalesced to the latest before routing, so a 1000 Hz mouse
costs one route per frame. `paint` routes queued events, returns typed messages
and paints the new state in the same frame; there is no one-frame lag.
