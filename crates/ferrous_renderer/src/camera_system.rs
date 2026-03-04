//! `CameraSystem` — owns all per-camera GPU state and the orbit controller.
//!
//! Extraído de `ferrous_renderer::lib` (Fase 3 del roadmap de arquitectura).
//! `Renderer` ahora delega toda la lógica de cámara aquí, eliminando ~80 líneas
//! del god-struct.
//!
//! ## Responsabilidades
//! - Mantener el estado `Camera` (posición, target, fov, etc.)
//! - Mantener el `OrbitState` (mouse-orbit / WASD)
//! - Subir el uniform al GPU cuando cambia (via `GpuCamera::sync`)
//! - Proveer matrices view/proj para que los passes las lean desde `FramePacket`

use ferrous_core::scene::{Camera, Controller};
use ferrous_core::input::InputState;

use crate::camera::{GpuCamera, OrbitState};
use crate::pipeline::PipelineLayouts;

/// Owns camera CPU state + GPU uniform + orbit controller.
pub struct CameraSystem {
    /// CPU-side camera parameters.
    pub camera: Camera,
    /// Mouse/keyboard orbit controller state.
    pub orbit: OrbitState,
    /// GPU uniform buffer + bind group.
    pub gpu: GpuCamera,
}

impl CameraSystem {
    /// Create a default perspective camera at (0, 0, 5) looking at the origin.
    pub fn new(
        device: &wgpu::Device,
        layouts: &PipelineLayouts,
        width: u32,
        height: u32,
    ) -> Self {
        let camera = Camera {
            eye: glam::Vec3::new(0.0, 0.0, 5.0),
            target: glam::Vec3::ZERO,
            up: glam::Vec3::Y,
            fovy: 45.0f32.to_radians(),
            aspect: width as f32 / height as f32,
            znear: 0.1,
            zfar: 2000.0,
            controller: Controller::with_default_wasd(),
        };
        let gpu = GpuCamera::new(device, &camera, &layouts.camera);
        CameraSystem {
            camera,
            orbit: OrbitState::default(),
            gpu,
        }
    }

    // -----------------------------------------------------------------------

    /// Apply keyboard/mouse input to the orbit camera.  `dt` is seconds.
    pub fn handle_input(&mut self, input: &mut InputState, dt: f32) {
        self.orbit.update(&mut self.camera, input, dt);
    }

    /// Upload camera matrices to the GPU uniform buffer if they have changed.
    /// Should be called once per frame before building the `FramePacket`.
    pub fn sync_gpu(&mut self, queue: &wgpu::Queue) {
        self.gpu.sync(queue, &self.camera);
    }

    /// Set the camera aspect ratio (called on resize).
    pub fn set_aspect(&mut self, aspect: f32) {
        self.camera.set_aspect(aspect);
    }

    /// Compute the view matrix from current camera state.
    #[inline]
    pub fn view_matrix(&self) -> glam::Mat4 {
        glam::Mat4::look_at_rh(self.camera.eye, self.camera.target, self.camera.up)
    }

    /// Compute the projection matrix from current camera state.
    #[inline]
    pub fn proj_matrix(&self) -> glam::Mat4 {
        glam::Mat4::perspective_rh(
            self.camera.fovy,
            self.camera.aspect,
            self.camera.znear,
            self.camera.zfar,
        )
    }

    /// View-projection matrix (view * proj in row-major, suitable for GPU).
    #[inline]
    pub fn view_proj(&self) -> glam::Mat4 {
        self.proj_matrix() * self.view_matrix()
    }

    /// Current camera eye position.
    #[inline]
    pub fn eye(&self) -> glam::Vec3 {
        self.camera.eye
    }

    /// Current look-at target.
    #[inline]
    pub fn target(&self) -> glam::Vec3 {
        self.camera.target
    }
}
