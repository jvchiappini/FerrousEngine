# Panel

`Panel` es el contenedor visual básico de la interfaz. Proporciona una superficie (fondo) con bordes configurables que sirve para agrupar otros widgets.

> **Import** — `ferrous_ui_core::Panel`

## Estructura

```rust
pub struct Panel {
    pub color: Option<Color>,
    pub radius: Option<f32>,
}
```

- `color`: El color de fondo. Si es `None`, se utiliza `theme.surface`.
- `radius`: El radio de las esquinas. Si es `None`, se utiliza `theme.border_radius`.

## Ejemplo de Uso

Los paneles suelen actuar como raíces de subárboles en la macro `ui!`:

```rust
ui! {
    Panel() {
        Label("Interior del panel")
        Button("Aceptar")
    }
}
```

También pueden personalizarse individualmente:

```rust
let custom_panel = Panel::new()
    .with_color(Color::hex("#1A1A2E"))
    .with_radius(12.0);
```

## Builder API

| Método | Descripción |
|--------|-------------|
| `new()` | Crea un panel con valores por defecto del tema. |
| `with_color(color)` | Sobrescribe el color de fondo. |
| `with_radius(radius)` | Sobrescribe el radio de las esquinas. |

## Layout y Comportamiento

- **Tamaño**: Por defecto, un panel intentará llenar el espacio disponible definido por sus restricciones de layout (ej. `fill()`, `fill_width()`).
- **Jerarquía**: Cualquier widget añadido como hijo de un `Panel` quedará confinado rígidamente dentro de su área (si se activa `Overflow::Hidden` en el estilo).
- **Z-Order**: Los paneles se dibujan antes que sus hijos, actuando como plano de fondo.

## Notas

- El `Panel` en `ferrous_ui_core` es puramente visual y jerárquico. La lógica de cómo se disponen sus hijos (filas, columnas, etc.) se define mediante el `Style` del nodo (`DisplayMode::FlexRow`, `FlexColumn`) y no por el widget en sí.
