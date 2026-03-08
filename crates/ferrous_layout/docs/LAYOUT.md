# 📐 Ferrous Layout — Referencia Técnica

`ferrous_layout` es el motor de cálculo de posiciones y dimensiones para la UI de FerrousEngine.

---

## 🏗️ Motor de Layout

El motor utiliza `Taffy` (una implementación en Rust de Flexbox y CSS Grid) para su resolución interna.

### 1. Sistema de Sincronización

El `LayoutEngine` actúa como un puente entre los dos mundos:

```
UiTree::NodeId  ─── node_map ──▶  taffy::NodeId
```

- Genera un espejo de la jerarquía en `TaffyTree` vía `sync_node`.
- Si el nodo ya existe en Taffy (mismo `NodeId`), **actualiza** su estilo y sus hijos en lugar de recrearlo.
- Asocia el `NodeId` de Ferrous al nodo de Taffy como **contexto** para que la función de medida pueda invocar `Widget::calculate_size`.

### 2. Conversión de Estilos (`convert_style`)

| `ferrous_ui_core::Style` | `taffy::Style` |
|--------------------------|----------------|
| `DisplayMode::Block` | `Display::Block` |
| `DisplayMode::FlexRow` | `Display::Flex` + `FlexDirection::Row` |
| `DisplayMode::FlexColumn` | `Display::Flex` + `FlexDirection::Column` |
| `Position::Relative` | `Position::Relative` |
| `Position::Absolute` | `Position::Absolute` |
| `Alignment::Center` | `AlignItems::Center` + `JustifyContent::Center` |
| `Alignment::Stretch` | `AlignItems::Stretch` |
| `Units::Px(v)` | `Dimension::Length(v)` |
| `Units::Percentage(v)` | `Dimension::Percent(v / 100.0)` |
| `Units::Flex(v)` | `flex_grow = v` + `Dimension::Auto` |
| `Units::Auto` | `Dimension::Auto` |

### 3. Función de Medida Personalizada

Para que widgets como `Label` puedan influir en su tamaño (tamaño intrínseco basado en el contenido), `compute_layout` usa `compute_layout_with_measure`. Cuando Taffy necesita saber el tamaño de un nodo hoja, llama a `Widget::calculate_size`:

```rust
// El motor invoca esto internamente por cada nodo hoja
let size = node.widget.calculate_size(&mut LayoutContext {
    available_space,
    known_dimensions,
    node_id,
});
```

### 4. Ciclo de Ejecución

```rust
layout_engine.compute_layout(&mut tree, screen_width, screen_height);
```

Este método internamente:
1. Llama a `sync_node` (recursivo) → sincroniza el grafo de Taffy.
2. Llama a `TaffyTree::compute_layout_with_measure` → resuelve todas las posiciones.
3. Llama a `apply_layout` (recursivo) → escribe los `Rect` finales de vuelta en el `UiTree`.

### 5. Aplicación de Resultados (`apply_layout`)

Las posiciones calculadas por Taffy son **relativas al padre**. `apply_layout` las convierte a **coordenadas absolutas de pantalla** acumulando los offsets recursivamente:

```
root        → x = 0, y = 0   (origen de pantalla)
  panel_a   → x = 0+10 = 10, y = 0+50 = 50
    button  → x = 10+8 = 18, y = 50+8 = 58
```

---

## 🚀 Optimizaciones

- **Dirty-flag aware**: El cálculo de layout solo ocurre si algún nodo tiene `layout_dirty = true`. Si la jerarquía no ha cambiado, el motor de layout no consume ciclos de CPU.
- **Caching en Taffy**: Taffy implementa su propio sistema de caché interno; los nodos cuyo estilo y hijos no han cambiado no se recalculan.
- **Reuso de NodeIds**: Los nodos de Taffy se reutilizan entre frames mediante el `node_map`, evitando allocations innecesarias.
