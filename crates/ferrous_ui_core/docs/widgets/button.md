# Button

`Button` es el widget interactivo fundamental del sistema de UI. Provee una superficie clicable con estados visuales de interacción (hover, press) y una API fluent para adjuntar comportamientos.

> **Import** — `ferrous_ui_core::Button`

## Estructura

En `ferrous_ui_core`, el botón es genérico sobre el estado de la aplicación `App`. Los callbacks reciben un `EventContext<App>`, lo que permite acceder y modificar el estado global de la aplicación, el árbol de UI o enviar comandos a la cola de ejecución.

```rust
pub struct Button<App> {
    pub label: String,
    pub is_hovered: bool,
    pub is_pressed: bool,
    // on_click_cb: Option<Box<dyn Fn(&mut EventContext<App>)>>
    // on_hover_cb: Option<Box<dyn Fn(&mut EventContext<App>)>>
    // on_hover_end_cb: Option<Box<dyn Fn(&mut EventContext<App>)>>
}
```

- `label` — El texto centrado dentro del área del botón.
- `is_hovered` — `true` mientras el cursor se encuentra dentro de los límites del botón.
- `is_pressed` — `true` mientras se mantiene pulsado el botón del ratón sobre el widget.

## Construcción y Uso

El botón soporta un estilo de construcción encadenado (Builder Pattern). Los closures deben cumplir con los requisitos `Send + Sync + 'static`.

```rust
use ferrous_ui_core::Button;

// 1. Definir el estado de la aplicación
struct MyWorld { entities: usize }

// 2. Crear el botón con lógica personalizada
let btn = Button::<MyWorld>::new("Crear Entidad")
    .on_click(|ctx| {
        ctx.app.entities += 1;
        println!("Nueva entidad creada. Total: {}", ctx.app.entities);
    })
    .on_hover(|_ctx| {
        // Lógica opcional al entrar el ratón
    });
```

## Builder API

| Método | Descripción |
|--------|-------------|
| `new(label)` | Crea una instancia con el texto inicial. |
| `on_click(closure)` | Define la acción al soltar el clic tras haber pulsado. |
| `on_hover(closure)` | Define la acción al entrar el cursor en el área del botón. |
| `on_hover_end(closure)` | Define la acción al salir el cursor del área del botón. |

## Comportamiento Visual y Temas

El botón delega su apariencia en el `Theme` configurado en el `DrawContext`. Esto asegura que todos los botones de la aplicación mantengan una estética coherente:

- **Fondo:** Usa `theme.primary` en estado normal.
- **Interacción:** El widget cambia a `theme.primary_variant` automáticamente cuando `is_hovered` es verdadero.
- **Tipografía:** El texto usa `theme.on_primary` para garantizar el máximo contraste.
- **Bordes:** Se aplica `theme.border_radius` de forma uniforme en las cuatro esquinas.

## Ciclo de Vida del Widget

1. **`calculate_size`**: El botón solicita un tamaño basado en el ancho del texto de su etiqueta más un margen interno (padding) predefinido.
2. **`draw`**: Genera un par de `RenderCommand`s: un `Quad` para el fondo y un `Text` para el contenido.
3. **`on_event`**:
   - `MouseEnter` / `MouseLeave`: Actualiza el estado interno y solicita un redibujado (`EventResponse::Redraw`).
   - `MouseDown`: Marca el botón como presionado.
   - `MouseUp`: Si se suelta dentro del área, dispara el callback `on_click`.

## Notas de Implementación

- A diferencia de los sistemas de modo inmediato, el botón no requiere ser recreado en cada frame. Su estado (hover/press) persiste en el `UiTree`.
- El hit-testing se realiza de forma precisa siguiendo la jerarquía de layouts calculada por el motor `ferrous_layout`.
