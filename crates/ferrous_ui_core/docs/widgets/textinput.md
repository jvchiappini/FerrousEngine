# TextInput

`TextInput` es un widget retenido complejo para la captura de texto en la interfaz gráfica. Soporta edición de una sola línea, posicionamiento del cursor mediante interacciones de teclado, placeholders e integración reactiva.

## Construcción y Configuración

```rust
use ferrous_ui_core::TextInput;

let input = TextInput::new("Escribe aquí...")
    .with_text("Ferrous")
    .on_change(|nuevo_texto| {
        println!("Texto actualizado: {}", nuevo_texto);
    });
```

El widget gestiona su propio estado de foco, respondiendo a interacciones del teclado (`KeyDown`, `Backspace`, `flechas`, etc.) únicamente cuando el usuario le ha hecho clic previamente.

## Integración Reactiva (Lag Cero)

El `TextInput` soporta el enlazamiento de datos bidireccional usando `Observable<String>`.

```rust
use ferrous_ui_core::{TextInput, Observable};
use std::sync::Arc;

let texto_estado = Arc::new(Observable::new(String::new()));
let txt = TextInput::new("Placeholder")
    .with_binding(texto_estado.clone(), node_id);
```

Cada cambio reflejado en la pantalla notifica al sistema a través de `tree.reactivity.notify_change(...)`.
