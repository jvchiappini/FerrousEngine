# Slider

`Slider` es un widget retenido interactivo utilizado para seleccionar un número de un rango contiguo establecido. Se construye bajo los principios del Modo Retenido con reactividad incorporada.

## Construcción Básica

Se requiere un rango y un valor inicial para la construcción de `Slider`:

```rust
use ferrous_ui_core::Slider;

let slider = Slider::new(0.5, 0.0, 1.0)
    .on_change(|v| println!("Nuevo volumen: {:.2}", v));
```

## Data Binding Reactivo

El uso más poderoso del `Slider` en el sistema de UI actual es vincularlo (`binding`) a un `Observable<f32>`. Esta conexión automática asegura que la interfaz se actualice cuando los datos subyacentes cambien *sin* regenerar el frame.

```rust
use ferrous_ui_core::{Slider, Observable};
use std::sync::Arc;

let volume = Arc::new(Observable::new(50.0));

let volume_slider = Slider::new(50.0, 0.0, 100.0)
    .with_binding(volume.clone(), node_id); 
```

*Nota:* `node_id` corresponde al `ID` del nodo asignado por el `UiTree`.
