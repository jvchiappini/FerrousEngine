//! Image registry for `guimaker::display_list::DrawCommand::Image`.
//!
//! The display list names an image by an adapter-defined `u64` source id. The
//! host registers the GPU texture for that id once; [`GuiMakerRenderer`] then
//! resolves every `Image` command through this map.
//!
//! [`GuiMakerRenderer`]: crate::GuiMakerRenderer

use std::collections::HashMap;
use std::sync::Arc;

use ferrous_ui_render::Texture2d;

/// Maps `GUIMaker` image source ids to GPU textures.
///
/// The map is intentionally small: it owns only the `Arc` handles. Upload,
/// reload and eviction belong to the host's asset pipeline; this type never
/// looks inside a texture.
#[derive(Clone, Debug, Default)]
pub struct TextureMap {
    textures: HashMap<u64, Arc<Texture2d>>,
}

impl TextureMap {
    /// Builds an empty map.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers `texture` for `source`, returning the previous handle.
    pub fn insert(&mut self, source: u64, texture: Arc<Texture2d>) -> Option<Arc<Texture2d>> {
        self.textures.insert(source, texture)
    }

    /// Removes a source, returning its texture.
    pub fn remove(&mut self, source: u64) -> Option<Arc<Texture2d>> {
        self.textures.remove(&source)
    }

    /// The texture registered for `source`.
    #[must_use]
    pub fn get(&self, source: u64) -> Option<&Arc<Texture2d>> {
        self.textures.get(&source)
    }

    /// Whether `source` has a texture.
    #[must_use]
    pub fn contains(&self, source: u64) -> bool {
        self.textures.contains_key(&source)
    }

    /// Number of registered sources.
    #[must_use]
    pub fn len(&self) -> usize {
        self.textures.len()
    }

    /// Whether the map holds no textures.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.textures.is_empty()
    }

    /// Drops every registration.
    pub fn clear(&mut self) {
        self.textures.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::TextureMap;

    #[test]
    fn tracks_presence_without_a_texture_handle() {
        let mut map = TextureMap::new();
        assert!(map.is_empty());
        assert!(!map.contains(7));
        assert!(map.get(7).is_none());
        assert!(map.remove(7).is_none());
        map.clear();
        assert!(map.is_empty());
    }
}
