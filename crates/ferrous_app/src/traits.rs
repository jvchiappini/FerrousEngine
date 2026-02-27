use crate::context::AppContext;
use ferrous_assets::font::Font;
use ferrous_gui::{GuiBatch, TextBatch, Ui};
use ferrous_renderer::Renderer;

/// Todos los métodos están vacíos por defecto.
/// ¡El usuario solo implementa lo que necesita!
#[allow(unused_variables)]
pub trait FerrousApp {
    /// Se llama una vez al iniciar.
    fn setup(&mut self, ctx: &mut AppContext) {}

    /// Lógica matemática y actualizaciones (60 veces por segundo).
    fn update(&mut self, ctx: &mut AppContext) {}

    /// Configura los widgets de la interfaz gráfica.
    fn configure_ui(&mut self, ui: &mut Ui) {}

    /// Dibuja el 2D y las interfaces gráficas. Ideal para apps empresariales o HUDs.
    fn draw_ui(
        &mut self,
        gui: &mut GuiBatch,
        text: &mut TextBatch,
        font: Option<&Font>,
        ctx: &mut AppContext,
    ) {
    }

    /// Dibuja el mundo 3D. Ideal para juegos. Se dibuja DEBAJO de la UI.
    fn draw_3d(&mut self, renderer: &mut Renderer, ctx: &mut AppContext) {}

    /// Para reaccionar a eventos crudos (teclas, redimensionamiento, arrastrar archivos).
    fn on_window_event(&mut self, event: &winit::event::WindowEvent, ctx: &mut AppContext) {}
}
