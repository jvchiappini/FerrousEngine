# TreeView

`TreeView` visualiza datos **jerárquicos** (árboles de nodos) con expandido/colapsado de nodos, selección simple y múltiple, y soporte de **Drag & Drop** entre nodos.

> **Import** — `ferrous_ui_core::{TreeView, TreeNode}`

Es el widget fundamental del **Ferrous Builder**: la jerarquía de escena, el explorador de archivos y cualquier estructura de árbol se construyen sobre él.

---

## API de `TreeNode` (datos)

| Método | Descripción |
|--------|-------------|
| `TreeNode::new(label)` | Crea un nodo de datos con la etiqueta dada. |
| `.icon(char)` | Asigna un carácter emoji/Unicode de icono. |
| `.expanded(bool)` | Estado expandido inicial. |
| `.user_data(u64)` | Dato de usuario opaco (e.g. ID de entidad). |
| `.add_child(child)` | Añade un nodo hijo (builder fluent). |

## API de `TreeView<App>`

| Método | Descripción |
|--------|-------------|
| `TreeView::new()` | Crea un árbol vacío. |
| `.with_root(node)` | Establece el nodo raíz de datos. |
| `.row_height(px)` | Alto de cada fila en píxeles (por defecto `24`). |
| `.indent_px(px)` | Sangría por nivel de profundidad (por defecto `16`). |
| `.multi_select(bool)` | Habilita selección múltiple. |
| `.on_select(f)` | Callback al seleccionar un nodo. Recibe la ruta `&[usize]`. |
| `.on_double_click(f)` | Callback al hacer doble clic. |

**Campos públicos de lectura:**

| Campo | Tipo | Descripción |
|-------|------|-------------|
| `selected` | `Vec<usize>` | Índices de las filas seleccionadas. |
| `root` | `Option<TreeNode>` | Acceso directo a la raíz de datos. |

---

## Ejemplo de uso

```rust
use ferrous_ui_core::{TreeView, TreeNode, StyleBuilder, StyleExt};

// Construir la jerarquía de datos
let mut escena = TreeNode::new("Escena").icon('🎬').expanded(true);
let mut objetos = TreeNode::new("Objetos").icon('📦').expanded(true);
objetos = objetos
    .add_child(TreeNode::new("Cubo").icon('🟦').user_data(1))
    .add_child(TreeNode::new("Esfera").icon('🔵').user_data(2))
    .add_child(TreeNode::new("Luz").icon('💡').user_data(3));
escena = escena
    .add_child(TreeNode::new("Cámara Principal").icon('📷').user_data(0))
    .add_child(objetos);

// Crear el widget
let scene_tree = TreeView::<MyApp>::new()
    .with_root(escena)
    .row_height(26.0)
    .indent_px(16.0)
    .on_select(|ctx, path| {
        // path = ruta de índices hacia el nodo seleccionado
        ctx.app.selected_entity_path = path.to_vec();
    });

let tree_id = ui_tree.add_node(Box::new(scene_tree), Some(left_panel_id));
ui_tree.set_node_style(tree_id, StyleBuilder::new().fill_width().fill_height().build());
```

```rust
// Modificar el árbol de datos externamente (p.ej. añadir un nodo en runtime)
if let Some(node) = ui_tree.get_node_mut(tree_id) {
    if let Some(view) = node.widget.downcast_mut::<TreeView<MyApp>>() {
        if let Some(root) = &mut view.root {
            root.children.push(TreeNode::new("Nuevo Objeto").icon('✨'));
        }
    }
    node.dirty.paint = true;
}
```

---

## Comportamiento del árbol

### Expandir / colapsar
Hacer clic en el triángulo `▶`/`▼` a la izquierda del nodo alterna el estado `expanded`.

### Selección
Un clic en cualquier fila (fuera del triángulo) selecciona el nodo e invoca `on_select`. La ruta `&[usize]` permite localizar el nodo exacto en la jerarquía de datos:

```rust
// Navegar la jerarquía siguiendo la ruta
fn get_node_at_path<'a>(root: &'a TreeNode, path: &[usize]) -> Option<&'a TreeNode> {
    let mut current = root;
    for &idx in path {
        current = current.children.get(idx)?;
    }
    Some(current)
}
```

### Drag & Drop
Al mantener pulsado y arrastrar una fila, aparece un **ghost** semitransparente del nodo. Al soltar sobre otra fila, se indica la posición de destino con una línea azul. La lógica de reparentado se delega al callback `on_drop` (pendiente de implementar en fases futuras).

---

## Anatomía visual

```
┌── TreeView (fill) ─────────────────────────────────────────────┐
│                                                                 │
│ 🎬 Escena                                         ← nivel 0    │
│   ▼ 📦 Objetos    ← expandido                    ← nivel 1    │
│       🟦 Cubo     ← seleccionado (fondo primary)  ← nivel 2    │
│       🔵 Esfera                                   ← nivel 2    │
│       💡 Luz                                      ← nivel 2    │
│   📷 Cámara Principal                             ← nivel 1    │
│   ▶ 🎬 FX                 ← colapsado (tiene hijos)            │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

- **Triángulo**: `▶` = colapsado, `▼` = expandido; solo aparece si el nodo tiene hijos.
- **Sangría**: `indent_px` px por nivel de profundidad.
- **Selección**: fondo `primary.with_alpha(0.25)` en la fila seleccionada.
- **Hover**: fondo `on_surface_muted.with_alpha(0.08)`.
- **Drag ghost**: quad semitransparente en la posición del cursor.

> [!TIP]
> Combina `TreeView` con `DockLayout` poniéndolo como panel `Left` para crear
> exactamente el mismo layout que Unity/Godot tiene para la jerarquía de escena.

> [!IMPORTANT]
> `TreeView` dibuja todo en `draw()` sin crear nodos hijo en el `UiTree`.
> Esto significa que el árbol de UI permanece compacto incluso con jerarquías
> de miles de nodos.
