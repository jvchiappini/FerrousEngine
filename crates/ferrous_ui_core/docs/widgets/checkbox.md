# Checkbox

`Checkbox` es un widget de selección booleana que permite al usuario alternar entre dos estados (marcado/desmarcado). Incluye una etiqueta de texto integrada para mayor claridad.

> **Import** — `ferrous_ui_core::Checkbox`

## Campos

```rust
pub struct Checkbox<App> {
    pub checked: bool,
    pub label: String,
    pub binding: Option<Arc<Observable<bool>>>,
}
```

- `checked`: Estado actual del checkbox.
- `label`: Etiqueta descriptiva situada a la derecha de la caja.
- `binding`: Vinculación reactiva bidireccional opcional.

## Ejemplo de Uso

```rust
use ferrous_ui_core::Checkbox;

// Checkbox estándar
let c1 = Checkbox::new("Aceptar términos", false)
    .on_change(|ctx, is_checked| {
        println!("Aceptado: {}", is_checked);
    });

// Con Binding Reactivo
let terms_obs = Arc::new(Observable::new(true));
let c2 = Checkbox::new("Recibir noticias", false)
    .with_binding(terms_obs.clone(), node_id);
```

## Builder API

| Método | Descripción |
|--------|-------------|
| `new(label, checked)` | Crea un checkbox con etiqueta y estado inicial. |
| `on_change(closure)` | Callback que se dispara al alternar el estado. |
| `with_binding(obs, id)`| Vincula el estado a un `Observable<bool>`. |

## Estilo Visual

- **Track (Caja):** `theme.surface_variant` (desmarcado) / `theme.primary` (marcado).
- **Check (Marca):** Un cuadro interior en `theme.on_primary` cuando está marcado.
- **Texto:** `theme.on_surface`.
- **Radio:** Se aplica `theme.border_radius * 0.5` para una estética moderna y ligeramente redondeada.

## Interacción

Al hacer clic en cualquier parte del área del widget (incluyendo la etiqueta), el estado se invierte automáticamente (`!current`). Esto es más accesible que limitar el hit-test solo a la pequeña caja cuadrada.
