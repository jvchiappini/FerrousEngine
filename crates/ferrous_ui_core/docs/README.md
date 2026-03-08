# ferrous_ui_core

`ferrous_ui_core` es el motor de datos y lógica fundamental para el nuevo sistema de UI de FerrousEngine. Implementa una arquitectura de **Modo Retenido** (Retained Mode) diseñada para ofrecer el máximo rendimiento ("Lag Cero") mediante la persistencia de widgets y el cacheo agresivo de comandos de renderizado.

---

## Conceptos Clave

| Estructura | Función Principal |
|------------|-------------------|
| `UiTree` | Gestor jerárquico que mantiene todos los nodos de la interfaz y coordina las fases de vida. |
| `Node` | Contenedor que vincula un `Widget` con sus metadatos, estilo, hijos y caché visual. |
| `Widget` | Trait que define el comportamiento del componente (construcción, actualización, dibujo). |
| `DirtyFlags` | Sistema de "banderas sucias" que minimiza el trabajo recalculando solo lo que ha cambiado. |
| `RenderCommand` | Lista abstracta de primitivas visuales (Quad, Text, Image) generadas por los widgets. |

---

## El Ciclo de Vida del Widget

A diferencia de los sistemas de modo inmediato, un `Widget` en `ferrous_ui_core` pasa por fases claras manejadas por el `UiTree`:

1.  **Build (`build`):** Se ejecuta cuando el widget entra al árbol. Es el momento de instanciar sub-widgets (hijos).
2.  **Update (`update`):** Lógica por frame (animaciones, timers). Solo se ejecuta si es necesario.
3.  **Layout (`calculate_size`):** Determina las dimensiones deseadas para que el motor de layout las procese.
4.  **Draw (`draw`):** Genera `RenderCommand`s que se guardan en el caché del `Node`. Solo se vuelve a llamar si el nodo se marca como "sucio de pintura" (`paint`).

---

## Optimización: Lag Cero

El sistema de "Lag Cero" se basa en la propagación de `DirtyFlags`. Si un widget no cambia:
- **No se recalcula su layout.**
- **No se vuelve a ejecutar su lógica de `draw`.**
- **Se reutilizan los comandos de dibujo del frame anterior.**

Esta arquitectura permite que interfaces complejas con miles de elementos se procesen a velocidades de microsegundos, dejando la CPU libre para la lógica del juego o editor.

---

## Ejemplo: Creación de un Nodo

```rust
use ferrous_ui_core::{UiTree, Widget, BuildContext};

struct MyPanel;
impl Widget for MyPanel {
    fn build(&mut self, ctx: &mut BuildContext) {
        // Añadir hijos de forma declarativa
        ctx.add_child(Box::new(MyButton));
    }
}

let mut tree = UiTree::new();
tree.add_node(Box::new(MyPanel), None); // Nodo raíz
tree.build(); // Ejecuta la fase de construcción
```
