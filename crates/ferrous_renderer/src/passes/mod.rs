#[cfg(any(feature = "render-3d", not(feature = "flat2d")))]
pub mod antialiasing_pass;
#[cfg(any(feature = "render-3d", not(feature = "flat2d")))]
pub mod cel_pass;
#[cfg(any(feature = "render-3d", not(feature = "flat2d")))]
pub mod compute_pass;
#[cfg(all(
    any(feature = "render-3d", not(feature = "flat2d")),
    feature = "gpu-driven"
))]
pub mod cull_pass;
#[cfg(any(feature = "render-3d", not(feature = "flat2d")))]
pub mod flat_pass;
#[cfg(any(feature = "render-3d", not(feature = "flat2d")))]
pub mod outline_pass;
#[cfg(any(feature = "render-3d", not(feature = "flat2d")))]
pub mod particle_pass;
#[cfg(any(feature = "render-3d", not(feature = "flat2d")))]
pub mod post_process_pass;
#[cfg(any(feature = "render-3d", not(feature = "flat2d")))]
pub mod prepass;
#[cfg(any(feature = "render-3d", not(feature = "flat2d")))]
pub mod procedural_sky_pass;
#[cfg(any(feature = "render-3d", not(feature = "flat2d")))]
pub mod skinning_pass;
#[cfg(any(feature = "render-3d", not(feature = "flat2d")))]
pub mod skybox_pass;
#[cfg(any(feature = "render-3d", not(feature = "flat2d")))]
pub mod ssao_blur_pass;
#[cfg(any(feature = "render-3d", not(feature = "flat2d")))]
pub mod ssao_pass;
#[cfg(feature = "gui")]
pub mod ui_pass;
#[cfg(any(feature = "render-3d", not(feature = "flat2d")))]
pub mod world_pass;

#[cfg(any(feature = "render-3d", not(feature = "flat2d")))]
pub use antialiasing_pass::{AntialiasingMode, AntialiasingPass, FxaaParams};
#[cfg(any(feature = "render-3d", not(feature = "flat2d")))]
pub use cel_pass::{CelFrameData, CelShadedPass};
#[cfg(any(feature = "render-3d", not(feature = "flat2d")))]
pub use compute_pass::ComputePass;
#[cfg(all(
    any(feature = "render-3d", not(feature = "flat2d")),
    feature = "gpu-driven"
))]
pub use cull_pass::{CullParamsUniform, CullPass};
#[cfg(any(feature = "render-3d", not(feature = "flat2d")))]
pub use flat_pass::{FlatFrameData, FlatShadedPass};
#[cfg(any(feature = "render-3d", not(feature = "flat2d")))]
pub use outline_pass::{OutlineFrameData, OutlinePass};
#[cfg(any(feature = "render-3d", not(feature = "flat2d")))]
pub use particle_pass::ParticleSystem;
#[cfg(any(feature = "render-3d", not(feature = "flat2d")))]
pub use post_process_pass::PostProcessPass;
#[cfg(any(feature = "render-3d", not(feature = "flat2d")))]
pub use prepass::PrePass;
#[cfg(any(feature = "render-3d", not(feature = "flat2d")))]
pub use procedural_sky_pass::ProceduralSkyPass;
#[cfg(any(feature = "render-3d", not(feature = "flat2d")))]
pub use skinning_pass::SkinningPass;
#[cfg(any(feature = "render-3d", not(feature = "flat2d")))]
pub use skybox_pass::{SkyboxPass, SkyboxPipeline};
#[cfg(any(feature = "render-3d", not(feature = "flat2d")))]
pub use ssao_blur_pass::SsaoBlurPass;
#[cfg(any(feature = "render-3d", not(feature = "flat2d")))]
pub use ssao_pass::SsaoPass;
#[cfg(feature = "gui")]
pub use ui_pass::UiPass;
#[cfg(any(feature = "render-3d", not(feature = "flat2d")))]
pub use world_pass::{SkyMode, WorldPass};
