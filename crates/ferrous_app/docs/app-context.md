# `AppContext` Reference

`AppContext<'_>` is the handle passed to every [`FerrousApp`](ferrous-app-trait.md)
callback. It bundles all per-frame access an application needs.

```rust
pub struct AppContext<'a> {
    // ── Read-only ──────────────────────────────────────────────────
    pub input:        &'a InputState,
    pub time:         Time,
    pub window_size:  (u32, u32),
    pub window:       &'a Window,
    pub render_stats: RenderStats,
    pub camera_eye:   Vec3,
    pub camera_target: Vec3,

    // ── Read-write ─────────────────────────────────────────────────
    pub world:        &'a mut World,
    pub viewport:     Viewport,
    pub gizmos:       Vec<GizmoDraw>,
    pub render:       RenderContext<'a>,
    pub asset_server: &'a mut AssetServer,
}
```

---

## Input — `ctx.input`

`InputState` tracks keyboard and mouse state for the current frame.

```rust
// Keyboard
ctx.input.just_pressed(KeyCode::Space)   // true on the frame the key was pressed
ctx.input.key_held(KeyCode::ShiftLeft)   // true while the key is held down
ctx.input.just_released(KeyCode::A)

// Mouse
ctx.input.button_just_pressed(MouseButton::Left)
ctx.input.button_held(MouseButton::Right)
ctx.input.mouse_position()               // (f64, f64) in window coords
ctx.input.mouse_pos_f32()               // (f32, f32) — convenience cast for UI math
ctx.input.scroll_delta()                 // (f32, f32) — (x, y) lines this frame
```

Common `KeyCode` values: `Escape`, `Space`, `Enter`, `Backspace`, `Tab`,
`ArrowUp/Down/Left/Right`, `KeyA`–`KeyZ`, `Digit0`–`Digit9`, `F1`–`F12`.

---

## Time — `ctx.time`

```rust
pub struct Time {
    pub delta:   f32,   // seconds since last frame (~0.016 at 60 fps)
    pub elapsed: f64,   // total seconds since app start
    pub fps:     f32,   // smoothed frames per second
}
```

```rust
// Move something at a constant speed regardless of frame rate
position.x += speed * ctx.time.delta;
```

---

## Window info

```rust
ctx.window_size          // (u32, u32) — physical pixels
ctx.width()              // u32 shortcut
ctx.height()             // u32 shortcut
ctx.aspect()             // f32 — width / height
ctx.gpu_backend()        // &str — "Vulkan", "Dx12", "Metal", "WebGPU", etc.
```

### Moving the window

```rust
// Move the OS window so its top-left corner is at (x, y) in physical screen pixels.
ctx.set_window_position(x, y);

// Query the current position (returns None on platforms that don't support it).
if let Some((x, y)) = ctx.window_position() { … }
```

This is the primary way to implement a **custom drag-to-move title bar** when
you create a decorations-off window with `.with_decorations(false)`:

```rust
fn update(&mut self, ctx: &mut AppContext) {
    let (mx, my) = ctx.input.mouse_position();
    let (mx, my) = (mx as i32, my as i32);

    if ctx.input.button_just_pressed(MouseButton::Left) {
        // Record drag offset relative to the window's current position.
        if let Some((wx, wy)) = ctx.window_position() {
            if /* mouse is inside the title bar rect */ {
                self.drag_offset = Some((mx - wx, my - wy));
            }
        }
    }

    if ctx.input.button_held(MouseButton::Left) {
        if let Some((ox, oy)) = self.drag_offset {
            ctx.set_window_position(mx - ox, my - oy);
        }
    } else {
        self.drag_offset = None;
    }
}
```

> **Note** — on some Wayland compositors the OS ignores `set_window_position`
> silently.  On Windows, macOS and X11 it works as expected.

### Resizing the window

When `with_decorations(false)` is used you also lose the OS resize handles.
Call `start_window_resize` on the frame the user presses the left button while
hovering a resize edge/corner you drew with `ferrous_gui`.  The OS takes over
the rest of the interaction — no mouse-delta tracking needed.

```rust
use ferrous_app::WindowResizeDirection;

// Typical 8-zone hit test (8 px edge zone)
fn resize_direction(mx: f32, my: f32, w: u32, h: u32) -> Option<WindowResizeDirection> {
    const E: f32 = 8.0;
    let (w, h) = (w as f32, h as f32);
    match (mx < E, mx > w - E, my < E, my > h - E) {
        (true,  false, true,  false) => Some(WindowResizeDirection::NorthWest),
        (false, true,  true,  false) => Some(WindowResizeDirection::NorthEast),
        (true,  false, false, true)  => Some(WindowResizeDirection::SouthWest),
        (false, true,  false, true)  => Some(WindowResizeDirection::SouthEast),
        (true,  false, false, false) => Some(WindowResizeDirection::West),
        (false, true,  false, false) => Some(WindowResizeDirection::East),
        (false, false, true,  false) => Some(WindowResizeDirection::North),
        (false, false, false, true)  => Some(WindowResizeDirection::South),
        _ => None,
    }
}

// In update():
let (mx, my) = ctx.input.mouse_pos_f32();
if ctx.input.button_just_pressed(MouseButton::Left) {
    if let Some(dir) = resize_direction(mx, my, ctx.width(), ctx.height()) {
        ctx.start_window_resize(dir);
    }
}
```

> **Note** — like `set_window_position`, this is silently ignored on Wayland
> and other platforms that manage window geometry themselves.
