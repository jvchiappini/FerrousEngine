# ScrollView

`ScrollView` es un contenedor que permite desplazar su contenido cuando este excede las dimensiones del widget. Soporta desplazamiento vertical y horizontal.

## Características

- **Clipping Automático:** Utiliza `overflow: Scroll` y `PushClip`/`PopClip` para asegurar que el contenido que sale del área no se dibuje fuera.
- **Rastreo de Offset:** Mantiene un `scroll_offset` que se aplica a todos los comandos de sus hijos.

## Estructura

```rust
pub struct ScrollView {
    pub scroll_offset: Vec2,
    pub show_bars: bool,
}
```

## Ejemplo de Uso

```rust
let scroll = ScrollView::new()
    .with_horizontal_scroll(false) // Solo vertical
    .with_vertical_scroll(true);
```

## Integración con Layout

Cuando un widget está dentro de un `ScrollView`, el sistema de layout le permite crecer más allá de los límites del contenedor, y el `ScrollView` captura los eventos de rueda de ratón para actualizar el `scroll_offset`.
25
