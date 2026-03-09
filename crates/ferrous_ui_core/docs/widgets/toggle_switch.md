# ToggleSwitch

`ToggleSwitch` es una alternativa visual al `Checkbox`, ideal para interfaces móviles o configuraciones de "activado/desactivado" de alto nivel. Representa un interruptor físico que se desliza lateralmente.

> **Import** — `ferrous_ui_core::ToggleSwitch`

## Características

- **Visualización Clara:** El pomo cambia de posición y el carril cambia de color según el estado.
- **Micro-animación (Planeada):** Diseñado para soportar transiciones suaves en futuras fases.
- **Reactividad:** Soporta `Observable<bool>` para sincronización de estado global.

## Estructura

```rust
pub struct ToggleSwitch<App> {
    pub is_on: bool,
    pub binding: Option<Arc<Observable<bool>>>,
}
```

## Ejemplo de Uso

```rust
use ferrous_ui_core::ToggleSwitch;

// Interruptor simple
let sw = ToggleSwitch::new(true)
    .on_change(|ctx, val| {
        ctx.app.settings.dark_mode = val;
    });

// Con Binding
let wifi_obs = Arc::new(Observable::new(false));
let sw_ui = ToggleSwitch::new(false)
    .with_binding(wifi_obs.clone(), node_id);
```

## Builder API

| Método | Descripción |
|--------|-------------|
| `new(is_on)` | Crea un interruptor con el estado inicial. |
| `on_change(closure)` | Callback que se dispara al alternar el switch. |
| `with_binding(obs, id)`| Vincula el estado a un `Observable<bool>`. |

## Estilo

- **Carril (Track):** Redondeado al 100% (`pill shape`). Usa `theme.primary` (ON) y `theme.surface_variant` (OFF).
- **Pomo (Knob):** Círculo blanco o color de contraste (`theme.on_primary`).
- **Dimensiones:** Fijas de `40x20` px por defecto para mantener consistencia.

## Notas

A diferencia del `Checkbox`, el `ToggleSwitch` no incluye una etiqueta de texto integrada. Se recomienda usarlo junto con un `Label` en un layout de fila (`FlexRow`):

```rust
ui! {
    Panel() {
        Label("Wi-Fi")
        ToggleSwitch(true)
    }
}
```
