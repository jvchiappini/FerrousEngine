# App Builder — `App<T>`

`App<T>` is the primary entry point for applications that implement
[`FerrousApp`](ferrous-app-trait.md) directly. It follows the **fluent builder
pattern**: chain `with_*` methods to configure the window and renderer, then
call `.run()` to start the event loop (which blocks until the window is closed).

```rust
use ferrous_app::{App, Color};

fn main() {
    App::new(MyApp::default())
        .with_title("My App")
        .with_size(1280, 720)
        .with_background_color(Color::rgb(0.1, 0.1, 0.12))
        .with_font("assets/fonts/Roboto-Regular.ttf")
        .run();
}
```

---

## Constructor

```rust
pub fn new(app_state: A) -> Self
```

Accepts any value that implements `FerrousApp`. Returns an `App<A>` with
default configuration (1280×720, vsync on, 60 fps cap, dark background).

---

## Window configuration

| Method | Type | Default | Description |
|--------|------|---------|-------------|
| `.with_title(title)` | `&str` | `"Ferrous Application"` | Window title bar text |
| `.with_size(w, h)` | `u32, u32` | `1280, 720` | Initial window size in logical pixels |
| `.with_resizable(r)` | `bool` | `true` | Allow the user to resize the window |
| `.with_decorations(d)` | `bool` | `true` | Show/hide the OS title bar and min/max/close buttons; set `false` for a fully-custom borderless window |
| `.with_vsync(v)` | `bool` | `true` | Lock present rate to monitor refresh; set `false` for uncapped |

### Custom title bar (borderless window)

Set `.with_decorations(false)` to remove the OS-native title bar and control
buttons entirely.  The window becomes a plain borderless surface — use
`ferrous_gui` to draw your own title bar, drag region, and close/minimize/
maximize buttons.

```rust
App::new(MyApp::default())
    .with_title("My App")          // still used as the taskbar / alt-tab label
    .with_decorations(false)       // no OS title bar
    .with_resizable(false)         // optional: prevent resize handles too
    .run();
```

> **Tip** — pair this with `AppMode::Desktop2D` for a lightweight GUI tool
> that has zero 3-D overhead.

---

## Rendering configuration

| Method | Type | Default | Description |
|--------|------|---------|-------------|
| `.with_background_color(c)` | `Color` | dark grey | Clear colour applied before every frame |
| `.with_msaa(n)` | `u32` | `1` | MSAA sample count — `1` = off, `4` = 4× MSAA |
| `.with_render_style(s)` | `RenderStyle` | `RenderStyle::Pbr` | Initial shading style (see below) |
| `.with_render_quality(q)` | `RenderQuality` | `RenderQuality::High` | Quality preset |
| `.with_hdri(path)` | `&str` | none | Path to an `.exr` environment map for image-based lighting |

### `RenderStyle` variants

```rust
RenderStyle::Pbr                                    // full PBR (default)
RenderStyle::CelShaded { toon_levels: 4,
                          outline_width: 0.02 }     // toon + outline
RenderStyle::FlatShaded                             // faceted / low-poly
```

---

## Performance

| Method | Type | Default | Description |
|--------|------|---------|-------------|
| `.with_target_fps(fps)` | `Option<u32>` | `Some(60)` | FPS cap; `None` = unlimited |
| `.with_idle_timeout(t)` | `Option<f32>` | `None` | Seconds of no input before the app stops continuous redraws (saves CPU for UI-heavy apps) |

---

## Font loading

Fonts are required for text rendering in `draw_ui`. Supply at most one.

```rust
// Load from disk at startup (desktop only)
.with_font("assets/fonts/Roboto-Regular.ttf")

// Embed bytes directly (works on all platforms including WASM)
.with_font_bytes(include_bytes!("../../assets/fonts/Roboto-Regular.ttf"))
```

`with_font_bytes` takes priority over `with_font` if both are set.

---

## Config file

```rust
// Load ferrous.toml first, then override individual fields in code
App::new(MyApp::default())
    .with_config_file("ferrous.toml")  // missing file is silently ignored
    .with_title("Override Title")       // code overrides take priority
    .run();
```

See [Configuration reference](config.md) for all supported `ferrous.toml` keys.

---

## Running

```rust
pub fn run(self)
```

Starts the winit event loop. **Blocks the calling thread** until the window is
closed. There is no async entry point; the runner internally drives polling and
rendering through winit callbacks.
