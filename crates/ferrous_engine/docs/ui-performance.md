# UI performance

The engine-side rules that keep a GUIMaker (or any `GuiBatch`) screen light.
The GUIMaker budgets are in its README; this page records what the engine does
to meet them and what it deliberately does not do.

## Idle costs nothing

`AppConfig::redraw_policy` defaults to `OnDemand`: the runner draws only when
an input, an app `request_redraw`/`redraw_at`, or a worker wake asks for it.
There is no timer tick, so an idle window presents one frame and then sleeps.
A focused caret is the one exception and wakes at the blink interval. See
`redraw.md`.

A pointer move that the app does not forward presents no frame
(`redraw.rs::event_requests_frame` and the `redraw` regression test). Pointer
oversampling is the app's decision, not the engine's.

## Damage, not a whole-window copy

`GuiBatch::damage_union` carries the union of the rectangles whose paint or
geometry changed. The runner compares it with the window and uses it only when
`AppConfig::persistent_ui` is on (see below). GUIMaker also exposes per-node
damage and per-overlay fingerprints, so a popup repaint does not depend on the
main window's frame.

## Buffers grow and stay

The GUI vertex, index and instance buffers are grown geometrically and never
shrunk per frame. Staging buffers are released after a readback. A batch that
outgrows its buffer reallocates once, not every frame; the memory report is
refreshed after a growth so a popup's `popup:{h}:instances` entry is truthful.

## One draw per segment

The GUI pass hoists pipeline, bind group, vertex/index buffer and scissor state
per segment and emits one draw call per texture/clip batch. Adjacent segments on
the same scissor and contiguous ranges are coalesced (`GuiBatch::extend_from`),
with a layer-start barrier. In the heavy-UI fixture this moved 62 draws / 38
segments to 23 draws / 14 segments, and the hover fixture from 73 to 25 buffer
binds.

## Glyph atlas on demand

The composite atlas is baked for the characters a frame actually draws and
extended when a fallback face is needed. Unused glyphs are evicted by the
per-atlas LRU; a Latin-only story pays nothing for CJK or emoji. A runtime
fallback bake swaps the atlas `Arc` once and both the main window and its popups
re-bind it, never per frame.

## Timestamp queries

`Renderer::enable_gpu_timings` creates one timestamp query set and reads it back
asynchronously with a lag; the per-frame counters and the per-pass times appear
in the frame trace. The headless render path never creates a query set, so a
`still`/`render` run cannot hang waiting for one.

## The persistent target

`PersistentUiTarget` reprojects the frame into a texture and blits only the
damaged rectangle. It is **off by default**: a measured A/B on the Motioner
heavy-UI and scrub fixtures showed a full repaint plus a full copy is 31.8 %
worse on scrub and 66.8 % worse on hover, because Motioner's damage was the whole
window at that point. It becomes a win only once an app feeds per-panel damage
and the damage fraction stays below the break-even
(`AppConfig::persistent_ui_break_even`, default 0.71). The option exists for
apps that can meet that condition; the engine does not switch it on.

Partial redraw, the scissored path and the break-even measurement are described
in `ferrous_ui_render/PARTIAL.md`.

## Popups

A layered popup renders into an offscreen texture on the shared device and
presents through `UpdateLayeredWindow`. The readback runs on a worker thread
(`LayeredReadback`), so a menu repaint never blocks the event loop on the main
window's GPU queue; the popup keeps its previous image until the mapped frame
arrives and the next callback uploads it. The swapchain backend remains
available through `FERROUS_POPUP_PRESENT=swapchain`. For the A/B measurement of
the old stall, `FERROUS_POPUP_SYNC_READBACK=1` forces the blocking readback on
every repaint (the first frame and evidence captures are always synchronous).

## Measured references

These are the numbers recorded in the prompt-18 P5/P6/P7 reports on the
reference machine; treat them as regression references, not a fresh run.

| Item | Before | After |
|---|---|---|
| GUI buffer binds (hover) | 73 | 25 |
| Segment count (heavy UI) | 38 | 14 |
| Draw calls (heavy UI) | 62 | 23 |
| 2D GUI depth target | 5.76 MB | disabled |
| Start-screen VRAM | 24.1 MB | 18.6 MB (then 7.6 MB with the lazy 2D target) |
| Idle frames (45 s tail) | continuous | 0 |
| Offscreen present (4K, 4 % damage) | 2.597 ms | 0.025 ms |
