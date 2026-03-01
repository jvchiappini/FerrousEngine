<!--
Reference for the camera subsystem: GpuCamera, OrbitState, and the
Controller configuration API re-exported from ferrous_core.
-->

# Camera

The camera subsystem is split across three layers.

| Layer | Type | Location |
|-------|------|----------|
| Application-facing state | `Camera` | `ferrous_core` (re-exported) |
| User configuration | `Controller` | `ferrous_core` (re-exported) |
| GPU mirror | `GpuCamera` | `camera/uniform.rs` |
| Input driver | `OrbitState` | `camera/controller.rs` |

## `Camera`

`Camera` is the logical camera owned by the application.  It stores
position, target, field of view, near/far planes, and a `Controller`
for configurable input.  It exposes `view_matrix()` and
`projection_matrix(aspect)` which the renderer samples each frame.

`Renderer` keeps one `Camera` instance internally.  Access it via:

```rust
renderer.camera()          // &Camera
renderer.camera_mut()      // &mut Camera
```

Modifying the camera directly is the correct way to teleport it, change
FOV, or swap out the controller.

## `Controller`

`Controller` lives inside `camera.controller` and drives motion
parameters.  All fields are public and can be changed at any time.

```rust
pub struct Controller {
    pub speed:            f32,   // world-units per second  (default 5.0)
    pub mouse_sensitivity: f32,  // radians per pixel       (default 0.005)
    pub orbit_distance:   f32,   // eye-to-target distance  (default 5.0)
    // --- private ---
    mappings: HashMap<KeyCode, Vec3>,
}
```

### Key bindings

Bindings map a `KeyCode` to a movement direction `Vec3` in camera
space.  The default WASD preset is installed by
`Controller::with_default_wasd()`.

| Method | Description |
|--------|-------------|
| `bind(key, dir)` | Map a key to a direction, overwriting any previous mapping |
| `unbind(key)` | Remove the mapping for a key |
| `clear_bindings()` | Remove all key mappings |
| `set_mapping(key, dir)` | Alias for `bind` |
| `direction(input)` | Return the summed movement vector for all currently held keys |

The pre-built WASD layout maps:
- `W` → `Vec3::NEG_Z` (forward)
- `S` → `Vec3::Z` (backward)
- `A` → `Vec3::NEG_X` (strafe left)
- `D` → `Vec3::X` (strafe right)

### Runtime reconfiguration

```rust
use ferrous_renderer::{Controller, KeyCode};
use glam::Vec3;

let cam = renderer.camera_mut();

// Change speed and sensitivity
cam.controller.speed            = 10.0;
cam.controller.mouse_sensitivity = 0.002;
cam.controller.orbit_distance   = 8.0;

// Replace movement keys with arrow keys
cam.controller.clear_bindings();
cam.controller.bind(KeyCode::ArrowUp,    Vec3::NEG_Z);
cam.controller.bind(KeyCode::ArrowDown,  Vec3::Z);
cam.controller.bind(KeyCode::ArrowLeft,  Vec3::NEG_X);
cam.controller.bind(KeyCode::ArrowRight, Vec3::X);
```

Any change takes effect on the next call to `Renderer::handle_input`.

### Disabling movement entirely

```rust
cam.controller.clear_bindings();
cam.controller.mouse_sensitivity = 0.0;
```

## `GpuCamera`

`GpuCamera` maintains the GPU-side representation of the camera.  It is
managed internally by `Renderer` and is not normally accessed directly.

```rust
pub struct GpuCamera {
    pub uniform:    CameraUniform,
    pub buffer:     Arc<wgpu::Buffer>,
    pub bind_group: Arc<wgpu::BindGroup>,
}
```

- **`uniform`** — a `CameraUniform` struct that is `Pod + Zeroable`
  (via `bytemuck`) and contains the 4×4 view-projection matrix.
- **`buffer`** — a `UNIFORM | COPY_DST` buffer created from
  `resources::buffer::create_uniform`.
- **`bind_group`** — bound to group 0 in the world pipeline shader.

**`sync(queue, camera)`** — writes the current view-projection matrix
to the buffer.  Called by `WorldPass::prepare` each frame.

## `OrbitState`

`OrbitState` is an orbital camera driver: the user rotates around a
fixed target point.  It accumulates yaw and pitch from mouse drag
deltas and applies movement key input along the orbit sphere.

```rust
pub struct OrbitState {
    pub yaw:   f32,   // horizontal angle in radians
    pub pitch: f32,   // vertical angle in radians, clamped to ±85°
}
```

`Renderer` owns one `OrbitState` instance and calls
`orbit.update(&mut camera, &mut input, delta_time)` from `handle_input`
each frame.

### What `update` does

1. Reads `camera.controller.direction(input)` to get the summed
   movement vector.  Scales by `controller.speed * dt`.
2. Applies right-button mouse drag: reads the delta from
   `input.mouse_delta()` and multiplies by `controller.mouse_sensitivity`.
3. Clamps pitch to `[-1.484, +1.484]` (≈ ±85°) to prevent gimbal flip.
4. Recomputes the eye position from yaw, pitch, and
   `controller.orbit_distance`.
5. Writes `camera.position` and calls `camera.look_at(target)`.

All configuration values are read from `camera.controller` — there are
no hardcoded constants in the renderer.

## Shader interface

The camera uniform is consumed in `assets/shaders/base.wgsl`:

```wgsl
struct CameraUniform {
    view_proj: mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;
```

Any custom pipeline that needs the camera must declare the same binding.
