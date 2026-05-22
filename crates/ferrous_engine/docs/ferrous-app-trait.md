# `FerrousApp` Trait

`FerrousApp` is the trait your application struct implements. Every method has
an empty default implementation — override only the ones you need.

```rust
pub trait FerrousApp {
    fn setup(&mut self, ctx: &mut AppContext) {}
    fn configure_ui(&mut self, ui: &mut Ui) {}
    fn update(&mut self, ctx: &mut AppContext) {}
    fn draw_ui(&mut self, dc: &mut DrawContext<'_, '_>) {}
    fn draw_3d(&mut self, ctx: &mut AppContext) {}   // only called in Game3D mode
    fn on_resize(&mut self, new_size: (u32, u32), ctx: &mut AppContext) {}
    fn on_window_event(&mut self, event: &winit::event::WindowEvent,
                       ctx: &mut AppContext) {}
}
```

---

## `DrawContext`

`DrawContext<'a, 'b>` is passed to `draw_ui` each frame. It bundles the four
things 2D drawing needs so you never have to thread them as separate parameters.

```rust
pub struct DrawContext<'a, 'b> {
    pub gui:  &'a mut GuiBatch,      // push 2D quads
    pub text: &'a mut TextBatch,     // push text strings
    pub font: &'a Font,              // always a valid font reference
    pub ctx:  &'a mut AppContext<'b>,// input, time, world, etc.
}
```

Destructure with field access or bind locals for ergonomics:

```rust
fn draw_ui(&mut self, dc: &mut DrawContext<'_, '_>) {
    let DrawContext { gui, text, font, ctx } = dc;
    text.draw_text(font, "Score: 0", [10.0, 10.0], 20.0, [1.0; 4]);
    gui.rect(0.0, 0.0, 200.0, 40.0, [0.0, 0.0, 0.0, 0.6]);
}
```

---

## App modes

Use `App::new(state).with_mode(AppMode::…)` to tell the runner which
subsystems to activate.  Only override what you actually need.

| Mode | ECS world sync | `draw_3d` called | Typical use |
|------|:--------------:|:----------------:|-------------|
| `AppMode::Game3D` *(default)* | ✓ | ✓ | 3-D games, simulations |
| `AppMode::Game2D` | ✗ | ✗ | 2-D games, side-scrollers |
| `AppMode::Desktop2D` | ✗ | ✗ | GUI tools, editors, utilities |

All three modes have a live renderer so `draw_ui`, fonts, and GUI widgets
work everywhere.

```rust
// Desktop tool — no 3D overhead at all
App::new(MyTool)
    .with_mode(AppMode::Desktop2D)
    .run();

// 2-D game
App::new(MyPlatformer)
    .with_mode(AppMode::Game2D)
    .run();

// 3-D game (default, no need to specify)
App::new(MyGame)
    .run();
```

---

## Call order per frame

```
resumed (once)
    └─▶ setup()
    └─▶ configure_ui()

every frame
    ├─▶ update()
    ├─▶ draw_3d()   ← Game3D only; skipped in Game2D / Desktop2D
    └─▶ draw_ui()   ← always; 2D GUI layer composited on top
```

`on_resize` and `on_window_event` are called as events arrive, outside the
normal frame loop.

---

## `setup`

```rust
fn setup(&mut self, ctx: &mut AppContext) {}
```

Called **once**, after the window and GPU are ready. Use it to:
- Spawn initial ECS entities (`ctx.world.spawn_cube(…)`)
- Load assets (`ctx.asset_server.load(…)`)
- Set the initial camera position

---

## `configure_ui`

```rust
fn configure_ui(&mut self, ui: &mut Ui) {}
```

Called **once**, during startup. Add interactive widgets here so the engine
routes input events to them automatically every frame.

```rust
fn configure_ui(&mut self, ui: &mut Ui) {
    ui.add(self.save_button.clone());
    ui.add(self.name_input.clone());
    ui.add(self.color_picker.clone());
}
```

> **Important:** widgets must be cloned into `Ui` because `Ui` takes
> ownership. Keep your own copy in the struct to read state (e.g.
> `self.save_button.pressed`).

---

## `update`

```rust
fn update(&mut self, ctx: &mut AppContext) {}
```

Called every frame, **before** rendering. This is the main place for:
- Reading input (`ctx.input.just_pressed(…)`)
- Mutating scene state (`ctx.world.set_position(…)`)
- Reading widget state that was updated by input routing
- Requesting exit (`ctx.request_exit()`)

---

## `draw_ui`

```rust
fn draw_ui(&mut self, dc: &mut DrawContext<'_, '_>) {}
```

Called every frame to emit 2D draw commands. Push quads and text into the
provided batches via `DrawContext`. These are composited **on top** of the 3D scene.

`DrawContext` bundles everything you need:

```rust
pub struct DrawContext<'a, 'b> {
    pub gui:  &'a mut GuiBatch,
    pub text: &'a mut TextBatch,
    pub font: &'a Font,          // always valid — no Option unwrap needed
    pub ctx:  &'a mut AppContext<'b>,
}
```

```rust
fn draw_ui(&mut self, dc: &mut DrawContext<'_, '_>) {
    self.toolbar_bg.draw(dc.gui);
    self.color_btn.draw(dc.gui);

    // font is always &Font — no Option check needed
    dc.text.draw_text(dc.font, "FPS: 60", [10.0, 10.0], 16.0, [1.0; 4]);

    // shape helpers
    dc.gui.rect(0.0, 0.0, 200.0, 30.0, [0.1, 0.1, 0.1, 0.8]);
}
```

---

## `draw_3d`

```rust
fn draw_3d(&mut self, ctx: &mut AppContext) {}
```

Called every frame before `draw_ui`. Use it to push gizmos or read camera
state. The 3D scene is rendered automatically from the ECS world — you only
need this callback for manual overlays or imperative rendering.

---

## `on_resize`

```rust
fn on_resize(&mut self, new_size: (u32, u32), ctx: &mut AppContext) {}
```

Called when the window is resized (physical pixels). The swap-chain and camera
aspect ratio are already updated when this fires.

```rust
fn on_resize(&mut self, new_size: (u32, u32), _ctx: &mut AppContext) {
    let (w, h) = new_size;
    // reposition UI elements that depend on window size
    self.toolbar.rect[2] = w as f32;
}
```

---

## `on_window_event`

```rust
fn on_window_event(&mut self, event: &winit::event::WindowEvent,
                   ctx: &mut AppContext) {}
```

Raw access to every winit window event after it has been processed by the GUI
and input systems. Use this for drag-and-drop, IME input, file drops, or
anything not covered by `AppContext::input`.
