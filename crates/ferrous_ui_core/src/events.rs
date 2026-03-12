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
    /// Pulsación de tecla física.
    KeyDown {
        /// Código de tecla abstracto para funciones lógicas (ej. `GuiKey::Enter`).
        key: GuiKey,
    },
    /// Liberación de tecla física.
    KeyUp {
        /// Código de tecla abstracto para funciones lógicas (ej. `GuiKey::Enter`).
        key: GuiKey,
    },
    /// Entrada de texto (carácter Unicode).
    Char { c: char },
    /// Movimiento de la rueda del ratón. Escala nominalmente ±1.0 por "clic".
    MouseWheel { delta_x: f32, delta_y: f32 },
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
    /// Ctrl+A — seleccionar todo el texto del widget enfocado.
    CtrlA,
    /// Ctrl+← — saltar al inicio de la palabra anterior.
    CtrlArrowLeft,
    /// Ctrl+→ — saltar al final de la siguiente palabra.
    CtrlArrowRight,
    /// Shift+← — extender la selección un carácter a la izquierda.
    ShiftArrowLeft,
    /// Shift+→ — extender la selección un carácter a la derecha.
    ShiftArrowRight,
    /// Shift+Home — extender la selección hasta el inicio.
    ShiftHome,
    /// Shift+End — extender la selección hasta el final.
    ShiftEnd,
    /// Ctrl+Shift+← — extender la selección al inicio de la palabra anterior.
    CtrlShiftArrowLeft,
    /// Ctrl+Shift+→ — extender la selección al final de la siguiente palabra.
    CtrlShiftArrowRight,
    /// Ctrl+C — copiar la selección al portapapeles.
    CtrlC,
    /// Ctrl+X — cortar la selección al portapapeles.
    CtrlX,
    /// Ctrl+V — pegar desde el portapapeles.
    CtrlV,
    /// Ctrl+Z — deshacer el último cambio.
    CtrlZ,
    /// Ctrl+Y — rehacer el último cambio deshecho.
    CtrlY,
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
