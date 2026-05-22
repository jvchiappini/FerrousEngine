# `RenderContext` Reference

`RenderContext<'_>` is the renderer facade exposed through `ctx.render` inside
every [`FerrousApp`](ferrous-app-trait.md) callback. It hides all GPU internals
and exposes only what application code needs.

---

## Shading style

```rust
// Switch shading style — takes effect from the next frame
ctx.render.set_style(RenderStyle::Pbr);
ctx.render.set_style(RenderStyle::FlatShaded);
ctx.render.set_style(RenderStyle::CelShaded { toon_levels: 4, outline_width: 1.5 });

// Shortcut on AppContext (equivalent)
ctx.set_render_style(RenderStyle::FlatShaded);
```

---

## Passes

```rust
// Toggle SSAO (screen-space ambient occlusion)
ctx.render.set_ssao(true);

// Change clear/background colour
ctx.render.set_clear_color(Color::rgb(0.05, 0.05, 0.05));

// Append a custom render pass after built-ins
ctx.render.add_pass(MyVignettePass::new(0.4));
```

---

## Materials

```rust
use ferrous_renderer::{MaterialDescriptor, AlphaMode};

// Create a material
let mut desc = MaterialDescriptor::default();
desc.base_color = [1.0, 0.2, 0.2, 1.0]; // red
desc.metallic   = 0.0;
desc.roughness  = 0.8;
let handle = ctx.render.create_material(&desc);

// Assign to a world entity
ctx.world.set_material_handle(entity_id, handle);

// Update scalar params later (texture handles stay fixed)
desc.base_color = [0.2, 1.0, 0.2, 1.0]; // green
ctx.render.update_material(handle, &desc);
```

### `MaterialDescriptor` fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `base_color` | `[f32; 4]` | `[1,1,1,1]` | RGBA albedo |
| `metallic` | `f32` | `0.0` | 0 = dielectric, 1 = metal |
| `roughness` | `f32` | `0.5` | 0 = mirror, 1 = fully rough |
| `emissive` | `[f32; 3]` | `[0,0,0]` | Emissive colour (HDR values allowed) |
| `albedo_tex` | `Option<TextureHandle>` | `None` | Albedo / diffuse texture |
| `alpha_mode` | `AlphaMode` | `Opaque` | `Opaque`, `Mask(cutoff)`, `Blend` |

---

## Textures

```rust
// Upload raw RGBA8 pixels
let tex = ctx.render.renderer_mut()
    .create_texture_from_rgba(width, height, &rgba_bytes);

// Hot-reload (update pixels in place)
ctx.render.renderer_mut()
    .update_texture_data(tex, width, height, &new_bytes);

// Release
ctx.render.renderer_mut().free_texture(tex);
```

---

## Lighting

```rust
// Override the global directional light
// direction: normalised vec pointing FROM light TOWARD scene
ctx.render.set_directional_light(
    [0.0, -1.0, -0.3],     // direction
    [1.0, 0.95, 0.85],     // colour (linear RGB)
    2.5,                    // intensity multiplier
);
```

Prefer spawning a `DirectionalLight` ECS component for scene-managed lights;
use this method for imperative / UI-driven control.

---

## Camera helpers

```rust
ctx.render.camera_eye()             // Vec3 — current eye position
ctx.render.camera_target()          // Vec3 — current look-at target
ctx.render.camera_orbit_distance()  // f32  — orbit radius

// Override eye position (call once in setup; after that drive via ECS)
ctx.render.set_camera_eye(Vec3::new(0.0, 5.0, 10.0));
```

---

## Statistics

```rust
let stats = ctx.render.stats();
println!("vertices: {}  draw calls: {}", stats.vertices, stats.draw_calls);
```
