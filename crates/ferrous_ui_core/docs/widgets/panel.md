# Panel

`Panel` es el contenedor visual fundamental de `ferrous_ui_core`. Proporciona un fondo sólido y opcionalmente bordes redondeados. Se utiliza como base para construir ventanas, barras de herramientas y grupos de widgets.

## Características

- **Sin lógica de Layout:** El `Panel` no se encarga de posicionar a sus hijos; eso es responsabilidad del `UiTree` y el motor de layout (`ferrous_layout`) basado en el `Style` del nodo.
- **Fondo Flexible:** Puede usar el color por defecto del tema (`surface`) o uno personalizado.
- **Overflow:** Soporta recortes de contenido mediante la propiedad `overflow: Hidden` en su estilo.

## Estructura

```rust
pub struct Panel {
    pub color: Option<Color>,
    pub radius: Option<f32>,
}
```

## Ejemplo de Uso

```rust
use ferrous_ui_core::{Panel, Color, StyleBuilder};

let panel = Panel::new()
    .with_color(Color::hex("#2C2C2C"))
    .with_radius(12.0);

// El posicionamiento se define en el Style del nodo que contiene al Panel
tree.set_node_style(panel_id, StyleBuilder::new()
    .width_px(300.0)
    .height_px(400.0)
    .padding_all(10.0)
    .column()
    .build());
```

## Integración con Temas

Si no se especifica un color o radio, el `Panel` heredará automáticamente `theme.surface` y `theme.border_radius`.
