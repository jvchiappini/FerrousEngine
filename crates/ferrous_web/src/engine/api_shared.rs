use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum Vector3Config {
    Array([f32; 3]),
    Object { x: f32, y: f32, z: f32 },
}

impl Vector3Config {
    pub fn to_array(&self) -> [f32; 3] {
        match self {
            Self::Array(arr) => *arr,
            Self::Object { x, y, z } => [*x, *y, *z],
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum ColorConfig {
    Hex(String),
    Array([f32; 3]),
    Object { r: f32, g: f32, b: f32 },
}

impl ColorConfig {
    pub fn to_array(&self) -> [f32; 3] {
        match self {
            Self::Hex(hex) => {
                let hex = hex.trim_start_matches('#');
                if hex.len() == 6 {
                    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(255) as f32 / 255.0;
                    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(255) as f32 / 255.0;
                    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(255) as f32 / 255.0;
                    [r, g, b]
                } else {
                    [1.0, 1.0, 1.0]
                }
            }
            Self::Array(arr) => *arr,
            Self::Object { r, g, b } => [*r, *g, *b],
        }
    }
}
