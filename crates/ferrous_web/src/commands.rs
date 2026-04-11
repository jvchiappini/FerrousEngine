#[derive(Clone)]
pub enum JsCommand {
    CreateScene {
        scene_id: u32,
    },
    SetActiveScene {
        scene_id: u32,
    },
    CreateBox {
        name: String,
        position: [f32; 3],
        size: [f32; 3],
        color: [f32; 3],
    },
    CreateSphere {
        name: String,
        position: [f32; 3],
        radius: f32,
        segments: u32,
        color: [f32; 3],
    },
    SpawnEntity {
        name: String,
        kind: String,
        position: [f32; 3],
        color: [f32; 3],
    },
    SetTransform {
        name: String,
        position: [f32; 3],
        rotation: [f32; 3],
        scale: [f32; 3],
    },
    SetCamera {
        eye: [f32; 3],
        target: [f32; 3],
    },
    SetCameraControlMode {
        mode: String,
    },
    AddPointLight {
        name: String,
        position: [f32; 3],
        color: [f32; 3],
        intensity: f32,
        range: f32,
    },
    SetDirectionalLight {
        direction: [f32; 3],
        color: [f32; 3],
        intensity: f32,
    },
    UpdateMaterial {
        entity_name: String,
        r: f32,
        g: f32,
        b: f32,
        metallic: f32,
        roughness: f32,
    },
    RemoveEntity {
        name: String,
    },
    ClearWorld,
    SetDebugMode {
        enabled: bool,
    },
    EnablePlugin {
        name: String,
    },
    DisablePlugin {
        name: String,
    },
    LegacyCreateTerrain,
    LegacyToggleSky,
}