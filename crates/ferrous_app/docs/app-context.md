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

---

## World — `ctx.world`

The ECS scene graph. Mutate it in `update()`; the runner syncs it to the
renderer automatically.

```rust
// Spawn built-in primitives
let id = ctx.world.spawn_cube("MyCube", Vec3::new(0.0, 1.0, 0.0));
let id = ctx.world.spawn_sphere("MySphere", Vec3::ZERO);
let id = ctx.world.spawn_plane("Ground", Vec3::ZERO);

// Transform
ctx.world.set_position(id, Vec3::new(1.0, 0.0, 0.0));
ctx.world.set_rotation(id, Quat::from_rotation_y(0.5));
ctx.world.set_scale(id, Vec3::splat(2.0));
ctx.world.transform(id)          // Option<&Transform>

// Despawn
ctx.world.despawn(id);
```

---

## Render — `ctx.render`

A safe facade over the renderer. See the full [RenderContext reference](render-context.md).

```rust
ctx.render.set_style(RenderStyle::CelShaded { toon_levels: 4, outline_width: 1.5 });
ctx.render.set_ssao(false);
ctx.render.set_clear_color(Color::rgb(0.1, 0.1, 0.1));
let mat = ctx.render.create_material(&MaterialDescriptor::default());
ctx.render.set_directional_light([0.0, -1.0, 0.0], [1.0; 3], 2.0);
```

---

## Asset server — `ctx.asset_server`

```rust
// Start loading an asset (non-blocking)
let handle = ctx.asset_server.load::<GltfModel>("assets/player.glb");

// Poll in subsequent frames
match ctx.asset_server.get(handle) {
    AssetState::Loading   => { /* still in flight */ }
    AssetState::Ready(m)  => { /* use the model */ }
    AssetState::Failed(e) => { eprintln!("load failed: {e}"); }
}
```

---

## Viewport — `ctx.viewport`

```rust
pub struct Viewport { pub x: u32, pub y: u32, pub width: u32, pub height: u32 }
```

Set this in `update()` to control where the 3D scene is rendered.  Useful for
split-panel UIs where the 3D view occupies only part of the window.

```rust
fn update(&mut self, ctx: &mut AppContext) {
    // Render 3D scene in the right 80 % of the window
    ctx.viewport = Viewport {
        x:      (ctx.width() as f32 * 0.2) as u32,
        y:      0,
        width:  (ctx.width() as f32 * 0.8) as u32,
        height: ctx.height(),
    };
}
```

---

## Camera info (read-only)

```rust
ctx.camera_eye     // Vec3 — world-space eye position this frame
ctx.camera_target  // Vec3 — world-space look-at target this frame
```

Populated at the start of `draw_3d`. Zero before the first frame.

---

## Exiting

```rust
ctx.request_exit();  // gracefully stops the event loop after the current frame
```

---

## Statistics

```rust
ctx.render_stats.vertices    // u64
ctx.render_stats.triangles   // u64
ctx.render_stats.draw_calls  // u32
```
