# Separator

`Separator` es un widget estructural que dibuja una línea divisoria, ya sea horizontal o vertical, para organizar visualmente grupos de widgets.

## Características

- **Espesor Configurable:** Permite definir qué tan gruesa será la línea.
- **Orientación Automática:** Detecta su orientación basándose en las dimensiones otorgadas por el layout (si es más ancho que alto es horizontal, y viceversa).

## Ejemplo de Uso

```rust
use ferrous_ui_core::Separator;

let sep = Separator::new().with_thickness(2.0);
```

## Estilo

- **Color:** Utiliza por defecto `theme.on_surface_muted` con opacidad baja para un efecto sutil.
