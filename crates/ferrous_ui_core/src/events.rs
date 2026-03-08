use glam::Vec2;
use serde::{Deserialize, Serialize};

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
    /// El ratón entró en el área del widget.
    MouseEnter,
    /// El ratón salió del área del widget.
    MouseLeave,
}

/// Enumeración de teclas especiales y de navegación comunes en aplicaciones de escritorio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Resultado de procesar un evento.
pub enum EventResponse {
    /// El evento fue ignorado por el widget.
    Ignored,
    /// El evento fue consumido pero no requiere cambios visuales inmediatos.
    Consumed,
    /// El evento fue consumido y el widget ha cambiado visualmente (necesita repintado).
    Redraw,
}
