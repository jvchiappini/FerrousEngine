use serde::{Serialize, Deserialize};
use crate::scene::{Element, DirectionalLight};

/// A serializable "blueprint" of a scene.
///
/// This structure captures all entities and global scene state (like lighting)
/// that should be persisted to disk or sent over the network.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneBlueprint {
    pub name: String,
    pub entities: Vec<Element>,
    pub directional_light: Option<DirectionalLight>,
}

impl SceneBlueprint {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            entities: Vec::new(),
            directional_light: None,
        }
    }
}
