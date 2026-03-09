# ProgressBar

`ProgressBar` es un indicador visual que muestra el progreso de una tarea de larga duración o la cantidad de algo (ej. vida, carga, descarga).

> **Import** — `ferrous_ui_core::ProgressBar`

## Estructura

```rust
pub struct ProgressBar {
    pub progress: f32, // Rango 0.0 a 1.0
    pub binding: Option<Arc<Observable<f32>>>,
}
```

## Ejemplo de Uso

```rust
use ferrous_ui_core::ProgressBar;

// Barra estática al 50%
let p1 = ProgressBar::new(0.5);

// Barra reactiva vinculada a una descarga
let download_obs = Arc::new(Observable::new(0.0));
let p2 = ProgressBar::new(0.0)
    .with_binding(download_obs.clone(), node_id);
```

## Builder API

| Método | Descripción |
|--------|-------------|
| `new(progress)` | Crea una barra con progreso inicial [0.0 - 1.0]. |
| `with_binding(obs, id)` | Vincula el progreso a un `Observable<f32>`. |

## Personalización Visual

- **Track (Fondo):** Utiliza `theme.surface_variant`.
- **Fill (Llenado):** Utiliza `theme.primary`.
- **Redondeado:** Utiliza `theme.border_radius` del tema global.
- **Dimensiones:** Por defecto ocupa `200x8` px si no se especifica lo contrario en el estilo de layout.

## Notas

- El valor se clampa automáticamente entre `0.0` y `1.0` en cada frame antes de dibujar.
- Si el progreso es menor a `0.001`, la barra de llenado no se dibuja para evitar artefactos visuales de 1 píxel.
