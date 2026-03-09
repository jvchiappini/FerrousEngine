# TextInput

`TextInput` es un campo de texto editable de una sola línea con soporte enriquecido para interacciones de teclado, cursor dinámico y vinculación de datos reactivos.

> **Import** — `ferrous_ui_core::TextInput`

## Estructura

En el sistema retenido, `TextInput` es genérico sobre el estado de la aplicación `App` y permite manejar suscripciones a cambios de texto de forma automática.

```rust
pub struct TextInput<App> {
    pub text: String,
    pub placeholder: String,
    pub cursor_pos: usize,
    pub is_focused: bool,
    pub binding: Option<Arc<Observable<String>>>,
    // on_submit_cb: Option<Box<dyn Fn(&mut EventContext<App>, &str)>>
}
```

- `text` — El contenido actual del campo (se ignora si hay un `binding` activo).
- `placeholder` — Texto mostrado en gris cuando el campo está vacío.
- `cursor_pos` — Posición del cursor de inserción de texto.
- `is_focused` — Estado de enfoque (activado al hacer clic, desactivado con Enter o clic fuera).

## Construcción y Uso

`TextInput` utiliza una API fluent para configurar su comportamiento inicial y callbacks.

```rust
use ferrous_ui_core::TextInput;

// Simple con placeholder
let input = TextInput::<AppState>::new("Nombre del proyecto...");

// Con callback de envío (Enter)
let input = TextInput::new("Buscar...")
    .on_submit(|ctx, text| {
        println!("Buscando: {}", text);
        ctx.app.perform_search(text);
    });

// Vinculado a un valor reactivo de la aplicación
let input = TextInput::new("Configuración")
    .with_binding(ctx.app.input_value.clone(), node_id);
```

## Builder API

| Método | Descripción |
|--------|-------------|
| `new(placeholder)` | Crea una instancia con el texto de sugerencia inicial. |
| `on_submit(closure)` | Callback que se ejecuta cuando el usuario pulsa **Enter**. |
| `with_binding(obs, id)` | Vincula el widget a un `Observable<String>` para sincronización bidireccional. |

## Comportamiento Visual

El widget adapta su apariencia al `Theme` activo:

- **Fondo:** `theme.surface` (normal) o `theme.surface_variant` (cuando tiene el foco).
- **Indicador de Foco:** Una línea en la base del widget usando `theme.primary` cuando está activo.
- **Texto:** `theme.on_surface`.
- **Placeholder:** `theme.on_surface_muted`.
- **Cursor:** Una barra vertical de 2px con el color `theme.primary`.

## Interacción de Teclado

| Tecla | Acción |
|-------|--------|
| **Cualquier Carácter** | Inserta el carácter en la posición del cursor. |
| **Backspace** | Elimina el carácter anterior al cursor. |
| **Flecha Izquierda** | Desplaza el cursor hacia la izquierda. |
| **Flecha Derecha** | Desplaza el cursor hacia la derecha. |
| **Enter** | Ejecuta `on_submit` y libera el foco. |
| **Esc** | Libera el foco sin ejecutar el submit. |

## Notas de Implementación

- El ancho del widget está predefinido en 200px por defecto, pero puede ser sobrescrito mediante el `StyleBuilder`.
- El cursor se posiciona dinámicamente asumiendo una fuente monoespaciada para el cálculo (aproximación actual de la Fase 6.1).
- Cuando está vinculado a un `Observable`, el widget notifica automáticamente al sistema de reactividad en cada pulsación de tecla para que otros componentes suscritos puedan reaccionar al cambio en tiempo real.
