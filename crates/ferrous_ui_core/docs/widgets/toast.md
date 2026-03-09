# Toast / Snackbar

`ToastManager` es el widget que gestiona notificaciones efímeras (toasts). Muestra mensajes breves en una esquina de la pantalla con animaciones de entrada y salida suaves, auto-cierre configurable y cuatro niveles semánticos.

> **Imports** — `ferrous_ui_core::{ToastManager, Toast, ToastLevel}`

---

## Arquitectura de dos piezas

| Tipo | Rol |
|------|-----|
| `Toast` | Descriptor inmutable de una notificación (texto, nivel, duración). |
| `ToastManager<App>` | Widget del árbol que gestiona la cola, las animaciones y el render. |

El `ToastManager` se coloca **una sola vez** como último hijo del nodo raíz. Los toasts se añaden empujando a `manager.queue` desde cualquier callback.

---

## Niveles semánticos

| `ToastLevel` | Acento | Icono | Uso |
|---|---|---|---|
| `Info` | Azul-violeta `#6B64FF` | ℹ | Mensajes informativos generales |
| `Success` | Verde `#3DCC70` | ✓ | Operación completada |
| `Warning` | Naranja `#FFB326` | ⚠ | Situación que requiere atención |
| `Error` | Rojo `#F2455C` | ✕ | Error o fallo crítico |

---

## API

### `Toast`

| Constructor | Descripción |
|-------------|-------------|
| `Toast::info(msg)` | Crea un toast informativo. |
| `Toast::success(msg)` | Crea un toast de éxito. |
| `Toast::warning(msg)` | Crea un toast de advertencia. |
| `Toast::error(msg)` | Crea un toast de error. |
| `.duration(secs)` | Duración visible total (por defecto `3.0`s). |

### `ToastManager<App>`

| Método / Campo | Descripción |
|---|---|
| `ToastManager::new()` | Crea el manager anclado en esquina inferior derecha. |
| `.anchor_left()` | Cambia el ancla a esquina inferior izquierda. |
| `manager.push(toast)` | Añade un toast a la cola. |
| `manager.queue` | `Vec<Toast>` con los toasts activos (pública). |

---

## Ejemplo de uso

```rust
use ferrous_ui_core::{ToastManager, Toast};

// 1. Registrar el manager al construir la UI
let toast_mgr_id = tree.add_node(
    Box::new(ToastManager::<MyApp>::new()),
    Some(root_id), // siempre último hijo
);
tree.set_node_style(toast_mgr_id, StyleBuilder::new().absolute()
    .top(0.0).left(0.0).fill().build());

// Guardar el ID en el estado de la App para acceder desde callbacks
app.toast_manager_id = toast_mgr_id;
```

```rust
// 2. Emitir un toast desde cualquier callback
Button::new("Guardar").on_click(|ctx| {
    ctx.app.do_save();

    // Acceder al manager usando su NodeId y empujar directamente
    if let Some(node) = ctx.tree.get_node_mut(ctx.app.toast_manager_id) {
        if let Some(mgr) = node.widget.downcast_mut::<ToastManager<MyApp>>() {
            mgr.push(Toast::success("Proyecto guardado correctamente"));
        }
    }
    ctx.tree.mark_paint_dirty(ctx.app.toast_manager_id);
});
```

```rust
// Toasts de diferentes niveles
mgr.push(Toast::info("Autoguardado activo"));
mgr.push(Toast::warning("Memoria por encima del 80%"));
mgr.push(Toast::error("No se pudo conectar al servidor").duration(6.0));
mgr.push(Toast::success("Compilación exitosa"));
```

---

## Ciclo de vida y animación

Cada toast tiene una duración total = `enter_secs + visible_secs + exit_secs`:

```
t=0                 t=enter_secs      t=total-exit_secs    t=total
 │── slide-in ──────│── visible ───── │── slide-out + fade ──│
 │  ease-out cubic  │  alpha=1.0      │  ease-out cubic      │ → eliminado
```

| Fase | `offset_y` | `alpha` |
|------|-----------|---------|
| Entrada | `height → 0` (slide desde abajo) | `0 → 1` |
| Visible | `0` | `1.0` |
| Salida | `0 → height` | `1 → 0` |

La curva usada es **ease-out cúbica** (`1 - (1-t)³`), que produce movimiento natural y rápido al inicio, suave al frenar.

---

## Anatomía visual

```
┌ sombra ──────────────────────────────┐
┌──┬────────────────────────────────────┐ ← TOAST_H = 52px
│  │  ✓  Proyecto guardado              │
│  │                                    │
│  ▔▔▔▔▔▔▔▔▔███████████────────────────│ ← barra de progreso (2px)
└──┴────────────────────────────────────┘
▲
└── acento lateral 4px (color del nivel)
```

- **Tamaño**: `320 × 52 px`
- **Margen desde la esquina**: `16px`
- **Separación entre toasts**: `8px`
- **Apilado**: el toast más reciente aparece abajo; los anteriores se desplazan arriba.
- **Barra de progreso**: refleja el tiempo restante antes de desaparecer.

> [!TIP]
> Para un comportamiento tipo "solo un toast a la vez" (como en Android),
> llama a `manager.queue.clear()` antes de hacer `push()`.
