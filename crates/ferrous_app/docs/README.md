# ferrous_app — Application Shell

`ferrous_app` is the top-level crate you depend on to build any FerrousEngine
application. It wires together the window (winit), the GPU renderer, the GUI
system, the asset server, and the ECS scheduler into a single coherent frame
loop.

## What lives here

| Module / type | Role |
|---------------|------|
| `App<T>` | Fluent builder that accepts your `FerrousApp` impl and starts the event loop |
| `AppBuilder` | Plugin-based alternative builder (compose `Plugin` impls instead of a trait impl) |
| `FerrousApp` | Trait your application struct implements; six optional frame callbacks |
| `AppContext<'_>` | Per-frame handle passed to every callback — input, world, render, assets |
| `RenderContext<'_>` | User-facing renderer facade inside `AppContext::render` |
| `AppConfig` | Plain struct with all window / rendering configuration |
| `Plugin` trait | Extensibility mechanism; `DefaultPlugins` bundles all built-ins |

## Documentation map

```
docs/
├── README.md              ← this file
├── getting-started.md     ← step-by-step first app
├── app-builder.md         ← App<T> fluent API reference
├── plugins.md             ← AppBuilder + Plugin trait
├── ferrous-app-trait.md   ← FerrousApp callbacks
├── app-context.md         ← AppContext field & method reference
├── render-context.md      ← RenderContext method reference
└── config.md              ← ferrous.toml & AppConfig fields
```

## Re-exported types

`ferrous_app` re-exports the most commonly needed primitives so you rarely need
to add `ferrous_core`, `ferrous_gui`, or `ferrous_renderer` as direct
dependencies:

```rust
// All usable as `ferrous_app::*`
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
