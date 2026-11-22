# ferrous_gui

> **Draw-only since prompt 15 phase 4.** `UiSystem` lays out and paints; the
> runner no longer feeds it pointer or keyboard events and
> `ferrous_events::no router   ` was removed. `on_click`/`on_change`/`on_event`
> callbacks do not fire. Input, gestures, focus, shortcuts, scrolling and text
> editing are `guimaker::input`; build new UI on GUIMaker. The retained UI is
> kept for the engine's layout/paint path until `ferrous_editor` is ported
> (Phase 11 backlog).

Orquestador principal ("facade") del sistema de UI de Ferrous Engine.

Coordina en un Ãºnico punto de entrada los tres sub-sistemas que quedan:
Ã¡rbol de widgets (`ferrous_ui_core`), motor de layout (`ferrous_layout`) y
backend de renderizado (`ferrous_ui_render`). El enrutado de entrada ya no
vive aquÃ­.

---

## Arquitectura

```
â”Œâ”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”
â”‚                       ferrous_gui                        â”‚
â”‚                                                          â”‚
â”‚   UiSystem<App>                                          â”‚
â”‚   â”œâ”€â”€ UiTree<App>         â† ferrous_ui_core              â”‚
â”‚   â”œâ”€â”€ LayoutEngine        â† ferrous_layout               â”‚
â”‚   â”œâ”€â”€ no router           â† ferrous_events               â”‚
â”‚   â””â”€â”€ render() â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â–º GuiBatch â†’ ferrous_ui_render  â”‚
â”‚                                                          â”‚
â”‚   builder.rs  (API fluent)                               â”‚
â”‚   ButtonBuilder / LabelBuilder / PanelBuilder /          â”‚
â”‚   WidgetBuilder                                          â”‚
â””â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”˜
```

El tipo central es `UiSystem<App>`, genÃ©rico sobre el estado de la aplicaciÃ³n
(`App`). Todos los callbacks de eventos reciben `&mut EventContext<App>`, lo
que les permite mutar el estado de la aplicaciÃ³n directamente.

---

## MÃ³dulos

### `system` â€” `UiSystem<App>`

El orquestador. Expone el ciclo de vida completo de la UI:

| MÃ©todo | DescripciÃ³n |
|---|---|
| `new()` | Crea el sistema con Ã¡rbol, layout y eventos vacÃ­os |
| `add(widget)` | AÃ±ade un widget genÃ©rico a la raÃ­z del Ã¡rbol |
| `update(dt, w, h)` | Actualiza widgets y recalcula el layout (Taffy/Flexbox) |
| ~~`dispatch_event(app, event)`~~ | Removed in prompt 15 phase 4; input routing is `guimaker::input` |
| `render(viewport)` | Recolecta `RenderCommand`s y genera un `GuiBatch` para WGPU |

**API fluent** (mÃ©todos de conveniencia â€” ver secciÃ³n siguiente):

| MÃ©todo | Devuelve |
|---|---|
| `ui.button("texto")` | `ButtonBuilder<App>` |
| `ui.label("texto")` | `LabelBuilder<App>` |
| `ui.panel()` | `PanelBuilder<App>` |
| `ui.widget(w)` | `WidgetBuilder<App>` |

### `builder` â€” Builders fluent

Permite crear, posicionar y asignar eventos a widgets en una sola cadena sin
tocar `UiTree`, `Style` ni `StyleBuilder` directamente.

Todos los builders comparten los mismos modificadores:

| Modificador | Efecto |
|---|---|
| `.at(x, y)` | PosiciÃ³n absoluta en el canvas (`Position::Absolute`) |
| `.size(w, h)` | TamaÃ±o fijo en pÃ­xeles |
| `.width(w)` / `.height(h)` | Solo una dimensiÃ³n |
| `.child_of(parent_id)` | Hace al widget hijo del nodo indicado |
| `.id("nombre")` | ID de texto para bÃºsqueda posterior con `get_node_by_id` |
| `.spawn(&mut ui)` | **Instancia** el widget â†’ devuelve `NodeId` |
| `.spawn_with(&mut ui, \|ui, id\| â€¦)` | Instancia y permite aÃ±adir hijos inline |

`ButtonBuilder` aÃ±ade ademÃ¡s:

| Modificador | Efecto |
|---|---|
| `.on_click(\|ctx\| â€¦)` | Callback al hacer clic |
| `.on_hover(\|ctx\| â€¦)` | Callback cuando el cursor entra |
| `.on_hover_end(\|ctx\| â€¦)` | Callback cuando el cursor sale |

---

## Ejemplos de uso

### BotÃ³n en posiciÃ³n absoluta con evento

```rust
use ferrous_gui::prelude::*;

ui.button("Guardar")
    .at(100.0, 200.0)
    .size(120.0, 36.0)
    .on_click(|_ctx| println!("Â¡Guardado!"))
    .spawn(&mut ui);
```

### Label con estilo

```rust
ui.label("VersiÃ³n 1.0")
    .at(10.0, 10.0)
    .font_size(12.0)
    .color(Color::rgb(0.6, 0.6, 0.6))
    .spawn(&mut ui);
```

### Panel con hijos inline

```rust
ui.panel()
    .at(50.0, 100.0)
    .size(300.0, 150.0)
    .spawn_with(&mut ui, |ui, panel| {
        ui.button("Cancelar")
            .child_of(panel).at(8.0, 8.0).size(90.0, 32.0)
            .on_click(|_| println!("cancelado"))
            .spawn(ui);
        ui.button("Aceptar")
            .child_of(panel).at(106.0, 8.0).size(90.0, 32.0)
            .on_click(|_| println!("aceptado"))
            .spawn(ui);
    });
```

### Cualquier widget custom

```rust
ui.widget(Slider::new(0.0, 100.0))
    .at(20.0, 80.0)
    .size(200.0, 28.0)
    .spawn(&mut ui);
```

### Loop principal tÃ­pico

```rust
// 1. Actualizar lógica y layout
ui.update(dt, viewport_w, viewport_h);

// 2. Renderizar (la entrada ya no pasa por UiSystem: es `guimaker::input`)
let batch: GuiBatch = ui.render(viewport_rect);
gui_renderer.draw(&batch, &queue, &device);
```

---

## `prelude`

Importa todo lo necesario para la API fluent en una sola lÃ­nea:

```rust
use ferrous_gui::prelude::*;
// Disponible: UiSystem, ButtonBuilder, LabelBuilder, PanelBuilder,
//             WidgetBuilder, NodeId, Color, Rect, Style, Units,
//             Position, Widget, EventContext, UiEvent, ...
```

---

## Dependencias directas

| Crate | Rol |
|---|---|
| `ferrous_ui_core` | Ãrbol de widgets, tipos de estilo, trait `Widget`, eventos abstractos |
| `ferrous_layout` | Motor Taffy (Flexbox) para cÃ¡lculo de posiciones |
| `ferrous_events` | Hit-testing, bubbling, conversores de winit |
| `ferrous_ui_render` | GeneraciÃ³n de `GuiBatch` / quads para WGPU |
| `ferrous_assets` *(opcional)* | Soporte de fuentes para renderizado de texto |

---

## Feature flags

| Flag | Efecto |
|---|---|
| `text` | Habilita la variante de `render()` que acepta una fuente y genera `TextQuad`s |
| *(sin flag)* | `render()` solo produce `GuiQuad`s geomÃ©tricos |


Esta guÃ­a estÃ¡ diseÃ±ada para que cualquier programador pueda utilizar este sistema para construir herramientas complejas, como un **Ferrous Builder**, **Scene Builder** o editores especializados en otros workspaces.

---

## 1. TopologÃ­a del Sistema y el Rol de Orchestrador

El ecosistema de UI estÃ¡ compuesto por mÃºltiples crates. `ferrous_gui` actÃºa como el punto de entrada principal, reexportando y coordinando las piezas:

*   **`ferrous_gui` (El Orquestador)**: Proveedor central que reexporta tipos. Es el Ãºnico crate que una aplicaciÃ³n final necesita importar para construir interfaces. Coordina el Ã¡rbol de UI (`UiTree`) y el ciclo de vida de los widgets.
*   **`ferrous_ui_core`**: Contiene la definiciÃ³n base del trait `Widget<App>`, los contextos (`EventContext`, `DrawContext`, `LayoutContext`) y todos los componentes estÃ¡ndar (`Button`, `Slider`, `ColorPicker`, `Panel`, etc.).
*   **`ferrous_layout`**: Motor de posicionamiento. Basado en flexbox, procesa el Ã¡rbol de nodos de la UI para calcular posiciones absolutas y dimensiones (`Rect`) de cada `NodeId`.
*   **`ferrous_events` / `ferrous_input`**: Manejo de eventos del teclado, ratÃ³n, toques, propagaciÃ³n, foco y hit-testing ("Â¿se hizo clic en este rectÃ¡ngulo?").
*   **`ferrous_ui_render`**: El backend de renderizado. Define el trait `ToBatches` para traducir abstracciones (`RenderCommand`) en quads y vÃ©rtices (`GuiBatch`) que FerrousEngine renderiza usando `wgpu`.

---

## 2. Creando un Editor o Builder (Ejemplo: `FerrousBuilder`)

Para crear una herramienta grÃ¡fica compleja (como el editor principal o un visor de escena separado), necesitas definir una estructura de estado y conectarla al ecosistema de `FerrousApp`.

### Paso 1: Definir el Estado de la AplicaciÃ³n

Tu aplicaciÃ³n dictarÃ¡ el tipo genÃ©rico con el que se instancian los widgets (por ejemplo, `Button<FerrousBuilder>`).

```rust
use ferrous_engine::{App, AppContext, FerrousApp, DrawContext};
use ferrous_gui::{UiTree, Button, Style, Units, NodeId};

// Este es tu estado principal
pub struct FerrousBuilder {
    pub show_grid: bool,
    pub camera_speed: f32,
    
    // Guardamos los ID de los nodos para usarlos o referenciarlos despuÃ©s
    grid_btn_id: Option<NodeId>,
}

impl Default for FerrousBuilder {
    fn default() -> Self {
        Self {
            show_grid: true,
            camera_speed: 1.0,
            grid_btn_id: None,
        }
    }
}
```

### Paso 2: Configurar el Ãrbol de UI (`configure_ui`)

La construcciÃ³n de la UI se realiza una sola vez de forma declarativa y se delega el control de estado a cierres (closures) reactivos usando `EventContext`. 

El layout ya **no** se define de forma absoluta en la creaciÃ³n del widget (`Button::new(x, y, w, h)` es obsoleto). Ahora confÃ­as en el sistema de Layout.

```rust
impl FerrousApp for FerrousBuilder {
    fn configure_ui(&mut self, ui: &mut UiTree<Self>) {
        // Crear un botÃ³n genÃ©rico tipado con nuestra aplicaciÃ³n
        let btn_grid = Button::new("Toggle Grid")
            .on_click(|ctx| {
                // ctx es &mut EventContext<'_, FerrousBuilder>
                // Mutar el estado directamente
                ctx.app.show_grid = !ctx.app.show_grid;
            });
            
        // Registrar en el Ã¡rbol de UI y guardar el NodeId resultante
        let btn_id = ui.add_node(Box::new(btn_grid), None);
        
        // Estilizar usando ferrous_layout
        ui.set_node_style(btn_id, Style {
            size: (Units::Px(120.0), Units::Px(35.0)),
            margin: ferrous_gui::RectOffset { left: 10.0, top: 10.0, bottom: 0.0, right: 0.0 },
            ..Default::default()
        });
        
        self.grid_btn_id = Some(btn_id);
    }
    
    fn update(&mut self, ctx: &mut AppContext) {
        // LÃ³gica de juego, movimiento de cÃ¡mara, actualizaciÃ³n de escenas...
    }
}
```

---

## 3. Manejo de Eventos y Callbacks Reactivos

A diferencia de implementaciones legacy (donde los valores se chequeaban leyendo `RefCell` en cada frame), el nuevo `ferrous_gui` es impulsado por eventos directos. 

Cuando el layout hace hit de un clic o interacciÃ³n sobre un widget, este gatilla el callback configurado y le pasa el `EventContext`. El `EventContext` contiene un puntero mutable hacia tu `App`.

### Sliders y Controles de Valor Constante
```rust
use ferrous_gui::Slider;

let speed_slider = Slider::new(1.0, 0.1, 10.0)
    .on_change(|ctx, new_value| {
        ctx.app.camera_speed = new_value;
    });
```

El estado es el Ãºnico dueÃ±o de la verdad (Single Source of Truth), y los widgets informan sus cambios directamente hacia Ã©l.

---

## 4. Dibujado de la UI: AutomÃ¡tico vs Manual

### Dibujado AutomÃ¡tico (Recomendado)
El motor recorre el `UiTree`, computa las dimensiones mediante yoga/flexbox para cada `NodeId`, y emite comandos de renderizado (`RenderCommand`). El trait `ToBatches` los convierte en `GuiBatch` quads implÃ­citamente.

### Dibujado Manual (Paneles Especializados)
En ocasiones, como en un Inspector de Materiales (`MaterialInspector`), puede ser necesario realizar el dibujo controlando exactamente el contexto:

```rust
use ferrous_gui::{DrawContext, ToBatches, Rect};

// En tu mÃ©todo draw_ui:
fn draw_ui(&mut self, dc: &mut ferrous_engine::DrawContext<'_, '_>) {
    let font = dc.font;
    let gui = &mut *dc.gui;
    
    // 1. Dibujado primitivo de fondos o lineas
    gui.push_quad( /* ... GuiQuad manual ... */ );
    gui.draw_text(font, "Inspector", [20.0, 30.0], 14.0, [1.0, 1.0, 1.0, 1.0]);

    // 2. Extraer parÃ¡metros calculados por el layout
    let btn_id = self.grid_btn_id.unwrap();
    // Suponiendo que conoces dÃ³nde lo quieres dibujar
    let rect = Rect::new(20.0, 50.0, 120.0, 30.0);
    
    // 3. Crear Contexto de dibujo
    let mut widget_dc = DrawContext {
        node_id: btn_id,
        rect,
        theme: ferrous_gui::theme::Theme::default(),
    };
    
    // 4. Acumular y compilar comandos a batches GPU
    let mut cmds = Vec::new();
    // NOTA: Para dibujado manual debes retener de alguna forma la instancia del widget
    // self.mi_widget.draw(&mut widget_dc, &mut cmds);
    
    for cmd in cmds {
        cmd.to_batches(gui, Some(font));
    }
}
```

---

## 5. MigraciÃ³n de CÃ³digo Antiguo (Legacy)

Al actualizar de versiones previas de `ferrous_gui` o construir cÃ³digo nuevo en este workspace con la memoria muscular antigua, ten en cuenta las siguientes **obsolescencias absolutas**:

1. **NO uses `RefCell` ni `Rc`** para guardar referencias de widgets vivos con la esperanza de leer si `.pressed` o `.value` cambiÃ³. Usa el API reactivo (`on_click()`, `on_change()`).
2. **NO pases dimensiones al constructor `new()`**. `Button::new(x, y, w, h)` ya no existe. El tamaÃ±o y posiciÃ³n son gobernados por `ferrous_layout::Style`.
3. **NO uses `PanelBuilder`**. Fue erradicado. La composiciÃ³n jerÃ¡rquica ahora debe hacerse registrando sub-nodos y definiendo la relaciÃ³n de flexbox (`display: Display::Flex`, `flex_direction`).
4. **NO uses `gui.quads.push()` ni `TextBatch` separado**. `ferrous_ui_render::GuiBatch` ha sido unificado. Usa `gui.push_quad()` y `gui.draw_text()`. El layout MSDF ya compensa el padding.

---

## 6. Resumen de Flujo de Trabajo Moderno

1. Incluye `use ferrous_gui::*;` (ActÃºa como orquestador Ãºnico).
2. Modela tu aplicaciÃ³n en base a estados transparentes (`struct MyBuilder`).
3. En `configure_ui`, inicializa componentes (vÃ­a `::new(...).on_event(|ctx| { ... })`).
4. AÃ±ade los componentes al `UiTree` con `add_node()`. Recibes un `NodeId`.
5. Delega la responsibilidad de posiciÃ³n a `set_node_style` usando variables de flexibilidad o pixeles en el Ã¡rbol.
6. Corre el programa. Los callbacks escucharÃ¡n el evento cuando se requiera. Las propiedades visuales fluirÃ¡n del Ã¡rbol general al frame render.