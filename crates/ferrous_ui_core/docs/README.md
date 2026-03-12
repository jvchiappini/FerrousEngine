# ferrous_ui_core

Núcleo de datos y lógica del sistema de UI de Ferrous Engine.

Implementa una arquitectura de **Modo Retenido** (Retained Mode): los widgets
persisten en un árbol de memoria (`UiTree`) entre frames. El sistema recalcula
sólo lo que ha cambiado, lo que permite cachear comandos de dibujo y omitir
ramas intactas del árbol en cada ciclo (estrategia "Lag Cero").

---

## Módulos

### `lib.rs` — Tipos primitivos y árbol principal

#### Tipos de datos fundamentales

| Tipo | Descripción |
|---|---|
| `Rect` | Rectángulo `{x, y, width, height}` con helpers `contains`, `intersect`, `intersects` |
| `RectOffset` | Offsets para los cuatro lados (margin, padding) |
| `Units` | `Px(f32)` · `Percentage(f32)` · `Flex(f32)` · `Auto` |
| `Style` | Propiedades de layout: `size`, `margin`, `padding`, `display`, `position`, `offsets`, `overflow`, `alignment` |
| `DisplayMode` | `Block` · `FlexRow` · `FlexColumn` |
| `Position` | `Relative` · `Absolute` |
| `Overflow` | `Visible` · `Hidden` · `Scroll` |
| `Alignment` | `Start` · `Center` · `End` · `Stretch` |
| `RenderCommand` | Primitiva de dibujo: `Quad`, `Text`, `Image`, `PushClip`, `PopClip` |
| `DirtyFlags` | Bits `layout`, `paint`, `hierarchy`, `subtree_dirty` para el sistema de invalidación |
| `NodeId` | Clave de `SlotMap` — identificador estable de un nodo |
| `CmdQueue` | Cola de comandos diferidos (acciones post-frame) |

#### `Node<App>`

Unidad mínima del árbol. Contiene:
- `widget: Box<dyn Widget<App>>` — el widget en sí
- `parent / children` — jerarquía
- `style: Style` — propiedades de layout
- `dirty: DirtyFlags` — qué aspectos necesitan recalcularse
- `rect: Rect` — rectángulo resuelto por el motor de layout
- `cached_cmds: Vec<RenderCommand>` — caché de comandos del último frame

#### `UiTree<App>`

Gestor del árbol usando un `SlotMap<NodeId, Node>` para acceso O(1) y
estabilidad de IDs incluso tras inserciones/eliminaciones.

Métodos principales:

| Método | Descripción |
|---|---|
| `add_node(widget, parent)` | Inserta un nodo y lo enlaza a su padre |
| `add_node_with_id(widget, parent, id_str)` | Ídem con ID de texto para búsqueda |
| `set_node_style(id, style)` | Aplica un `Style` e invalida el layout |
| `set_node_rect(id, rect)` | Escribe el rect resuelto (resultado del layout engine) |
| `mark_layout_dirty(id)` | Invalida layout y propaga hacia arriba |
| `mark_paint_dirty(id)` | Invalida repintado del nodo |
| `collect_commands(cmds, viewport)` | Recorre el árbol y recolecta `RenderCommand`s, usando caché si el nodo está limpio y aplicando culling por viewport |
| `get_node_by_id(id_str)` | Busca un nodo por su ID de texto |
| `build()` | Fase de construcción recursiva desde la raíz |
| `update(dt)` | Aplica reactividad pendiente y actualiza la lógica de cada widget |

#### `Widget<App>` trait

El contrato que todo widget debe cumplir:

| Método | Cuándo se llama | Descripción |
|---|---|---|
| `build(&mut BuildContext)` | Al insertar el widget | Para añadir hijos iniciales |
| `update(&mut UpdateContext)` | Cada frame | Animaciones, timers, lógica |
| `calculate_size(&mut LayoutContext) -> Vec2` | Durante el layout | Tamaño preferido |
| `draw(&mut DrawContext, &mut Vec<RenderCommand>)` | Cuando el nodo está sucio | Genera primitivas visuales |
| `on_event(&mut EventContext, &UiEvent) -> EventResponse` | Ante un evento | Lógica de interacción |
| `reflect() / reflect_mut()` | Editor (GUIMaker) | Acceso al sistema de reflexión |

Todos los métodos tienen implementación por defecto — solo se sobrescriben los necesarios.

#### Contextos de fase

| Contexto | Acceso |
|---|---|
| `BuildContext<App>` | `&mut UiTree`, `NodeId`, `Theme` |
| `UpdateContext` | `delta_time`, `NodeId`, `Rect`, `Theme` |
| `LayoutContext` | `available_space`, `known_dimensions`, `NodeId`, `Theme` |
| `DrawContext` | `NodeId`, `Rect` resuelto, `Theme` |
| `EventContext<App>` | `NodeId`, `Rect`, `Theme`, `&mut UiTree`, `&mut App` |

#### `Component<App>` trait

Permite crear componentes reutilizables (grupos de widgets) similares a
funciones `@Composable` de Compose o componentes funcionales de React.

---

### `theme.rs` — Sistema de temas

`Color` RGBA normalizado con constructores `hex("#RRGGBB")`, `from_rgba8`, `lerp`, `with_alpha`.
Constantes predefinidas: `BLACK`, `WHITE`, `TRANSPARENT`, `FERROUS_ACCENT`.

`Theme` define roles semánticos accesibles desde todos los widgets via `DrawContext`:

| Rol | Descripción |
|---|---|
| `primary / primary_variant` | Color de acento (botones, activos) |
| `on_primary` | Texto sobre el color primario |
| `background / surface / surface_elevated` | Fondos de app, paneles y popups |
| `on_surface / on_surface_muted` | Texto principal y secundario |
| `error / success / warning` | Colores de feedback |
| `border_radius` | Radio global de esquinas |
| `font_size_base / small / heading` | Escala tipográfica |

Temas incluidos: `Theme::dark()` (Catppuccin Mocha) · `Theme::light()`.
Builder fluent: `.with_primary(c)`, `.with_surface(c)`, `.with_border_radius(r)`, etc.

---

### `reactive.rs` — Sistema reactivo

`Observable<T>` — valor observable thread-safe (`Arc<Mutex<T>>`):

| Método | Descripción |
|---|---|
| `new(value)` | Crea el observable |
| `get()` | Lee el valor actual |
| `set(new_val)` | Actualiza y devuelve la lista de `NodeId` suscritos para invalidar |
| `subscribe(node_id)` | Registra un nodo para recibir notificaciones |

`ReactivitySystem` — cola de nodos pendientes de invalidación. El `UiTree`
lo llama en `update()` para trasladar las notificaciones a `mark_paint_dirty`.

---

### `reflect.rs` — Sistema de reflexión para el Editor

Permite que los widgets sean inspeccionables y editables desde GUIMaker / Ferrous Builder.

| Tipo | Descripción |
|---|---|
| `PropValue` | Enum serializable: `String`, `Float`, `Bool`, `Color`, `Rect`, `Int` |
| `InspectorProp` | Metadatos de una propiedad: `key`, `label`, `category`, `value`, `range`, `tooltip` |
| `FerrousWidgetReflect` | Trait: `widget_type_name()`, `inspect_props()`, `apply_prop(key, val)` |
| `WidgetFactory<App>` | Registro de constructores de widgets por nombre string |
| `FuiNode` | Nodo serializado en formato `.fui` (RON) con `to_ron()` / `from_ron()` |

`WidgetFactory::instantiate_tree()` carga un árbol completo desde un `FuiNode`,
aplicando las propiedades serializadas vía `apply_prop`.

---

### `style_builder.rs` — `StyleBuilder`

API fluent para construir `Style` sin rellenar los campos manualmente:

```rust
let style = StyleBuilder::new()
    .fill_width()
    .height_px(48.0)
    .padding_all(8.0)
    .row()
    .center_items()
    .build();
```

---

### `widgets/` — Catálogo de widgets integrados

| Categoría | Widgets |
|---|---|
| **Basic** | `Button`, `Label`, `Panel`, `Separator`, `Spacer`, `PlaceholderWidget` |
| **Input** | `TextInput`, `Checkbox`, `Slider`, `NumberInput`, `ToggleSwitch`, `DropDown`, `ColorPicker` |
| **Layout** | `ScrollView`, `SplitPane`, `DockLayout`, `AspectRatio` |
| **Display** | `Image`, `Svg`, `ProgressBar` |
| **Navigation** | `Tabs`, `Accordion`, `TreeView`, `Modal`, `Tooltip` |
| **Data** | `DataTable`, `VirtualList`, `VirtualGrid` |
| **Feedback** | `Toast` |
| **Special** | `ViewportWidget` |

Todos los widgets implementan `Widget<App>`. `Button<App>` además implementa
`FerrousWidget` (derive macro) para habilitar reflexión automática.

---

## Feature flags

| Flag | Efecto |
|---|---|
| `assets` | Habilita la variante `RenderCommand::Image` que usa `Arc<ferrous_assets::Texture2d>` |
| *(sin flag)* | `Image` usa un `texture_id: u64` como fallback |

---

## Dependencias

| Crate | Uso |
|---|---|
| `slotmap` | `SlotMap` para el `UiTree` |
| `glam` | `Vec2` para tamaños y posiciones |
| `serde` + `ron` | Serialización del sistema de reflexión y `.fui` |
| `ferrous_ui_macros` | `#[derive(FerrousWidget)]` y la macro `ui!` |
| `ferrous_assets` *(opcional)* | Texturas para `RenderCommand::Image` |


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
use ferrous_ui_core::{UiTree, Widget, BuildContext, Panel, Label, Button};

// 1. Declarar el estado de tu aplicación
struct MyApp {
    score: i32,
}

// 2. Definir un widget personalizado genérico sobre tu App
struct MyPanel;
impl Widget<MyApp> for MyPanel {
    fn build(&mut self, ctx: &mut BuildContext<MyApp>) {
        ctx.add_child(Box::new(Label::new("Hola, Ferrous!")));
        ctx.add_child(Box::new(Button::new("Click me").on_click(|ctx| {
            ctx.app.score += 1;
            println!("Score: {}", ctx.app.score);
        })));
    }
}

// 3. Instanciar el árbol con el tipo de tu aplicación
let mut tree = UiTree::<MyApp>::new();
tree.add_node(Box::new(MyPanel), None); // Nodo raíz
tree.build(); // Ejecuta la fase de construcción recursiva
```

---

## Callbacks en Widgets (Fase 5.2)

`Button` y `Slider` ahora soportan closures directamente en su builder, sin necesidad de implementar `Widget::on_event` manualmente:

```rust
use ferrous_ui_core::Button;

let btn = Button::new("Eliminar")
    .on_click(|ctx| {
        println!("Elemento eliminado");
    })
    .on_hover(|ctx| println!("hover activado"))
    .with_radius(6.0);
```

```rust
use ferrous_ui_core::Slider;

let slider = Slider::new(0.5, 0.0, 1.0)
    .on_change(|ctx, v| println!("Nuevo volumen: {:.2}", v));
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
| [`TextInput`](widgets/text_input.md) | Campo editable de una sola línea con soporte de teclado, cursores y enlazamiento de texto. |
| [`NumberInput`](widgets/number_input.md) | Input especializado en números con validación y parseo automático. |
| [`Slider`](widgets/slider.md) | Control de arrastre para `f32` con `on_change` y soporte de `Observable<f32>`. |
| [`Checkbox`](widgets/checkbox.md) | Toggle interactivo booleano con soporte reactivo. |
| [`ToggleSwitch`](widgets/toggle_switch.md) | Switch alternativo a checkbox, ideal para interfaces mobile. |
| [`DropDown`](widgets/drop_down.md) | Selector desplegable con lista de opciones y callback de cambio. |
| [`ColorPicker`](widgets/color_picker.md) | Picker HSV inline. Tres formas: `Circle`, `Rect`, `Triangle`. Soporte de `Observable<[f32;4]>`. |
| [`ScrollView`](widgets/scroll_view.md) | Contenedor con scroll vertical/horizontal y recorte de hijos. |
| [`Tabs`](widgets/tabs.md) | Navegación por pestañas con lazy rendering. Solo el contenido activo vive en el árbol. |
| [`Accordion`](widgets/accordion.md) | Sección expandible/colapsable con icono animado y `Overflow::Hidden`. |
| [`SplitPane`](widgets/split_pane.md) | División en dos paneles con divisor arrastrable. Orientación H/V, `ratio_range` configurable. |
| [`Tooltip`](widgets/tooltip.md) | Popup de texto con delay configurable. Posicionamiento automático dentro del viewport. |
| [`Modal`](widgets/modal.md) | Diálogo flotante bloqueante con backdrop. Cierre con `[x]`, backdrop click o `Escape`. |
| [`Toast`](widgets/toast.md) | Notificaciones efímeras apilables. 4 niveles semánticos + barra de progreso. Slide+fade ease-out. |
| [`AspectRatio`](widgets/aspect_ratio.md) | Obliga al hijo a mantener proporción fija `w/h`. Letterbox/pillarbox negro automático. |
| [`DockLayout`](widgets/dock_layout.md) | Sistema de paneles anclables tipo IDE (Left/Right/Top/Bottom/Center). Divisores arrastrables. |
| [`ProgressBar`](widgets/progress_bar.md) | Indicador visual de progreso de un proceso (0.0 a 1.0). |
| [`Separator`](widgets/separator.md) | Línea divisoria configurable para layouts estables. |
| [`Spacer`](widgets/spacer.md) | Widget elástico layout que no pinta información visual. |
| [`PlaceholderWidget`](widgets/placeholder.md) | Nodo vacío para uso estructural o provisional. |

---

## Further reading

- [Tipos y estructuras del núcleo](CORE.md)
- [Motor de layout — ferrous_layout](../../ferrous_layout/docs/README.md)
- [Backend de renderizado — ferrous_ui_render](../../ferrous_ui_render/docs/README.md)
- [Sistema de eventos — ferrous_events](../../ferrous_events/docs/README.md)
