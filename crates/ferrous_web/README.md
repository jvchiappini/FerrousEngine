# ferrous_web

WebAssembly bindings for `FerrousWebEngine`.

## Architecture (modular)

`ferrous_web` no longer uses a terrain-demo-centric runtime as core behavior.

- `src/lib.rs`: minimal entrypoint + wasm export surface
- `src/engine.rs`: wasm API facade (`FerrousWebEngine`)
- `src/runtime.rs`: frame runtime + command processing
- `src/commands.rs`: JS→Rust command contract
- `src/config.rs`: runtime configuration + metrics

This keeps the engine API library-focused (closer to Three.js patterns) and avoids a God file.

## Core API (generic scene usage)

- Scene lifecycle: `create_scene`, `set_active_scene`
- Geometry: `create_box`, `create_sphere`, `spawn_entity`
- Transform: `set_transform`
- Camera: `set_camera`, `configure_camera`
- Lighting: `add_point_light`, `set_directional_light`
- Materials: `update_material`
- Cleanup: `remove_entity`, `clear_world`, `dispose`
- Runtime controls: `set_debug_mode`, `get_metrics_json`

## Legacy compatibility (plugins)

Legacy demo methods are retained but treated as plugin behavior:

- `create_terrain` → `terrain` plugin
- `toggle_sky` → `sky` plugin

Plugin toggles:

- `enable_plugin(name)`
- `disable_plugin(name)`

## PR1: API export sync guard

This crate keeps generated bindings in `pkg/` checked against the Rust wasm API.

### Build and regenerate bindings

From workspace root:

```bash
./scripts/build_ferrous_web.sh
```

Or dev profile:

```bash
./scripts/build_ferrous_web.sh dev
```

### Run sync check only

```bash
python3 ./scripts/check_ferrous_web_exports.py
```

The check validates method parity for `FerrousWebEngine` between:

- `crates/ferrous_web/src/lib.rs`
- `crates/ferrous_web/pkg/ferrous_web.js`
- `crates/ferrous_web/pkg/ferrous_web.d.ts`

If it fails, regenerate bindings with the build script above.