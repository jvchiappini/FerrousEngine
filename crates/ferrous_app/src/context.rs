use ferrous_core::InputState;
use ferrous_renderer::Viewport;
use winit::window::Window;

pub struct AppContext<'a> {
    pub input: &'a InputState,
    pub dt: f32,
    pub window_size: (u32, u32),
    pub window: &'a Window,
    /// Área destinada al 3D. Si la app es pura UI, puede ser de tamaño 0.
    pub viewport: Viewport,
    pub(crate) exit_requested: bool,
}

impl<'a> AppContext<'a> {
    pub fn request_exit(&mut self) {
        self.exit_requested = true;
    }
}
