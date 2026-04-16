use crate::render::types::SpriteInstance;
use std::collections::HashMap;

/// Groups sprites by their Texture ID, which allows one Draw Call per texture.
#[derive(Default)]
pub struct SpriteBatcher {
    // texture_id -> list of sprite instances
    pub batches: HashMap<u32, Vec<SpriteInstance>>,
}

impl SpriteBatcher {
    pub fn clear(&mut self) {
        // Keep allocations but clear data for the next frame
        for instances in self.batches.values_mut() {
            instances.clear();
        }
    }

    pub fn push_sprite(&mut self, texture_id: u32, instance: SpriteInstance) {
        if let Some(instances) = self.batches.get_mut(&texture_id) {
            instances.push(instance);
        } else {
            self.batches.insert(texture_id, vec![instance]);
        }
    }
}

/// Simple batcher for untextured technical shapes.
#[derive(Default)]
pub struct ShapeBatcher {
    pub instances: Vec<crate::render::types::ShapeInstance>,
}

impl ShapeBatcher {
    pub fn clear(&mut self) {
        self.instances.clear();
    }

    pub fn push_shape(&mut self, instance: crate::render::types::ShapeInstance) {
        self.instances.push(instance);
    }
}
