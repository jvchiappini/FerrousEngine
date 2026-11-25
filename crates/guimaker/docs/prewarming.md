# Popup pre-warming

Popups are created lazily: the first time an overlay is hosted in a popup
window, the host creates the OS window, its `wgpu::Surface` and its
`GuiRenderer`. That creates the first-open cost, and it is where a menu that
feels instant on a pooled reopen feels slow the very first time.

Pre-warming is the opt-in opposite: build one hidden popup (window, surface and
renderer) while the app is idle, before the user asks for it. Only `show` plus
the first frame remain on the input path. It is **off by default**; nothing in
the library pre-warms on its own.

## The switch

There is no automatic pre-warm and no environment variable that changes this
default. The library's popup pool only reuses windows that an app has already
opened and hidden. An app opts in by running the normal open path while the
popup is still hidden, then showing it later:

1. `PopupQueue::open` the popup at idle; it creates without showing.
2. Drain the queue so the backend creates the window and surface.
3. Create and register the popup's `GuiRenderer` (sharing the main renderer's
   pipelines when possible).
4. `PopupQueue::show` only when the user actually opens the menu.

The Phase 8E evidence harness exposes exactly that sequence as the
`--prewarm` switch:

```text
cargo run --release -p ferrous_engine --example p8e_popup_cost -- --mode=sharing --prewarm
```

Without `--prewarm` the harness measures the cold first open; with it, the
hidden popup is built during the idle period and the measured first open is
show plus first frame.

## Measured cost and benefit

Reference: release build, Windows host otherwise idle, 320x200 transparent
popup on an 800x600 main window, `--mode=sharing`. RSS is
`PROCESS_MEMORY_COUNTERS::WorkingSetSize`.

| Measurement | Cold first open | Pre-warmed |
|---|---:|---:|
| Hidden popup cost at idle | 0 | **+10.6 MB RSS, 69.8 ms once** |
| First open (input to present) | **69.6 ms** | **3.0 ms** |

The hidden window is the cost: about 10.6 MB of RSS (a swapchain plus its
surfaces and the popup renderer). The benefit is a 23x lower first-open
latency, but only for the first open of that popup. Every later open is served
by the pool and costs about 3 ms whether it was pre-warmed or not.

## Pool cap interaction

The hidden popup pool keeps at most **two** windows (`POPUP_POOL_CAP = 2`), the
common "menu then submenu" case. A hidden pre-warmed popup occupies one of
those two slots. Hiding a third popup destroys the oldest hidden one, so a
pre-warm does not survive an app that keeps opening more than two popups at
once: the window is destroyed by normal pool trimming, and the next open is
cold again. Do not pre-warm more than the pool can hold; worst-case hidden RSS
is therefore about 21 MB for two pre-warmed popups.

## Why it is off by default

- It trades **10.6 MB of idle RSS and 69.8 ms of idle work** for a benefit that
  only an app that opens popups actually uses. An app that never opens a menu
  pays the cost for nothing.
- The per-popup RSS budget is about 8 MB, and a single hidden popup is already
  10.6 MB. Pre-warming is the wrong default when memory is tight.
- Reopening an already-used popup is already cheap (about 3 ms) through the
  pool. Pre-warming only helps the very first open.
- The right default is pay-as-you-open. Turn pre-warming on explicitly when the
  first-open latency target matters more than the idle memory, and pre-warm
  exactly one popup.
