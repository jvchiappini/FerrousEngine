# ferrous_app — Application Shell

`ferrous_app` is the top-level crate you depend on to build any FerrousEngine
application. It wires together the window (winit), the GPU renderer, the GUI
system, the asset server, and the ECS scheduler into a single coherent frame
loop.

## Retained UI is draw-only (prompt 15 phase 4)

The retained UI (errous_gui::UiSystem + errous_ui_core widgets) is
**layout and paint only**. The runner no longer dispatches pointer and key
events to it; errous_events::EventManager was removed. Input, gestures,
focus, shortcuts, scrolling and text editing are guimaker::input. Apps that
need interaction build on GUIMaker.

Affected: crates/ferrous_editor (configure_ui builds Button/Slider/
ViewportWidget; they draw but their callbacks do not fire), any external
desktop or web app that implements FerrousApp::configure_ui, and web apps
linked against errous_web/errous_gui that use the trait. Tests such as
	ests/zero_alloc.rs use UiSystem for layout/paint only. Retained-UI input
is not being restored: the errous_editor port to GUIMaker is in the
Phase 11 backlog.

## What lives here

| Module / type | Role |
|---------------|------|
| `App<T>` | Fluent builder that accepts your `FerrousApp` impl and starts the event loop |
| `AppBuilder` | Plugin-based alternative builder (compose `Plugin` impls instead of a trait impl) |
| `FerrousApp` | Trait your application struct implements; six optional frame callbacks |
| `AppContext<'_>` | Per-frame handle passed to every callback — input, world, render, assets |
| `RenderContext<'_>` | User-facing renderer facade inside `AppContext::render` |
| `FileDropEvent` | Queued OS file-drag events delivered to `FerrousApp::on_file_drop` |
| `AppConfig` | Plain struct with all window / rendering configuration |
| `Plugin` trait | Extensibility mechanism; `DefaultPlugins` bundles all built-ins |

## Documentation map

```
docs/
â”œâ”€â”€ README.md              â† this file
â”œâ”€â”€ getting-started.md     â† step-by-step first app
â”œâ”€â”€ app-builder.md         â† App<T> fluent API reference
â”œâ”€â”€ plugins.md             â† AppBuilder + Plugin trait
â”œâ”€â”€ ferrous-app-trait.md   â† FerrousApp callbacks
â”œâ”€â”€ app-context.md         â† AppContext field & method reference
â”œâ”€â”€ render-context.md      â† RenderContext method reference
â””â”€â”€ config.md              â† ferrous.toml & AppConfig fields
- performance.md         GPU instruments, pass timing, MSAA and target sizes
- present.md             present modes (Fifo/FifoRelaxed/Mailbox) and pacing
- memory.md              memory hints and the GPU/CPU inventory
```

## Re-exported types

`ferrous_app` re-exports the most commonly needed primitives so you rarely need
to add `ferrous_core`, `ferrous_gui`, or `ferrous_renderer` as direct
dependencies:

```rust
// All usable as `ferrous_engine::*`
pub use ferrous_core::{Color, Handle, InputState, KeyCode, MouseButton,
                       Time, Transform, World, Vec2, Vec3, Vec4, Quat, Mat4,
                       RenderStats, Viewport};
pub use ferrous_core::scene::{Camera3D, DirectionalLight, OrbitCamera,
                              Material, MaterialBuilder};
pub use ferrous_renderer::RenderStyle;
pub use ferrous_core::RenderQuality;
pub use ferrous_renderer::scene::GizmoDraw;
pub use ferrous_ecs::prelude::{Entity, StagedScheduler};
```
