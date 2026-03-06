# Configuration — `ferrous.toml`

FerrousEngine can be configured with a `ferrous.toml` file in the working
directory. All fields are optional; missing keys fall back to defaults.

Load it in code with:

```rust
App::new(MyApp::default())
    .with_config_file("ferrous.toml")  // load first…
    .with_title("Override")            // …then override in code
    .run();
```

---

## Supported keys

```toml
[window]
title      = "My App"   # window title
width      = 1280       # initial width  (logical pixels)
height     = 720        # initial height (logical pixels)
resizable  = true
vsync      = true

[performance]
target_fps    = 60      # integer FPS cap; omit for unlimited
idle_timeout  = 5.0     # seconds of no input before stopping redraws

[renderer]
msaa          = 1       # 1 = off, 4 = 4x MSAA
render_style  = "pbr"   # "pbr" | "cel" | "flat"
hdri          = "assets/skybox/scene.exr"

[quality]
preset = "high"         # "low" | "medium" | "high" | "ultra"
```

---

## `AppConfig` struct

The same settings are available programmatically as fields on `AppConfig`:

```rust
use ferrous_app::{App, AppConfig, Color};
use ferrous_renderer::RenderStyle;

let config = AppConfig {
    title:            "My App".to_string(),
    width:            1280,
    height:           720,
    vsync:            true,
    resizable:        true,
    background_color: Color::rgb(0.1, 0.1, 0.1),
    target_fps:       Some(60),
    idle_timeout:     None,
    sample_count:     1,
    hdri_path:        None,
    render_style:     RenderStyle::Pbr,
    render_quality:   ferrous_core::RenderQuality::High,
    font_path:        None,
    font_bytes:       None,
};

// Use with AppBuilder
AppBuilder::new().with_config(config).run();
```
