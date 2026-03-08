# ferrous_layout

`ferrous_layout` es el cerebro geométrico detrás de la UI de FerrousEngine. Transforma las reglas de diseño abstractas (Style) en coordenadas físicas exactas (Rect) para cada elemento de la pantalla.

---

## Motor Subyacente: Taffy

Este crate utiliza **Taffy** como motor de resolución de restricciones. Taffy es una implementación de alto rendimiento en Rust para los algoritmos de **Flexbox** y **CSS Grid**. 

Gracias a Taffy, `ferrous_layout` puede manejar:
- Anchos y altos porcentuales, fijos o flexibles (`Flex`).
- Alineación dinámica (`Alignment::Center`, `Stretch`, etc.).
- Modos de visualización complejos (`FlexRow`, `FlexColumn`).

---

## Flujo de Trabajo

1.  **Sincronización:** Se recorre el `UiTree` y se crea un espejo de la jerarquía en la estructura `taffy::TaffyTree`.
2.  **Cálculo:** Se ejecuta el algoritmo de layout sobre el grafo de Taffy basándose en el tamaño de la ventana.
3.  **Aplicación:** Los resultados (Coordenadas X, Y y Dimensiones W, H) se escriben de vuelta en el campo `rect` de cada `Node` en el `UiTree`.

---

## Unidades Soportadas

| Unidad | Comportamiento |
|--------|----------------|
| `Px(f32)` | Valor absoluto en píxeles. |
| `Percentage(f32)` | Relativo al contenedor padre (0.0 a 100.0). |
| `Flex(f32)` | Reparte el espacio sobrante proporcionalmente (Similar a `1fr` en CSS). |

---

## Optimizaciones

El cálculo de layout solo ocurre cuando un nodo o uno de sus hijos es marcado con la bandera `layout_dirty`. Si la jerarquía no ha cambiado y los tamaños son estables, el motor de layout no consume ciclos de CPU.
