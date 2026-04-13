#[derive(Clone)]
pub enum JsCommand {
    // ── Scene management ────────────────────────────────────────────────────
    CreateScene { scene_id: u32 },
    SetActiveScene { scene_id: u32 },
    ClearWorld,
    ExportScene { request_id: u32 },
    ImportScene { json: String },

    // ── Entity lifecycle ────────────────────────────────────────────────────
    SpawnEntity { name: String, kind: String, position: [f32; 3], color: [f32; 3] },
    RemoveEntity { name: String },
    SetVisible { name: String, visible: bool },

    // ── Primitives ──────────────────────────────────────────────────────────
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
    CreateCylinder {
        name: String,
        position: [f32; 3],
        radius_top: f32,
        radius_bottom: f32,
        height: f32,
        radial_segments: u32,
        open_ended: bool,
        color: [f32; 3],
    },
    CreateCone {
        name: String,
        position: [f32; 3],
        radius: f32,
        height: f32,
        radial_segments: u32,
        color: [f32; 3],
    },
    CreateTorus {
        name: String,
        position: [f32; 3],
        radius: f32,
        tube: f32,
        radial_segments: u32,
        tubular_segments: u32,
        color: [f32; 3],
    },
    CreatePlane {
        name: String,
        position: [f32; 3],
        width: f32,
        height: f32,
        width_segments: u32,
        height_segments: u32,
        color: [f32; 3],
    },
    CreateCapsule {
        name: String,
        position: [f32; 3],
        radius: f32,
        height: f32,
        radial_segments: u32,
        cap_segments: u32,
        color: [f32; 3],
    },
    CreateCircle {
        name: String,
        position: [f32; 3],
        radius: f32,
        segments: u32,
        color: [f32; 3],
    },
    CreateRing {
        name: String,
        position: [f32; 3],
        inner_radius: f32,
        outer_radius: f32,
        segments: u32,
        rings: u32,
        color: [f32; 3],
    },
    CreateText3D {
        name: String,
        text: String,
        font_data: Vec<u8>,
        position: [f32; 3],
        depth: f32,
        bevel_enabled: bool,
        bevel_thickness: f32,
        bevel_size: f32,
        quality: u32,
        color: [f32; 3],
    },

    // ── Transform ───────────────────────────────────────────────────────────
    SetPosition { name: String, position: [f32; 3] },
    SetRotation { name: String, rotation: [f32; 3] },
    SetScale    { name: String, scale: [f32; 3] },

    // ── Camera ──────────────────────────────────────────────────────────────
    SetCamera { eye: [f32; 3], target: [f32; 3] },
    SetCameraControlMode { mode: String },
    SetCameraParams { speed: f32, sensitivity: f32 },
    SetCameraFov { fov_degrees: f32 },

    // ── Lighting ────────────────────────────────────────────────────────────
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
    SetAmbientLight { color: [f32; 3], intensity: f32 },

    // ── Materials ───────────────────────────────────────────────────────────
    UpdateMaterial {
        entity_name: String,
        r: f32, g: f32, b: f32,
        metallic: f32,
        roughness: f32,
        clearcoat: f32,
        clearcoat_roughness: f32,
        opacity: f32,
        albedo_tex: Option<u32>,
    },

    // ── Environment ─────────────────────────────────────────────────────────
    SetEnvironment { fog_color: [f32; 3], fog_density: f32 },
    SetExposure { exposure: f32 },
    SetBackground { r: f32, g: f32, b: f32 },

    // ── Assets ──────────────────────────────────────────────────────────────
    LoadTexture { url: String, request_id: u32 },
    LoadModel   { url: String, request_id: u32 },

    // ── Debug / Plugins ─────────────────────────────────────────────────────
    SetDebugMode { enabled: bool },
    EnablePlugin  { name: String },
    DisablePlugin { name: String },
    SetSsaoParams {
        radius: f32,
        bias: f32,
        intensity: f32,
        power: f32,
    },
    LegacyCreateTerrain,
    LegacyToggleSky,
}