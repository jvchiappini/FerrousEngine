# `AppBuilder` & Plugins

`AppBuilder` is an alternative entry point to `App<T>`. Instead of implementing
a single `FerrousApp` struct, you compose **plugins** — each plugin registers
systems, modifies config, or adds render passes.

```rust
use ferrous_app::{AppBuilder, DefaultPlugins, Plugin};

fn main() {
    AppBuilder::new()
        .add_plugin(DefaultPlugins)
        .add_plugin(MyGamePlugin)
        .run();
}
```

---

## `AppBuilder` API

```rust
// Window / renderer config (same options as App<T>)
AppBuilder::new()
    .with_title("My App")
    .with_size(1280, 720)
    .with_render_quality(RenderQuality::High)
    .with_config_file("ferrous.toml")

// Plugin registration
    .add_plugin(DefaultPlugins)
    .add_plugin(MyPlugin)

// Register a plain function as a system (Stage::Update)
    .add_system_fn(my_update_fn)

// Register a system at a specific stage
    .add_system(Stage::PostUpdate, MySystem)

    .run();
```

---

## `Plugin` trait

```rust
pub trait Plugin: 'static {
    fn name(&self) -> &'static str;
    fn build(&self, app: &mut AppBuilder);
    fn cleanup(&self, _app: &mut AppBuilder) {} // optional
}
```

`build` is called immediately when `add_plugin` is called, in registration
order.

### Example plugin

```rust
struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn name(&self) -> &'static str { "PhysicsPlugin" }

    fn build(&self, app: &mut AppBuilder) {
        app.add_system(Stage::Update, PhysicsStep);
        app.add_system(Stage::PostUpdate, CollisionSolver);
    }
}
```

### Duplicate detection

Registering the same plugin name twice panics:

```
Plugin 'DefaultPlugins' registered twice — each plugin may only be added once
```

---

## Built-in plugins

### `DefaultPlugins`

Bundles all engine subsystems in one call:

```rust
AppBuilder::new().add_plugin(DefaultPlugins).run();
```

Equivalent to registering: `CorePlugin`, `WindowPlugin`, `InputPlugin`,
`AssetPlugin`, `RendererPlugin`, `GuiPlugin`.

### Individual plugins

| Plugin | What it registers |
|--------|-------------------|
| `CorePlugin` | `TimeSystem`, `VelocitySystem`, `AnimationSystem`, `BehaviorSystem`, `TransformSystem` |
| `WindowPlugin` | Window title/size config |
| `InputPlugin` | Keyboard/mouse input subsystem |
| `AssetPlugin` | `AssetServer` |
| `RendererPlugin` | GPU renderer, render style, MSAA |
| `GuiPlugin` | 2D GUI layer (`Ui`, `UiPass`) |
| `TimePlugin` | Frame timing only (subset of CorePlugin) |

### Customising `RendererPlugin`

```rust
use ferrous_app::{AppBuilder, RendererPlugin, DefaultPlugins};
use ferrous_app::RenderStyle;

AppBuilder::new()
    .add_plugin(DefaultPlugins)
    // Override the renderer added by DefaultPlugins is not needed;
    // configure via RendererPlugin before DefaultPlugins instead:
    .run();

// Or build without DefaultPlugins and add individually:
AppBuilder::new()
    .add_plugin(RendererPlugin {
        render_style: RenderStyle::CelShaded { toon_levels: 3, outline_width: 1.0 },
        hdri_path:    Some("assets/skybox/scene.exr".to_string()),
        sample_count: 4,
    })
    .run();
```

---

## ECS stages

Systems run in this fixed order every frame:

| Stage | Default systems | When to use |
|-------|----------------|-------------|
| `Stage::PreUpdate` | `TimeSystem` | Read-only prep work |
| `Stage::Update` | `VelocitySystem`, `AnimationSystem`, `BehaviorSystem` | Main game logic |
| `Stage::PostUpdate` | `TransformSystem` | Propagate computed values (transforms, etc.) |

```rust
app.add_system(Stage::Update, MyMoveSystem);
app.add_system_fn(|world: &mut World, res: &mut ResourceMap| {
    // closure-style system
});
```
