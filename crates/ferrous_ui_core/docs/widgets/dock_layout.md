# DockLayout

`DockLayout` implementa un sistema de paneles anclables tipo IDE. Divide la ventana en cinco zonas (`Left`, `Right`, `Top`, `Bottom`, `Center`) con divisores arrastrables entre ellas. Es el widget fundamental para **Ferrous Builder**.

> **Import** — `ferrous_ui_core::{DockLayout, DockZone}`

---

## `DockZone`

```rust
pub enum DockZone {
    Left,    // Panel lateral izquierdo — ancho fijo
    Right,   // Panel lateral derecho  — ancho fijo
    Top,     // Banda superior         — alto fijo
    Bottom,  // Banda inferior         — alto fijo
    Center,  // Área central           — ocupa todo el espacio sobrante (flex=1)
}
```

Solo se requiere `Center`; las demás zonas son opcionales.

---

## API Constructor

| Método | Descripción |
|--------|-------------|
| `DockLayout::new()` | Crea un layout vacío. |
| `.dock(zone, size, widget)` | Ancla un widget a la zona indicada. `size` = px (ignorado para `Center`). |

| Método de instancia | Descripción |
|----|---|
| `set_visible(index, bool)` | Muestra u oculta una zona por su índice de registro. |
| `entry_count()` | Número de zonas registradas. |

---

## Ejemplo — Layout tipo Ferrous Builder

```rust
use ferrous_ui_core::{DockLayout, DockZone, StyleBuilder, StyleExt};

let layout = DockLayout::<EditorApp>::new()
    // Jerarquía de escena (izquierda)
    .dock(DockZone::Left, 280.0, Box::new(scene_hierarchy_widget))
    // Inspector de propiedades (derecha)
    .dock(DockZone::Right, 300.0, Box::new(properties_inspector_widget))
    // Consola / assets (abajo)
    .dock(DockZone::Bottom, 200.0, Box::new(console_widget))
    // Viewport 3D (centro — ocupa el resto)
    .dock(DockZone::Center, 0.0, Box::new(
        AspectRatio::<EditorApp>::new(16.0 / 9.0)
            .with_child(Box::new(viewport3d_widget))
    ));

let layout_id = tree.add_node(Box::new(layout), Some(root_id));
tree.set_node_style(layout_id, StyleBuilder::new().fill().build());
```

---

## Arquitectura del árbol generado

```
DockLayout (root — FlexColumn, fill)
├── [Top panel]      — fill_width, height_px(N)
├── [Divisor H]      — fill_width, height_px(4)  ← arrastrable
├── Middle (FlexRow, flex=1)
│   ├── [Left panel]  — width_px(N), fill_height
│   ├── [Divisor V]   — width_px(4), fill_height ← arrastrable
│   ├── [Center]      — flex=1, fill_height       (siempre presente)
│   ├── [Divisor V]   — width_px(4), fill_height ← arrastrable
│   └── [Right panel] — width_px(N), fill_height
├── [Divisor H]      — fill_width, height_px(4)  ← arrastrable
└── [Bottom panel]   — fill_width, height_px(N)
```

Los nodos que no existen (zona no registrada) simplemente no se crean.

---

## Divisores arrastrables

Cada divisor es un nodo `DockDivider` interno con:
- Fondo `theme.on_surface_muted` al 12% (prácticamente invisible).
- Al hover: resaltado con `theme.primary` al 50%.
- Hitbox ampliado ±2px para facilitar la captura.

Al arrastrar un divisor:

| Zona adimensional | Movimiento cursor | Efecto |
|---|---|---|
| `Left` | Derecha (+) | Aumenta el ancho del panel izquierdo |
| `Right` | Izquierda (−) | Aumenta el ancho del panel derecho |
| `Top` | Abajo (+) | Aumenta el alto de la banda superior |
| `Bottom` | Arriba (−) | Aumenta el alto de la banda inferior |

El rango es `[min_size=60px, max_size=2000px]` por defecto.

---

## Mostrar / Ocultar paneles

```rust
// En un callback de botón o menú
Button::new("Vista › Jerarquía").on_click(|ctx| {
    if let Some(node) = ctx.tree.get_node_mut(dock_layout_id) {
        if let Some(dock) = node.widget.downcast_mut::<DockLayout<EditorApp>>() {
            let visible = dock.entries[0].visible; // índice del Left
            dock.set_visible(0, !visible);
        }
    }
    ctx.tree.mark_layout_dirty(dock_layout_id);
});
```

> [!IMPORTANT]
> El `DockLayout` debe ser un nodo de alto nivel (generalmente hijo directo del root)
> para que su `fill_width` + `fill_height` tome el tamaño correcto de la ventana.

> [!TIP]
> Para una barra de menú/toolbar, añade `DockZone::Top` con `height=32`:
> ```rust
> .dock(DockZone::Top, 32.0, Box::new(menu_bar_widget))
> ```
