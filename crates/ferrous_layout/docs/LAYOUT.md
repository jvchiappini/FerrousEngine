# 📐 Ferrous Layout

`ferrous_layout` es el motor de cálculo de posiciones y dimensiones para la UI de FerrousEngine. 

## 🏗️ Motor de Layout
El motor utiliza `Taffy` (una implementación en Rust de Flexbox y CSS Grid) para su resolución interna. 

### 1. Sistema de Sincronización
El `LayoutEngine` actúa como un puente:
1.  Traduce el `UiTree` a un grafo interno de Taffy mediante el método `sync_node`.
2.  Mapea las unidades `Units` de Ferrous (Pixels, Percentage, Flex) a los tipos de Taffy (`Dimension`, `LengthPercentage`, `LengthPercentageAuto`).
3.  Asocia cada `NodeId` de Ferrous con un `NodeId` estable en Taffy mediante `node_map`.

### 2. Resolución de Flexbox
Soporta las unidades de medida:
- `Px(f32)`: Altura/ancho fijos.
- `Percentage(f32)`: Valores relativos al padre (0-100%).
- `Flex(f32)`: Unidades dinámicas para repartir el espacio sobrante (implementado como `flex_grow`).

### 3. Modos de Visualización
Mapeados desde `DisplayMode`:
- `Block`: Comportamiento estándar de bloque (uno encima de otro).
- `FlexRow`: Disposición horizontal con lógica Flexbox.
- `FlexColumn`: Disposición vertical con lógica Flexbox.

### 4. Ciclo de Ejecución
```rust
layout_engine.compute_layout(&mut tree, screen_width, screen_height);
```
Este método:
1. Sincroniza el grafo de Taffy.
2. Ejecuta el motor de cálculo de Taffy (`compute_layout`).
3. Aplica los rectángulos finales (`x`, `y`, `width`, `height`) de vuelta a los nodos del `UiTree` de forma recursiva.
