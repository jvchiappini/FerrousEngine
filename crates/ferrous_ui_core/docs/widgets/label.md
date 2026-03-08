# Label

El `Label` es el widget más básico para exhibir un String (ya sea texto estático o dinámico) en `ferrous_ui_core`. A diferencia de los motores de modo inmediato, aquí un Label es un nodo retenido del DOM en `UiTree`.

## Construcción y Configuración

El texto de base puede predeterminarse y el color aplicarse encadenando los builders.

```rust
use ferrous_ui_core::Label;

let titulo = Label::new("Ferrous Engine")
    .with_size(18.0)
    .with_color([0.8, 0.8, 0.9, 1.0]);
```

## Reactividad 

`Label` se destaca por ser uno de los widgets que aprovecha a fondo el `Observable<String>`.

```rust
use ferrous_ui_core::{Label, Observable};
use std::sync::Arc;

// Estado de la app
let user_name = Arc::new(Observable::new("Invitado".to_string()));

// En la inicialización de UI:
let label = Label::new("")
    .with_binding(user_name.clone(), node_id);

// En la lógica de red/juego:
let dirty_nodes = user_name.set("Ana".to_string());
tree.reactivity.notify_change(dirty_nodes);
// El UiTree sólo repinta este label específico
```
