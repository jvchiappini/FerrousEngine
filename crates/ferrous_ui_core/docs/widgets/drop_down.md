# DropDown

`DropDown` (o ComboBox) es un selector que permite al usuario escoger una opción de una lista desplegable. Soporta estados de apertura/cierre, gestión de selección y sincronización reactiva.

> **Import** — `ferrous_ui_core::DropDown`

## Estructura

El widget almacena la lista de opciones y el índice del elemento actualmente seleccionado. Como todos los widgets del núcleo, es genérico sobre el estado de la aplicación `App`.

```rust
pub struct DropDown<App> {
    pub options: Vec<String>,
    pub selected_index: usize,
    pub is_open: bool,
    pub binding: Option<Arc<Observable<usize>>>,
    // on_change_cb: Option<Box<dyn Fn(&mut EventContext<App>, usize)>>
}
```

- `options` — Lista de cadenas de texto con las opciones disponibles.
- `selected_index` — Índice de la opción activa.
- `is_open` — Controla si la lista desplegable es visible.
- `binding` — Vinculación opcional a un valor reactivo entero.

## Construcción y Uso

```rust
use ferrous_ui_core::DropDown;

let selector = DropDown::new(vec!["Bajo", "Medio", "Alto", "Ultra"])
    .on_change(|ctx, index| {
        println!("Calidad cambiada a índice: {}", index);
    });

// Con binding reactivo
let selector = DropDown::new(opciones)
    .with_binding(ctx.app.settings.quality_obs.clone(), node_id);
```

## Builder API

| Método | Descripción |
|--------|-------------|
| `new(options)` | Crea el selector con un vector de cadenas. |
| `on_change(closure)` | Callback que se dispara al seleccionar una nueva opción. |
| `with_binding(obs, id)` | Vincula la selección a un `Observable<usize>`. |

## Comportamiento Visual

El `DropDown` consta de dos partes principales:

1. **Trigger (Botón base):** Siempre visible. Muestra la opción seleccionada y un icono indicador (▼). Usa `theme.surface_elevated`.
2. **Lista (Popup):** Solo visible cuando `is_open` es true. Se despliega debajo del trigger.
   - Cada item de la lista tiene un highlight visual (usando `theme.primary` con transparencia) cuando coincide con la selección actual.
   - El fondo de la lista usa `theme.surface`.

## Interacción

- Al hacer clic en el **Trigger**, se alterna el estado de `is_open`.
- Al hacer clic en una **Opción** de la lista, se actualiza la selección, se cierra el menú y se dispara el callback `on_change` (así como la notificación reactiva si existe binding).
- Si el menú está abierto y se hace clic fuera de las opciones, el menú se cierra automáticamente.

## Notas de Implementación

- **Z-Order:** Actualmente, el dibujo se realiza de forma secuencial. En futuras versiones del motor, el popup del DropDown se elevará a una capa de overlay para asegurar que no quede oculto por otros widgets hermanos (Fase 6.3).
- Las dimensiones basales del selector son de 150x36 píxeles por defecto.
