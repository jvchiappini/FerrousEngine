//! Voxel render passes.
//!
//! Each pass implements [`ferrous_render_graph::RenderPass`] and can be
//! registered with the renderer via `renderer.add_pass(Box::new(pass))`.
//!
//! | Pass                  | Added in  | Purpose                                    |
//! |-----------------------|-----------|--------------------------------------------|
//! | `VoxelGpuUploadPass`  | Phase 2   | Sync dirty DAG nodes CPU → GPU             |
//! | `HddaPrimaryPass`     | Phase 3   | HDDA raymarching → G-Buffer                |
//! | `ReStirPass`          | Phase 6   | ReSTIR DI candidates + visibility + reuse  |
//! | `GiPass`              | Phase 8   | SSRC + DDGI + WSRC                         |
//! | `SvgfPass`            | Phase 7   | Temporal accrual + wavelet denoiser        |
//! | `TaaPass`             | Phase 9   | TAA jitter + composite                     |

pub mod gpu_upload_pass;
pub mod hdda_pass;

pub use gpu_upload_pass::VoxelGpuUploadPass;
pub use hdda_pass::HddaPrimaryPass;
