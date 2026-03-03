pub mod font;
pub mod texture;
pub mod gltf_loader;

// Exponemos la estructura principal para que sea fácil de importar
pub use font::Font;
pub use texture::Texture2d;
pub use gltf_loader::{AssetMesh, AssetModel, RawMaterial, load_gltf};

#[cfg(test)]
mod tests {
	use super::*;
	use std::path::Path;

	#[test]
	fn missing_file_returns_error() {
		let res = load_gltf(Path::new("this-file-does-not-exist.gltf"));
		assert!(res.is_err());
	}
}
