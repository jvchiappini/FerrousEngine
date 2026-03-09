# Separator

`Separator` es un widget visual minimalista que dibuja una línea sutil para dividir secciones en un layout.

> **Import** — `ferrous_ui_core::Separator`

## Estructura

```rust
pub struct Separator {
    pub color: Option<Color>,
}
```

- `color`: El color de la línea. Si es `None`, se utiliza `theme.on_surface_muted` con un 10% de opacidad para una integración discreta.

## Ejemplo de Uso

```rust
ui! {
    FlexColumn() {
        Label("Sección superior")
        Separator()
        Label("Sección inferior")
    }
}
```

## Builder API

| Método | Descripción |
|--------|-------------|
| `new()` | Crea un separador con el color tenue por defecto. |
| `with_color(color)` | Permite personalizar el color de la línea. |

## Detalles de Dibujo

- El separador siempre se dibuja como una línea horizontal de **1px** de grosor situada en el centro geométrico de su área asignada.
- Su `calculate_size` sugiere una altura de 1px, lo que permite que en layouts de tipo `Column`, ocupe exactamente lo necesario, mientras que en `Row` puede expandirse verticalmente (aunque seguirá pintando solo el píxel central).
