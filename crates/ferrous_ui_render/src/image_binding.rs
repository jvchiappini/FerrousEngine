//! Runtime selection of the GUI image-binding strategy.
//!
//! Desktop Vulkan/DX12/Metal expose `TEXTURE_BINDING_ARRAY` and
//! `SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING`, so the GUI
//! samples its images from one bind group holding a descriptor array.  GL,
//! WebGL2 and older Android drivers lack those features, so the same batches
//! are drawn from a layout with one texture + sampler binding per slot
//! (`assets/shaders/gui_web.wgsl`).  Both paths share the drawing and batching
//! code; only the shader source and the image bind group differ.

/// How the GUI image pass resolves textures on a device.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ImageBindingMode {
    /// One descriptor array of [`MAX_TEXTURE_SLOTS`](crate::MAX_TEXTURE_SLOTS)
    /// textures and samplers (`gui.wgsl`).
    Bindless,
    /// One texture + sampler binding per slot (`gui_web.wgsl`).
    Separate,
}

/// Number of texture slots in [`ImageBindingMode::Separate`].
///
/// `assets/shaders/gui_web.wgsl` declares eight `texture_2d` / `sampler`
/// pairs; a batch with more distinct textures keeps only the first eight on
/// that path (the rest sample the last bound slot, as on the existing web
/// backend).
pub const SEPARATE_TEXTURE_SLOTS: u32 = 8;

/// The adapter features the bindless strategy needs.
pub const BINDLESS_FEATURES: wgpu::Features = wgpu::Features::TEXTURE_BINDING_ARRAY
    .union(wgpu::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING);

impl ImageBindingMode {
    /// Selects the strategy for the features enabled on a device.
    ///
    /// `wgpu::Device::features()` reports the features the device was
    /// *created with*, not everything the adapter can do, so asking for a
    /// minimal device (as the headless GL test does) exercises the separate
    /// path on any adapter.
    #[must_use]
    pub fn for_features(features: wgpu::Features) -> Self {
        if features.contains(BINDLESS_FEATURES) {
            Self::Bindless
        } else {
            Self::Separate
        }
    }

    /// Whether the strategy samples through a descriptor array.
    #[must_use]
    pub fn is_bindless(self) -> bool {
        matches!(self, Self::Bindless)
    }

    /// Number of texture slots the matching bind group layout exposes.
    #[must_use]
    pub fn slots(self) -> u32 {
        match self {
            Self::Bindless => crate::MAX_TEXTURE_SLOTS,
            Self::Separate => SEPARATE_TEXTURE_SLOTS,
        }
    }

    /// Stable name used in pipeline labels and logs.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Bindless => "bindless",
            Self::Separate => "separate",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ImageBindingMode, BINDLESS_FEATURES, SEPARATE_TEXTURE_SLOTS};

    #[test]
    fn a_device_with_both_features_uses_the_bindless_array() {
        assert_eq!(
            ImageBindingMode::for_features(BINDLESS_FEATURES),
            ImageBindingMode::Bindless
        );
        assert!(ImageBindingMode::Bindless.is_bindless());
    }

    #[test]
    fn a_gl_like_device_uses_separate_bindings() {
        // GL/WebGL2 expose neither feature.  Either one missing is enough to
        // take the compatible path.
        assert_eq!(
            ImageBindingMode::for_features(wgpu::Features::empty()),
            ImageBindingMode::Separate
        );
        assert_eq!(
            ImageBindingMode::for_features(wgpu::Features::TEXTURE_BINDING_ARRAY),
            ImageBindingMode::Separate
        );
        assert_eq!(
            ImageBindingMode::for_features(
                wgpu::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING
            ),
            ImageBindingMode::Separate
        );
        assert!(!ImageBindingMode::Separate.is_bindless());
    }

    #[test]
    fn separate_mode_reports_the_shader_slot_count() {
        assert_eq!(ImageBindingMode::Bindless.slots(), 16);
        assert_eq!(ImageBindingMode::Separate.slots(), SEPARATE_TEXTURE_SLOTS);
        assert_eq!(ImageBindingMode::Separate.slots(), 8);
    }
}
