# Checkbox

`Checkbox` es un widget interactivo de tipo toggle (booleano) con soporte de reactividad y callbacks, diseñado para integrarse con la arquitectura de modo retenido.

## Fields

```rust
pub struct Checkbox {
    pub checked: bool,
    pub label: String,
    pub color_unchecked: [f32; 4],
    pub color_checked: [f32; 4],
    pub check_color: [f32; 4],
    pub is_hovered: bool,
    pub binding: Option<std::sync::Arc<Observable<bool>>>,
}
```

## Construction

```rust
use ferrous_ui_core::Checkbox;

let cb = Checkbox::new("Aceptar términos", false)
    .on_change(|checked| println!("Términos: {}", checked));
```

## Reactivity

```rust
use ferrous_ui_core::{Checkbox, Observable};
use std::sync::Arc;

let is_active = Arc::new(Observable::new(true));
let cb = Checkbox::new("Activar VSync", true)
    .with_binding(is_active.clone(), node_id);
```
