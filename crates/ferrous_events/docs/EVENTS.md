# ⚡ Ferrous Events

`ferrous_events` es la abstracción pura de eventos de entrada para la interfaz de usuario de FerrousEngine. 

## 🛡️ Estructuras de Eventos

### 1. `UiEvent`
Define las señales comunes en cualquier interfaz gráfica:
- `MouseDown`, `MouseUp`, `MouseMove`.
- `KeyDown` (con mapeo de `GuiKey`).
- `MouseEnter`, `MouseLeave` (estados automáticos detectados por el motor).

### 2. `EventManager`
Capa de lógica que se comunica entre el motor y los widgets.
- **`hovered_node`**: Realiza un seguimiento del nodo bajo el ratón.
- **`focused_node`**: El receptor actual de las pulsaciones de teclado.

## 🚀 Hit-Testing y Propagación

### Hit-Testing Progresivo
Un algoritmo recursivo que recorre el `UiTree` para encontrar el nodo más profundo y visible que contiene un punto (el puntero del ratón). Los hermanos se recorren de atrás hacia adelante para detectar correctamente el "overlap" (Z-order).

### 💧 Event Bubbling (Propagación)
Los eventos siguen un flujo de "burbujeo":
1. El evento se envía primero al nodo hijo más profundo (el objetivo del hit-test).
2. El widget procesa el evento y devuelve un `EventResponse`.
3. Si la respuesta es `Ignored`, el evento se propaga automáticamente a su padre jerárquico.
4. Si la respuesta es `Consumed` o `Redraw`, la propagación se detiene.

### 🎨 Visual Feedback: `EventResponse::Redraw`
Permite que un widget notifique que su representación visual ha cambiado (por ejemplo, al activarse un hover). Al recibir esta señal, el `EventManager` marca automáticamente el nodo como `PaintDirty` en el `UiTree`.

## 📦 Conversión de Winit
Incluye funciones de utilidad (`winit_to_guikey`) para mapear teclas físicas del backend de ventanas a los tipos abstractos del motor.
