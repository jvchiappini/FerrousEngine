# Modal / Dialog

`Modal` presenta contenido sobre un **backdrop semitransparente** que bloquea toda interacción con la UI subyacente mientras está abierto. Se usa para confirmaciones, formularios emergentes y mensajes críticos.

> **Import** — `ferrous_ui_core::Modal`

---

## API Constructor

| Método | Descripción |
|--------|-------------|
| `Modal::new()` | Crea un modal cerrado y vacío. |
| `.with_title(str)` | Título que aparece en la barra superior. |
| `.with_content(widget)` | Widget de contenido del diálogo. |
| `.with_width(f32)` | Ancho del panel en píxeles (por defecto `400`). |
| `.with_height(f32)` | Alto del panel en píxeles (por defecto `240`). |
| `.close_on_backdrop(bool)` | Si `true` (por defecto), un clic en el backdrop cierra el modal. |
| `.backdrop_color(Color)` | Color del overlay semitransparente. |

## API de control

| Método | Descripción |
|--------|-------------|
| `modal.open()` | Muestra el modal. |
| `modal.close()` | Oculta el modal. |
| `modal.toggle()` | Alterna visible/oculto. |
| `modal.is_open` | Estado actual. |

---

## Ejemplo de uso

```rust
use ferrous_ui_core::{Modal, Label, Button};

// Definición del modal (normalmente como campo de tu App)
let confirm_modal = Modal::<MyApp>::new()
    .with_title("Confirmar eliminación")
    .with_content(Box::new(Label::new(
        "Esta acción no se puede deshacer. ¿Continuar?"
    )))
    .with_width(380.0)
    .with_height(200.0)
    .close_on_backdrop(true);

let modal_id = tree.add_node(Box::new(confirm_modal), Some(root_id));

// Añadir al árbol ROOT para que el z-order sea correcto.
// El modal debe ser el último hijo del root para pintarse encima.
```

```rust
// Abrir desde un botón:
Button::new("Eliminar archivo").on_click(move |ctx| {
    if let Some(node) = ctx.tree.get_node_mut(modal_id) {
        // Como Modal implementa Widget<App>, accedemos al estado directamente
        // a través del campo is_open, pero necesitamos una forma de mutar.
        // Patrón recomendado: usar un Observable<bool> compartido.
        ctx.app.show_confirm_modal = true;
    }
    ctx.tree.mark_paint_dirty(modal_id);
});
```

### Patrón recomendado con `Observable<bool>`

```rust
// En tu App:
struct MyApp {
    show_modal: Arc<Observable<bool>>,
    modal_id: NodeId,
}

// Botón que abre el modal:
let show_ref = app.show_modal.clone();
Button::new("Eliminar").on_click(move |ctx| {
    show_ref.set(true);
    ctx.tree.mark_paint_dirty(ctx.app.modal_id);
});

// En el update loop de tu UiTree:
let should_open = app.show_modal.get();
if let Some(node) = tree.get_node_mut(app.modal_id) {
    // Acceso directo al campo público
    // ...
}
```

---

## Comportamiento de eventos

Mientras el modal está abierto:

| Evento | Acción |
|--------|--------|
| `MouseDown` en backdrop | Cierra si `close_on_backdrop = true`; siempre consume |
| `MouseDown` en panel | Consume (no pasa a widgets de fondo) |
| `MouseMove` / `MouseWheel` | Consumidos — la UI de fondo no recibe scroll |
| `KeyDown(Escape)` | Cierra el modal |

> [!IMPORTANT]
> Para que el bloqueo de eventos sea efectivo, el Modal debe ser el **último hijo del nodo raíz**. En el sistema de enrutamiento de eventos de `ferrous_ui_core`, los nodos se visitan en orden inverso (último hijo primero), por lo que el modal consumirá los eventos antes de que lleguen a la UI de fondo.

---

## Anatomía visual

```
┌ backdrop (rgba 0,0,0,0.55) ──────────────────────────────────────┐
│                                                                   │
│     ┌ sombra (offset 6,8) ──────────────────────────┐            │
│     ┌───────────────────────────────────────────────┐            │
│     │ ▔▔▔▔▔▔▔▔▔  accent border (2px)  ▔▔▔▔▔▔▔▔▔   │            │
│     │ surface_elevated  Título del Modal      [×]   │            │
│     ├───────────────────────────────────────────────┤            │
│     │                                               │            │
│     │   <widget de contenido>                       │            │
│     │                                               │            │
│     └───────────────────────────────────────────────┘            │
└───────────────────────────────────────────────────────────────────┘
```

- **Backdrop**: `rgba(0,0,0,0.55)` por defecto, cubre el rect del nodo raíz.
- **Sombra**: quad negro al 35%, offset `(6,8)`.
- **Panel**: `theme.surface`, `border-radius` del tema.
- **Acento superior**: barra de 2px `theme.primary`.
- **Barra de título**: `theme.surface_elevated`, altura 40px.
- **Botón [×]**: top-right, cierra al hacer clic.

> [!TIP]
> Para un modal de confirmación completo con dos botones, pasa un widget contenedor
> como contenido que incluya los botones "Confirmar" y "Cancelar" con sus callbacks.
