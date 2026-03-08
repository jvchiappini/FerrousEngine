# Label

`Label` es un widget simple para mostrar texto estático. Es ligero y eficiente, ideal para descripciones, títulos y etiquetas de otros controles.

## Características

- **Tamaño Intrínseco:** El sistema de layout calcula automáticamente el ancho y alto basándose en la longitud del texto y el tamaño de la fuente.
- **Tematizado:** Usa `theme.on_surface` y `theme.font_size_base` por defecto.

## Estructura

```rust
pub struct Label {
    pub text: String,
    pub color: Option<Color>,
    pub font_size: Option<f32>,
}
```

## Ejemplo de Uso

```rust
use ferrous_ui_core::{Label, Color};

// Uso básico (usa valores del tema)
let l1 = Label::new("Usuario:");

// Uso con personalización
let l2 = Label::new("Alerta!")
    .with_color(Color::hex("#FF0000"))
    .with_size(18.0);
```

## Notas de Layout

`Label` implementa `calculate_size`, lo que permite que contenedores como `Panel` con modo `Flex` lo posicionen correctamente sin necesidad de especificar dimensiones manuales.
