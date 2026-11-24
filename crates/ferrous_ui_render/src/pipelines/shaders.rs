//! Shader modules shared by every GUI pipeline set.
//!
//! A `GuiRenderer` builds these once.  A second renderer on the same device
//! (a popup, Phase 8E) clones the modules instead of re-parsing the same WGSL,
//! and can then create its own pipelines from them when its target format or
//! sample count differs from the main window's.
//!
//! The modules are stored behind `Arc` (wgpu 23 handles are not `Clone`), so a
//! popup's clone is one pointer increment and keeps the compiled module alive
//! exactly as long as a renderer uses it.

use std::sync::Arc;

use super::{
    blur::blur_shader, id::id_shader, overdraw::overdraw_shader, quad::quad_shader,
    svg::svg_shader, text::text_shader,
};

/// The WGSL modules behind the GUI pipelines.
#[derive(Clone)]
pub struct GuiShaders {
    /// Rounded quads (`gui.wgsl` / `gui_web.wgsl` + clip mask).
    pub quad: Arc<wgpu::ShaderModule>,
    /// Text and icons (`msdf_common.wgsl` + clip mask + `text.wgsl`).
    pub text: Arc<wgpu::ShaderModule>,
    /// SVG vector paths (`svg.wgsl` + clip mask).
    pub svg: Arc<wgpu::ShaderModule>,
    /// The `R32Uint` node-id pass (`id_buffer.wgsl`).
    pub id: Arc<wgpu::ShaderModule>,
    /// Backdrop downsample and Gaussian (`backdrop_blur.wgsl`).
    pub blur: Arc<wgpu::ShaderModule>,
    /// Overdraw diagnostics (`gui_overdraw.wgsl`).
    pub overdraw: Arc<wgpu::ShaderModule>,
}

impl GuiShaders {
    /// Builds every module for `device`.
    #[must_use]
    pub fn new(device: &wgpu::Device) -> Self {
        Self {
            quad: Arc::new(quad_shader(device)),
            text: Arc::new(text_shader(device)),
            svg: Arc::new(svg_shader(device)),
            id: Arc::new(id_shader(device)),
            blur: Arc::new(blur_shader(device)),
            overdraw: Arc::new(overdraw_shader(device)),
        }
    }
}
