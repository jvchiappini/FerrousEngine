# ferrous_events

`ferrous_events` proporciona una capa de abstracción pura para la entrada del usuario. Su objetivo es desacoplar el sistema de UI del backend de ventanas (como `winit`) y del motor gráfico, permitiendo que los widgets reaccionen a eventos mediante un lenguaje común.

---

## Flujo de Eventos

1.  **Captura:** El motor (FerrousEngine) recibe eventos nativos del sistema operativo.
2.  **Conversión:** Traducción de tipos nativos (ej: `winit::KeyEvent`) a tipos internos de `ferrous_events` (ej: `UiEvent::KeyDown`).
3.  **Enrutamiento (Routing):** El `EventManager` determina qué widget debe recibir el evento basándose en el foco y la posición del ratón.

---

## Tipos de Eventos Principales

| Evento | Descripción |
|--------|-------------|
| `MouseDown` / `MouseUp` | Interacciones de botones físicos del ratón. |
| `MouseMove` | Actualización de la posición del puntero para efectos de *hover*. |
| `KeyDown` | Entrada de teclado, incluyendo tanto el código físico (`GuiKey`) como el texto interpretado. |

---

## Gestión de Estado: `EventManager`

El `EventManager` rastrea dos estados críticos para la interactividad:

- **Hovered Node:** El ID del nodo que tiene el cursor encima. Se usa para estados de resaltado (highlight).
- **Focused Node:** El ID del nodo que tiene el "foco". Recibe todos los eventos de teclado de forma exclusiva.

---

## Integración con `winit`

El crate incluye implementaciones de `From` para convertir automáticamente códigos de tecla de `winit` a `GuiKey`, garantizando que la lógica de la aplicación sea portable:

```rust
// Ejemplo de uso interno
let key: GuiKey = winit_key_code.into(); 
```

---

## Diseño de Referencia: `GuiKey`

Enumeración de teclas de navegación estándar:
- `Enter`, `Escape`, `Tab`, `Backspace`, `Delete`.
- `ArrowLeft`, `ArrowRight`, `ArrowUp`, `ArrowDown`.
- `Home`, `End`.
