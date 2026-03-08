# Button

`Button` es un widget interactivo base que permite clics y callbacks en el modo retenido. Detecta automáticamente interacciones del ratón (hover, press) y provee una sintaxis fluida para manejar `on_click`, `on_hover` y `on_hover_end`.

## Diseño Retenido (Lag Cero)

En `ferrous_ui_core`, el botón almacena sus propios callbacks dentro del `Widget` en vez de requerir polling manual (a diferencia del antiguo `Button` en modo inmediato de `ferrous_gui`).

## Fields Principales

```rust
pub struct Button {
    pub label: String,
    pub color: [f32; 4],
    pub hover_color: [f32; 4],
    pub text_color: [f32; 4],
    pub border_radius: f32,
    pub is_hovered: bool,
}
```

## Consrucción y Uso

Los closures asociados al botón deben cumplir los bounds `Send + Sync + 'static` para permitir ejecución segura en entornos multi-hilo (y eventual envío de comandos).

```rust
use ferrous_ui_core::Button;

let btn = Button::new("Eliminar")
    .on_click(|| {
        println!("Elemento eliminado");
    })
    .on_hover(|| println!("hover activado"))
    .with_radius(6.0)
    .with_color([0.2, 0.2, 0.2, 1.0])
    .with_hover_color([0.3, 0.3, 0.3, 1.0]);
```

## Propagación de EventFeedbacks (`EventResponse::Redraw`)

El widget genera automáticamente `EventResponse::Redraw` durante los eventos `MouseEnter` y `MouseLeave`, lo que notifica al `Tree` que el subárbol marcado como "sucio visualmente" necesita recolectar nuevamente el color del botón.
