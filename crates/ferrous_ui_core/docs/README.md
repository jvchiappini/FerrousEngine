# ferrous_ui_core

`ferrous_ui_core` es el motor de datos y lógica fundamental para el sistema de UI de FerrousEngine. Implementa una arquitectura de **Modo Retenido** (Retained Mode) diseñada para ofrecer el máximo rendimiento ("Lag Cero") mediante la persistencia de widgets y el cacheo agresivo de comandos de renderizado.

---

## Module overview

| Módulo | Exportaciones clave | Descripción |
|--------|---------------------|-------------|
| `lib` | `UiTree`, `Node`, `NodeId`, `Rect`, `Style`, `RenderCommand`, `DirtyFlags` | Núcleo del árbol de UI y sus tipos fundamentales |
| `widgets` | `Panel`, `Label`, `Button`, `Slider`, `PlaceholderWidget` | Widgets nativos del sistema retenido |
| `events` | `UiEvent`, `EventResponse` | Tipos de eventos y respuestas de la UI |
| `reactive` | `Observable<T>`, `ReactivitySystem` | Sistema de data binding reactivo |
| `style_builder` | `StyleBuilder`, `StyleExt` | API fluent para construir estilos de layout |
| `theme` | `Theme`, `Color` | Sistema de temas globales y paleta semántica |

---

## Conceptos Clave

| Estructura | Función Principal |
|------------|-------------------|
| `UiTree` | Gestor jerárquico que mantiene todos los nodos de la interfaz y coordina las fases de vida. |
| `Node` | Contenedor que vincula un `Widget` con sus metadatos, estilo, hijos y caché visual. |
| `Widget` | Trait que define el comportamiento del componente: construcción, actualización, dibujo y eventos. |
| `DirtyFlags` | Sistema de "banderas sucias" que minimiza el trabajo recalculando solo lo que ha cambiado. |
| `RenderCommand` | Enum de primitivas visuales abstractas (`Quad`, `Text`, `Image`, `PushClip`, `PopClip`). |
| `Rect` | Rectángulo con utilidades integradas: `intersect`, `intersects`, `contains`. |
| `Observable<T>` | Valor reactivo que notifica a sus observadores cuando cambia. |
| `StyleBuilder` | Constructor fluent para `Style`: encadena modificadores legibles en lugar de struct literals. |
| `Theme` | Paleta semántica de colores y valores visuales compartidos por todos los widgets. |
| `Color` | Color RGBA normalizado con helpers `hex()`, `lerp()`, `with_alpha()` y constantes. |

---

## El Ciclo de Vida del Widget

A diferencia de los sistemas de modo inmediato, un `Widget` en `ferrous_ui_core` pasa por fases claras manejadas por el `UiTree`:

1. **Build (`build`):** Se ejecuta cuando el widget entra al árbol. Es el momento de instanciar sub-widgets (hijos).
2. **Update (`update`):** Lógica por frame (animaciones, timers). Solo se ejecuta si es necesario.
3. **Layout (`calculate_size`):** Determina las dimensiones deseadas para que el motor de layout las procese.
4. **Draw (`draw`):** Genera `RenderCommand`s que se guardan en el caché del `Node`. Solo se vuelve a llamar si el nodo se marca como "sucio de pintura" (`paint`).

---

## Optimización: Lag Cero

El sistema de "Lag Cero" se basa en la propagación de `DirtyFlags`. Si un widget no cambia:
- **No se recalcula su layout.**
- **No se vuelve a ejecutar su lógica de `draw`.**
- **Se reutilizan los comandos de dibujo del frame anterior.**
- **El Culling de Viewport omite nodos fuera de pantalla antes de procesarlos.**

Esta arquitectura permite que interfaces complejas con miles de elementos se procesen en microsegundos, dejando la CPU libre para la lógica del juego o editor.

---

## Culling Automático

`collect_commands` acepta un `Rect` de viewport. Cualquier nodo cuyo `rect` no intersecte con él es descartado junto a todo su subárbol, sin generar ninguna primitiva para la GPU:

```rust
// Recolectar solo los nodos visibles en pantalla
let viewport = Rect::new(0.0, 0.0, 1920.0, 1080.0);
tree.collect_commands(&mut cmds, viewport);
```

---

## Ejemplo: Creación de un Árbol

```rust
use ferrous_ui_core::{UiTree, Widget, BuildContext, Panel, Label};

struct MyPanel;
impl Widget for MyPanel {
    fn build(&mut self, ctx: &mut BuildContext) {
        ctx.add_child(Box::new(Label::new("Hola, Ferrous!")));
    }
}

let mut tree = UiTree::new();
tree.add_node(Box::new(MyPanel), None); // Nodo raíz
tree.build(); // Ejecuta la fase de construcción recursiva
```

---

## Callbacks en Widgets (Fase 5.2)

`Button` y `Slider` ahora soportan closures directamente en su builder, sin necesidad de implementar `Widget::on_event` manualmente:

```rust
use ferrous_ui_core::Button;

let btn = Button::new("Eliminar")
    .on_click(|| {
        println!("Elemento eliminado");
    })
    .on_hover(|| println!("hover activado"))
    .with_radius(6.0);
```

```rust
use ferrous_ui_core::Slider;

let slider = Slider::new(0.5, 0.0, 1.0)
    .on_change(|v| println!("Nuevo volumen: {:.2}", v));
```

---

## StyleBuilder — API Fluent (Fase 5.4)

`StyleBuilder` reemplaza el verbose `Style { ... }` struct literal por métodos encadenables:

```rust
use ferrous_ui_core::StyleBuilder;

let style = StyleBuilder::new()
    .fill_width()
    .height_px(48.0)
    .padding_all(8.0)
    .row()
    .center_items()
    .build();

tree.set_node_style(node_id, style);
```

Métodos disponibles: `.width_px()`, `.height_pct()`, `.fill_width()`, `.fill_height()`, `.fill()`, `.flex()`, `.padding_all()`, `.padding_xy()`, `.margin_all()`, `.row()`, `.column()`, `.block()`, `.center_items()`, `.absolute()`, `.top()`, `.left()`, `.right()`, `.bottom()`.

---

## Sistema de Temas (Fase 5.5)

`Theme` centraliza todos los colores de la aplicación. Elimina los `[f32; 4]` hardcodeados en los widgets:

```rust
use ferrous_ui_core::{Theme, Color};

let theme = Theme::dark()
    .with_primary(Color::hex("#6C63FF"))
    .with_surface(Color::hex("#1E1E2E"))
    .with_on_surface(Color::hex("#CDD6F4"))
    .with_border_radius(8.0)
    .with_base_font_size(14.0);

// Acceder a colores semánticos:
let bg_color = theme.surface.to_array();
let text_color = theme.on_surface.to_array();
```

`Color` soporta:
- Construcción desde hex: `Color::hex("#RRGGBB")` / `Color::hex("#RRGGBBAA")`
- Conversión: `.to_array() -> [f32; 4]`
- Interpolación: `.lerp(other, t)`
- Transparencia: `.with_alpha(a)`
- Constantes: `Color::BLACK`, `Color::WHITE`, `Color::TRANSPARENT`

---

## Data Binding Reactivo

El sistema de `Observable<T>` permite que los valores de la aplicación conduzcan la UI automáticamente sin polling manual:

```rust
use ferrous_ui_core::{Observable, Label};
use std::sync::Arc;

// Observable compartido entre la lógica de la app y la UI
let fps_counter: Arc<Observable<String>> = Arc::new(Observable::new("60 FPS".into()));

// El Label se actualiza solo cuando fps_counter cambia
let label = Label::new("").with_binding(fps_counter.clone(), node_id);

// En la lógica del juego:
let dirty_nodes = fps_counter.set("120 FPS".into());
tree.reactivity.notify_change(dirty_nodes); // Solo el Label se repinta
```

---

## Widgets Disponibles

| Widget | Descripción |
|--------|-------------|
| [`Panel`](widgets/panel.md) | Contenedor visual con color de fondo y radios de esquina configurables. |
| [`Label`](widgets/label.md) | Texto estático o reactivo vinculado a un `Observable<String>`. |
| [`Button`](widgets/button.md) | Botón con estados hover/press, callbacks `on_click`/`on_hover` y builder fluent. |
| [`TextInput`](widgets/textinput.md) | Campo editable de una sola línea con soporte de teclado, cursores y enlazamiento de texto. |
| [`Slider`](widgets/slider.md) | Control de arrastre para `f32` con `on_change` y soporte de `Observable<f32>`. |
| [`Checkbox`](widgets/checkbox.md) | Toggle interactivo booleano con soporte reactivo. |
| `ToggleSwitch` | Switch alternativo a checkbox, ideal para interfaces mobile. |
| `ProgressBar` | Indicador visual de progreso de un proceso (0.0 a 1.0). |
| `Separator` | Línea divisoria configurable para layouts estables. |
| `Spacer` | Widget elástico layout que no pinta información visual. |
| `PlaceholderWidget` | Nodo vacío para uso estructural o provisional. |

---

## Further reading

- [Tipos y estructuras del núcleo](CORE.md)
- [Motor de layout — ferrous_layout](../../ferrous_layout/docs/README.md)
- [Backend de renderizado — ferrous_ui_render](../../ferrous_ui_render/docs/README.md)
- [Sistema de eventos — ferrous_events](../../ferrous_events/docs/README.md)
