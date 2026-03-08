//! `ferrous_events` — Abstracción pura de eventos de entrada para la interfaz de usuario.
//!
//! Este crate separa la lógica de interacción del motor gráfico y del sistema de ventanas (OS).
//! Proporciona un lenguaje común para que los widgets reaccionen a clicks, movimientos,
//! y pulsaciones de teclas sin conocer a `winit` o APIs similares.

use ferrous_ui_core::NodeId;
use glam::Vec2;

/// Representación unificada de una acción del usuario en la interfaz.
#[derive(Debug, Clone)]
pub enum UiEvent {
    /// El usuario presionó un botón del ratón en una coordenada específica.
    MouseDown { button: MouseButton, pos: Vec2 },
    /// El usuario liberó un botón del ratón.
    MouseUp { button: MouseButton, pos: Vec2 },
    /// Movimiento del puntero dentro de la ventana.
    MouseMove { pos: Vec2 },
    /// Pulsación de tecla física o evento de entrada de texto.
    KeyDown { 
        /// Representación textual si aplica (ej. "a", "€").
        text: String, 
        /// Código de tecla abstracto para funciones lógicas (ej. `GuiKey::Enter`).
        code: Option<GuiKey> 
    },
}

/// Enumeración de teclas especiales y de navegación comunes en aplicaciones de escritorio.
/// Esta lista es agnóstica al teclado físico y se mapea desde el backend de eventos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum GuiKey {
    Backspace,
    Delete,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    Home,
    End,
    Enter,
    Escape,
    Tab,
}

/// Botones del ratón soportados por el sistema de enrutamiento de UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Gestor del estado de foco y hover del sistema de eventos.
/// Mantiene rastro de qué nodos del `UiTree` están interactuando con el usuario.
pub struct EventManager {
    /// Nodo que actualmente tiene el cursor encima. Utilizado para efectos visuales (Highlight).
    pub hovered_node: Option<NodeId>,
    /// Nodo que posee el foco del teclado. Todos los `KeyDown` se dirigirán aquí primero.
    pub focused_node: Option<NodeId>,
}

impl EventManager {
    /// Inicializa un gestor de eventos sin estado activo.
    pub fn new() -> Self {
        Self {
            hovered_node: None,
            focused_node: None,
        }
    }
}

/// Conversión desde códigos de tecla de `winit` al lenguaje interno de Ferrous UI.
/// Esto permite que el motor use `winit` como proveedor de eventos sin acoplar los crates de la UI.
impl From<winit::keyboard::KeyCode> for GuiKey {
    fn from(k: winit::keyboard::KeyCode) -> Self {
        match k {
            winit::keyboard::KeyCode::Backspace => GuiKey::Backspace,
            winit::keyboard::KeyCode::Delete => GuiKey::Delete,
            winit::keyboard::KeyCode::ArrowLeft => GuiKey::ArrowLeft,
            winit::keyboard::KeyCode::ArrowRight => GuiKey::ArrowRight,
            winit::keyboard::KeyCode::ArrowUp => GuiKey::ArrowUp,
            winit::keyboard::KeyCode::ArrowDown => GuiKey::ArrowDown,
            winit::keyboard::KeyCode::Home => GuiKey::Home,
            winit::keyboard::KeyCode::End => GuiKey::End,
            winit::keyboard::KeyCode::Enter | winit::keyboard::KeyCode::NumpadEnter => GuiKey::Enter,
            winit::keyboard::KeyCode::Escape => GuiKey::Escape,
            winit::keyboard::KeyCode::Tab => GuiKey::Tab,
            _ => GuiKey::Backspace, // Desvío por defecto para teclas no manejadas
        }
    }
}
