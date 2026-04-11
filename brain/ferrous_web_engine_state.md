# 🛠️ Ferrous Web: Engine State Summary

## 📊 Current Core Capabilities

| Feature | Status | Description |
| :--- | :--- | :--- |
| **Renderer** | ✅ Stable | WebGPU-based PBR renderer with bindless-lite texture support. |
| **ECS** | ✅ Stable | High-performance Archetype-based ECS integration. |
| **JS API** | ✅ Fluent | Chainable command pattern: `engine.createBox().setPosition(...)`. |
| **Materials** | ✅ Pro | Support for metallic, roughness, clearcoat, opacity, and textures. |
| **Assets** | ✅ Async | Promise-based loading for textures and GLTF models. |
| **Environment**| ✅ Dynamic | Real-time fog, exposure, and sky controls. |
| **Persistence**| ✅ Active | Full scene save/load via JSON serialization (`SceneBlueprint`). |

---

## 💾 Scene Persistence Pro
The engine now supports deep serialization of world state. This enables professional workflows similar to Unreal Engine or Unity, where entire scenes can be exported to JSON/Binary and reconstructed perfectly.

### Key Logic Flow
1. **Serialization**: `serde` captures component data from the ECS and legacy registry.
2. **Aggregation**: `World::to_blueprint` bundles all entities and global lights into a `SceneBlueprint`.
3. **Bridge**: `FerrousWebEngine.export_scene()` returns a JS Promise that resolves with the JSON string.
4. **Reconstruction**: `FerrousWebEngine.import_scene(json)` clears the world and re-spawns entities from the blueprint.

---

## 📂 Public JS API (WASM Bridge)

### Scene Persistence
- `engine.export_scene() -> Promise<string>`: Serializes the entire active scene to JSON.
- `engine.import_scene(json: string)`: Clears the current world and loads a new scene from JSON.

### Fluent Entity Creation
- `engine.createBox(name, x, y, z, sx, sy, sz, r, g, b) -> JsEntity`
- `engine.createSphere(name, x, y, z, radius, segments, r, g, b) -> JsEntity`
- `engine.spawnEntity(name, kind, x, y, z, r, g, b) -> JsEntity`

### JS Entity Methods (Chainable)
- `entity.set_position(x, y, z)`
- `entity.set_rotation(rx, ry, rz)`
- `entity.set_scale(sx, sy, sz)`
- `entity.set_material(r, g, b, metal, rough)`
- `entity.set_texture(texture_id)`
- `entity.remove()`

---

## 🚀 Recent Accomplishments
1. **Scene Persistence**: Enabled full-world save/load capabilities, supporting complex entity hierarchies and light setups.
2. **Type-Safe Serialization**: Implemented `Serialize`/`Deserialize` for all core engine structures without introducing technical debt.
3. **Internal Sync**: Optimized `WebRuntime` to stay in sync with the ECS world after massive scene modifications/imports.
