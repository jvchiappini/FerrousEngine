# ferrous_events

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
