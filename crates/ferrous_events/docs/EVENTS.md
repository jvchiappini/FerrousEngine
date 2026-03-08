# ⚡ Ferrous Events — Referencia Técnica

`ferrous_events` es la abstracción pura de eventos de entrada para la interfaz de usuario de FerrousEngine.

---

## 🛡️ Estructuras de Eventos

### `UiEvent`
Define las señales comunes en cualquier interfaz gráfica:

```rust
pub enum UiEvent {
    MouseDown { position: [f32; 2], button: MouseButton },
    MouseUp   { position: [f32; 2], button: MouseButton },
    MouseMove { position: [f32; 2] },
    MouseEnter,
    MouseLeave,
    KeyDown { key: Option<GuiKey>, text: Option<String> },
}
```

### `EventResponse`
Respuesta que los widgets devuelven al sistema de eventos tras procesar un `UiEvent`:

```rust
pub enum EventResponse {
    /// No procesado; el sistema lo propagará al nodo padre.
    Ignored,
    /// Procesado; la propagación se detiene.
    Consumed,
    /// Procesado y el widget necesita repintarse; marca PaintDirty automáticamente.
    Redraw,
}
```

### `EventManager`
Capa de lógica que enruta los eventos a los nodos correctos del `UiTree`:
- **`hovered_node`**: ID del nodo bajo el puntero; se actualiza en cada `MouseMove`.
- **`focused_node`**: ID del receptor exclusivo de eventos de teclado.
- Envía automáticamente `MouseEnter` / `MouseLeave` al cambiar el nodo apuntado.

---

## 🚀 Hit-Testing y Propagación

### Hit-Testing Progresivo

El `EventManager` recorre el árbol de UI de atrás hacia adelante (mayor Z-order primero), comparando el punto del puntero con el `Rect` resuelto de cada nodo. Se detiene en el nodo más profundo y visible que contiene el punto.

El método `Rect::contains([x, y])` implementa esta comprobación en O(1):
```rust
// Interno del EventManager
if node.rect.contains(mouse_pos) && !found {
    found = Some(node_id);
}
```

### 💧 Event Bubbling (Propagación)
Los eventos siguen un flujo de "burbujeo":
1.  El evento se envía primero al nodo hijo más profundo (el objetivo del hit-test).
2.  El widget procesa el evento y devuelve un `EventResponse`.
3.  Si la respuesta es `Ignored`, el evento se propaga automáticamente a su padre jerárquico.
4.  Si la respuesta es `Consumed` o `Redraw`, la propagación se detiene.

### 🎨 Visual Feedback: `EventResponse::Redraw`
Permite que un widget notifique que su representación visual ha cambiado (por ejemplo, al activarse un hover). Al recibir esta señal, el `EventManager` marca automáticamente el nodo como `PaintDirty` en el `UiTree`.

```rust
// Ejemplo en Button::on_event
fn on_event(&mut self, _ctx: &mut EventContext, event: &UiEvent) -> EventResponse {
    match event {
        UiEvent::MouseEnter => { self.is_hovered = true; EventResponse::Redraw }
        UiEvent::MouseLeave => { self.is_hovered = false; EventResponse::Redraw }
        UiEvent::MouseDown { .. } => EventResponse::Consumed,
        _ => EventResponse::Ignored,
    }
}
```

---

## 📦 Conversión de Winit

Incluye implementaciones de `From` para mapear teclas físicas del backend de ventanas a los tipos abstractos del motor. La lógica de la UI **nunca** depende de `winit` directamente.

```rust
// Conversión automática al recibir eventos de winit
let key: Option<GuiKey> = winit_keycode.into();

// GuiKey cubre el conjunto mínimo de teclas de navegación UI:
// Enter, Escape, Tab, Backspace, Delete
// ArrowLeft, ArrowRight, ArrowUp, ArrowDown
// Home, End
```
