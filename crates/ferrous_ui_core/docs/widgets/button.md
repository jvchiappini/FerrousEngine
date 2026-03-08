# Button

`Button` es un widget interactivo base que permite clics y callbacks en el modo retenido. Detecta automáticamente interacciones del ratón (hover, press) y provee una sintaxis fluida para manejar `on_click`, `on_hover` y `on_hover_end`.

## Diseño Retenido y Genérico

En `ferrous_ui_core`, el botón es genérico sobre el estado de la aplicación `App`. Los callbacks reciben un `EventContext<App>`, lo que permite modificar el estado global de la aplicación directamente desde el botón.

## Estructura

```rust
pub struct Button<App> {
    pub label: String,
    pub is_hovered: bool,
    // Callbacks internos (Boxed closures)
}
```

> [!NOTE]
> A diferencia de versiones anteriores, el color y el radio de borde ya no se almacenan en el struct, sino que se obtienen dinámicamente del `Theme` durante la fase de dibujo, asegurando consistencia visual.

## Construcción y Uso

Los closures asociados al botón deben cumplir los bounds `Send + Sync + 'static`.

```rust
use ferrous_ui_core::Button;

// Definimos nuestra App
struct MyState { count: i32 }

let btn = Button::<MyState>::new("Incrementar")
    .on_click(|ctx| {
        ctx.app.count += 1;
        println!("Contador: {}", ctx.app.count);
    })
    .on_hover(|_ctx| println!("Hovering!"));
```

## Comportamiento Visual

El botón utiliza los colores semánticos del tema:
- **Normal:** `theme.primary`
- **Hover:** `theme.primary_variant`
- **Texto:** `theme.on_primary`

## Eventos y Redibujado

El widget genera automáticamente `EventResponse::Redraw` durante los eventos `MouseEnter` y `MouseLeave`, lo que notifica al `Tree` que el subárbol necesita recolectar nuevamente los comandos de dibujo para reflejar el cambio de color (hover).
