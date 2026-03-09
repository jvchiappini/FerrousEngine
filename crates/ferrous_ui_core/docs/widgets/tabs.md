# Tabs

`Tabs` es un widget de navegación que organiza contenido en pestañas. Implementa **lazy rendering**: solo el widget de la pestaña activa existe en el árbol de UI, garantizando rendimiento O(1) independientemente del número de pestañas.

> **Import** — `ferrous_ui_core::Tabs`

---

## API Constructor

| Método | Descripción |
|--------|-------------|
| `Tabs::new()` | Crea un tabs vacío (pestaña activa = 0). |
| `.add_tab(title, Box<dyn Widget<App>>)` | Añade una pestaña con su widget de contenido. |
| `.with_active(usize)` | Establece el índice de la pestaña abierta al inicio. |

---

## Ejemplo de Uso

```rust
use ferrous_ui_core::{Tabs, Label, Panel};

let tabs = Tabs::<MyApp>::new()
    .add_tab("General", Box::new(
        Label::new("Configuración general")
    ))
    .add_tab("Avanzado", Box::new(
        Label::new("Opciones de rendimiento")
    ))
    .add_tab("Sobre…", Box::new(
        Label::new("FerrousEngine v0.1")
    ))
    .with_active(0);

let tabs_id = tree.add_node(Box::new(tabs), Some(root_id));
tree.set_node_style(tabs_id, StyleBuilder::new().fill().build());
```

---

## Arquitectura Interna

```
Tabs (root — FlexColumn)
├── Panel "1A1A2E" (header — FlexRow, h=36px)
│   ├── TabButton "General"  [is_active=true]
│   ├── TabButton "Avanzado" [is_active=false]
│   └── TabButton "Sobre…"   [is_active=false]
└── Panel "181825" (content_area — FlexColumn, flex=1)
    └── <widget de la pestaña activa>   ← único hijo, lazy
```

Al cambiar de pestaña:
1. Se extrae el widget activo del árbol (guardado en el campo `panels` del `Tabs`).
2. Se reemplazan los `TabButton` de la cabecera con versiones actualizadas (`is_active`).
3. Se inserta el widget de la nueva pestaña activa.

> [!NOTE]
> Los widgets de pestañas inactivas viven dentro del struct `Tabs` fuera del árbol,
> preservando su estado hasta que vuelvan a ser seleccionados.

---

## Estilo Visual

La barra de cabecera (`#1A1A2E`) contiene botones `TabButton` que muestran:
- **Fondo resaltado** con `theme.primary` al 25% si están activos.
- **Línea inferior** de 2px en `theme.primary` para el tab activo.
- **Texto** en `theme.on_surface` (activo) o `theme.on_surface_muted` (inactivo).

El área de contenido (`#181825`) tiene `padding: 8px` y ocupa el espacio restante vía `flex(1)`.

> [!TIP]
> Para controlar el estilo de las pestañas usa el sistema de temas:
> `tree.theme.primary = Color::hex("#your-accent-color");`
