#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CameraControlMode {
    None,
    Fly,
    Orbit,
}

impl CameraControlMode {
    pub fn parse(mode: &str) -> Option<Self> {
        match mode.to_ascii_lowercase().as_str() {
            "none" | "static" => Some(Self::None),
            "fly" | "fps" | "wasd" => Some(Self::Fly),
            "orbit" => Some(Self::Orbit),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Fly => "fly",
            Self::Orbit => "orbit",
        }
    }
}

#[derive(Clone, Copy)]
pub struct EngineConfig {
    pub move_speed: f32,
    pub eye: [f32; 3],
    pub target: [f32; 3],
    pub control_mode: CameraControlMode,
    pub look_sensitivity: f32,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            move_speed: 12.0,
            eye: [12.0, 8.0, 12.0],
            target: [0.0, 0.0, 0.0],
            control_mode: CameraControlMode::Fly,
            look_sensitivity: 0.005,
        }
    }
}

#[derive(Default)]
pub struct EngineMetrics {
    pub commands_processed: u64,
}