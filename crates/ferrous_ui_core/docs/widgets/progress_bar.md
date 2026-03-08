# ProgressBar

`ProgressBar` es un widget informativo que visualiza el progreso de una tarea de larga duración. Soporta tanto modos determinados (0.0 a 1.0) como indeterminados.

## Características

- **Visualización de Carga:** Útil para pantallas de carga, exportación de archivos o progreso de descargas.
- **Soporte Reactivo:** Puede vincularse a un `Observable<f32>`.

## Estructura

```rust
pub struct ProgressBar {
    pub progress: f32, // 0.0 to 1.0
    pub is_indeterminate: bool,
}
```

## Ejemplo de Uso

```rust
let bar = ProgressBar::new(0.45); // 45% completo
```

## Estilo

- **Track:** `theme.surface_elevated`.
- **Fill:** `theme.primary` (o `theme.success`).
