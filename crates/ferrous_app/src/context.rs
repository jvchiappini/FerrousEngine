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
    /// referencia opcional al renderer 3D. Algunas etapas del ciclo de vida
    /// proporcionan un `Some` para que la aplicación pueda manipular la
    /// cámara u otros detalles renderer-specific.  Puede ser `None` durante
    /// ciertos callbacks como `on_window_event` antes de que se haya iniciali
    /// zado el sistema gráfico.
    pub renderer: Option<&'a mut ferrous_renderer::Renderer>,
    pub(crate) exit_requested: bool,
}

impl<'a> AppContext<'a> {
    pub fn request_exit(&mut self) {
        self.exit_requested = true;
    }

    /// Convenience accessor returning a mutable reference to the renderer if
    /// available.
    pub fn renderer(&mut self) -> Option<&mut ferrous_renderer::Renderer> {
        self.renderer.as_deref_mut()
    }
}
