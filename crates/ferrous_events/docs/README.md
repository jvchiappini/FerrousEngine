# ferrous_events

Capa de abstracción de eventos de entrada para el sistema de UI de Ferrous Engine.

Separa completamente la lógica de interacción del motor gráfico y del sistema de
ventanas del sistema operativo. Los widgets nunca conocen a `winit` — solo hablan
con los tipos definidos en este crate y en `ferrous_ui_core`.

---

## Responsabilidades

| Área | Descripción |
|---|---|
| **Hit-testing** | Determina qué nodo del árbol contiene un punto dado (posición del cursor) |
| **Event bubbling** | Propaga eventos desde el nodo hoja hacia los padres hasta que uno los consume |
| **Estado hover/foco** | Rastrea qué nodo está bajo el cursor (`hovered_node`) y cuál tiene el foco de teclado (`focused_node`) |
| **Adaptadores winit** | Convierte tipos nativos de `winit` a los tipos abstractos de Ferrous (`GuiKey`, `MouseButton`) |

---

## Struct principal: `EventManager`

```rust
pub struct EventManager {
    pub hovered_node: Option<NodeId>,
    pub focused_node: Option<NodeId>,
}
```

### Métodos

#### `new() -> EventManager`
Crea el gestor sin estado activo.

#### `hit_test(&self, tree, pos) -> Option<NodeId>`
Recorre el `UiTree` recursivamente de atrás hacia adelante (los hijos más
recientes están "encima") y devuelve el `NodeId` más profundo cuyo `Rect`
contiene el punto `pos`.

#### `dispatch_event(&mut self, tree, app, event)`
Punto de entrada principal del loop de eventos. Dado un `UiEvent` crudo,
decide a qué nodo enviarlo y con qué semántica:

| Evento | Comportamiento |
|---|---|
| `MouseMove` | Hit-test → emite `MouseEnter` / `MouseLeave` si el hover cambió, luego propaga `MouseMove` al nodo hovered |
| `MouseDown` | Hit-test → actualiza `focused_node` → bubble |
| `MouseUp` | Hit-test → bubble |
| `KeyDown` / `KeyUp` / `Char` | Enviados directamente al `focused_node` → bubble |
| `MouseWheel` | Enviado al `hovered_node` → bubble |

#### Event bubbling
Si el nodo destino devuelve `EventResponse::Ignored`, el evento sube
automáticamente al nodo padre. El burbujeo se detiene cuando un nodo devuelve
`Consumed` o `Redraw`, o cuando se alcanza la raíz.

Cuando un nodo devuelve `Redraw`, el manager llama a
`tree.mark_paint_dirty(id)` para invalidar ese nodo de forma eficiente.

---

## Adaptadores de `winit`

### `winit_to_guikey(KeyCode) -> GuiKey`
Convierte un `winit::keyboard::KeyCode` al enum abstracto `GuiKey`:

```
Backspace, Delete, ArrowLeft/Right/Up/Down, Home, End, Enter, Escape, Tab
```

### `winit_to_mousebutton(MouseButton) -> ferrous_ui_core::MouseButton`
Convierte `winit::event::MouseButton` a `ferrous_ui_core::MouseButton`
(`Left`, `Right`, `Middle`).

---

## Flujo de datos

```
winit event
    │
    ▼
winit_to_guikey / winit_to_mousebutton   ← conversión al tipo abstracto
    │
    ▼
EventManager::dispatch_event(tree, app, UiEvent)
    │
    ├─ hit_test()           → NodeId destino
    ├─ send_to_node()       → Widget::on_event(&mut EventContext)
    │       │
    │       └─ EventResponse::Redraw → mark_paint_dirty()
    └─ bubble_event()       → sube al padre si Ignored
```

---

## Dependencias

| Crate | Uso |
|---|---|
| `ferrous_ui_core` | `NodeId`, `UiTree`, `UiEvent`, `Rect`, `EventContext`, `EventResponse`, `GuiKey`, `MouseButton` |
| `glam` | `Vec2` para posiciones |
| `winit` | Tipos nativos de entrada del OS para los adaptadores |

---

## Uso típico (dentro de `ferrous_gui`)

```rust
use ferrous_events::{EventManager, winit_to_guikey, winit_to_mousebutton};
use ferrous_ui_core::UiEvent;

// En el loop de winit:
let event = UiEvent::MouseDown {
    button: winit_to_mousebutton(winit_button),
    pos: glam::vec2(mx, my),
};
event_manager.dispatch_event(&mut ui_tree, &mut app_state, event);
```

Este crate **no debe usarse directamente** en aplicaciones — el orquestador
`ferrous_gui::UiSystem` lo integra y expone `dispatch_event` de forma unificada.


`ferrous_events` proporciona una capa de abstracción pura para la entrada del usuario. Su objetivo es desacoplar el sistema de UI del backend de ventanas (como `winit`) y del motor gráfico, permitiendo que los widgets reaccionen a eventos mediante un lenguaje común.

---

## Module overview

| Tipo | Descripción |
|------|-------------|
| `UiEvent` | Enum de todos los eventos posibles en la UI (mouse, teclado, foco). |
| `EventResponse` | Respuesta que un widget devuelve al sistema de eventos. |
| `EventManager` | Enrutador: decide qué nodo recibe cada evento en función del hit-test y el foco. |
| `GuiKey` | Enumeración ligera de teclas de navegación estándar, independiente de `winit`. |

---

## Tipos de Eventos (`UiEvent`)

| Variante | Datos | Descripción |
|----------|-------|-------------|
| `MouseDown` | `position: [f32; 2]`, `button` | Botón del ratón pulsado. |
| `MouseUp` | `position: [f32; 2]`, `button` | Botón del ratón soltado. |
| `MouseMove` | `position: [f32; 2]` | Movimiento del puntero. |
| `MouseEnter` | — | El puntero entra en el área del nodo. |
| `MouseLeave` | — | El puntero sale del área del nodo. |
| `KeyDown` | `key: Option<GuiKey>`, `text: Option<String>` | Tecla pulsada con mapeo opcional de texto. |

---

## Respuestas de Evento (`EventResponse`)

Un widget devuelve un `EventResponse` al procesar cada evento:

| Variante | Significado |
|----------|-------------|
| `Ignored` | El widget no ha consumido el evento; el sistema lo propagará al padre. |
| `Consumed` | El widget ha procesado el evento; la propagación se detiene. |
| `Redraw` | El widget ha cambiado de aspecto visual; el sistema lo marca como `PaintDirty`. |

---

## Flujo de Eventos

```
Sistema Operativo / winit
         │
         ▼
   Traducción de tipos
  (winit → UiEvent)
         │
         ▼
   EventManager
         │
         ├─ Hit-Test → NodeId del nodo bajo el cursor
         │
         ▼
   Dispatching (Bubbling)
   Nodo objetivo → padre → abuelo → …
         │
         ▼
   Widget::on_event(&mut ctx, &event) → EventResponse
```

### Hit-Testing Preciso

El `EventManager` realiza un recorrido del árbol de UI de atrás hacia adelante (Z-order) usando los `Rect` resueltos por el motor de layout. Se encuentra el nodo más profundo y visible bajo el puntero del ratón.

### Event Bubbling (Propagación)

1. El evento se envía al nodo objetivo (resultado del hit-test).
2. Si devuelve `Ignored`, el evento se propaga a su padre.
3. Si devuelve `Consumed` o `Redraw`, la propagación se detiene.
4. `Redraw` además lanza `mark_paint_dirty` sobre el nodo automáticamente.

---

## `GuiKey` — Teclas de Navegación

Enumeración independiente de `winit`, portable entre plataformas:

```rust
pub enum GuiKey {
    Enter, Escape, Tab,
    Backspace, Delete,
    ArrowLeft, ArrowRight, ArrowUp, ArrowDown,
    Home, End,
}
```

La conversión desde `winit::KeyCode` está implementada vía `From`:

```rust
// Automático al recibir eventos de teclado de winit
let key: Option<GuiKey> = winit_logical_key.into();
```

---

## Integración con `winit`

El motor convierte los eventos de `winit` al sistema abstracto de `ferrous_events` antes de pasarlos al `EventManager`. El código de UI **nunca** depende de `winit` directamente.

---

## Further reading

- [Referencia de eventos detallada](EVENTS.md)
- [Árbol de UI — ferrous_ui_core](../../ferrous_ui_core/docs/README.md)
