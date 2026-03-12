# ferrous_gui

Orquestador principal ("facade") del sistema de UI de Ferrous Engine.

Coordina en un único punto de entrada los cuatro sub-sistemas independientes:
árbol de widgets (`ferrous_ui_core`), motor de layout (`ferrous_layout`),
gestor de eventos (`ferrous_events`) y backend de renderizado (`ferrous_ui_render`).

---

## Arquitectura

```
┌──────────────────────────────────────────────────────────┐
│                       ferrous_gui                        │
│                                                          │
│   UiSystem<App>                                          │
│   ├── UiTree<App>         ← ferrous_ui_core              │
│   ├── LayoutEngine        ← ferrous_layout               │
│   ├── EventManager        ← ferrous_events               │
│   └── render() ──────────► GuiBatch → ferrous_ui_render  │
│                                                          │
│   builder.rs  (API fluent)                               │
│   ButtonBuilder / LabelBuilder / PanelBuilder /          │
│   WidgetBuilder                                          │
└──────────────────────────────────────────────────────────┘
```

El tipo central es `UiSystem<App>`, genérico sobre el estado de la aplicación
(`App`). Todos los callbacks de eventos reciben `&mut EventContext<App>`, lo
que les permite mutar el estado de la aplicación directamente.

---

## Módulos

### `system` — `UiSystem<App>`

El orquestador. Expone el ciclo de vida completo de la UI:

| Método | Descripción |
|---|---|
| `new()` | Crea el sistema con árbol, layout y eventos vacíos |
| `add(widget)` | Añade un widget genérico a la raíz del árbol |
| `update(dt, w, h)` | Actualiza widgets y recalcula el layout (Taffy/Flexbox) |
| `dispatch_event(app, event)` | Hit-test + bubbling a través de `EventManager` |
| `render(viewport)` | Recolecta `RenderCommand`s y genera un `GuiBatch` para WGPU |

**API fluent** (métodos de conveniencia — ver sección siguiente):

| Método | Devuelve |
|---|---|
| `ui.button("texto")` | `ButtonBuilder<App>` |
| `ui.label("texto")` | `LabelBuilder<App>` |
| `ui.panel()` | `PanelBuilder<App>` |
| `ui.widget(w)` | `WidgetBuilder<App>` |

### `builder` — Builders fluent

Permite crear, posicionar y asignar eventos a widgets en una sola cadena sin
tocar `UiTree`, `Style` ni `StyleBuilder` directamente.

Todos los builders comparten los mismos modificadores:

| Modificador | Efecto |
|---|---|
| `.at(x, y)` | Posición absoluta en el canvas (`Position::Absolute`) |
| `.size(w, h)` | Tamaño fijo en píxeles |
| `.width(w)` / `.height(h)` | Solo una dimensión |
| `.child_of(parent_id)` | Hace al widget hijo del nodo indicado |
| `.id("nombre")` | ID de texto para búsqueda posterior con `get_node_by_id` |
| `.spawn(&mut ui)` | **Instancia** el widget → devuelve `NodeId` |
| `.spawn_with(&mut ui, \|ui, id\| …)` | Instancia y permite añadir hijos inline |

`ButtonBuilder` añade además:

| Modificador | Efecto |
|---|---|
| `.on_click(\|ctx\| …)` | Callback al hacer clic |
| `.on_hover(\|ctx\| …)` | Callback cuando el cursor entra |
| `.on_hover_end(\|ctx\| …)` | Callback cuando el cursor sale |

---

## Ejemplos de uso

### Botón en posición absoluta con evento

```rust
use ferrous_gui::prelude::*;

ui.button("Guardar")
    .at(100.0, 200.0)
    .size(120.0, 36.0)
    .on_click(|_ctx| println!("¡Guardado!"))
    .spawn(&mut ui);
```

### Label con estilo

```rust
ui.label("Versión 1.0")
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

### Loop principal típico

```rust
// 1. Actualizar lógica y layout
ui.update(dt, viewport_w, viewport_h);

// 2. Enviar eventos del OS
ui.dispatch_event(&mut app_state, UiEvent::MouseDown { button, pos });

// 3. Renderizar
let batch: GuiBatch = ui.render(viewport_rect);
gui_renderer.draw(&batch, &queue, &device);
```

---

## `prelude`

Importa todo lo necesario para la API fluent en una sola línea:

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
| `ferrous_ui_core` | Árbol de widgets, tipos de estilo, trait `Widget`, eventos abstractos |
| `ferrous_layout` | Motor Taffy (Flexbox) para cálculo de posiciones |
| `ferrous_events` | Hit-testing, bubbling, conversores de winit |
| `ferrous_ui_render` | Generación de `GuiBatch` / quads para WGPU |
| `ferrous_assets` *(opcional)* | Soporte de fuentes para renderizado de texto |

---

## Feature flags

| Flag | Efecto |
|---|---|
| `text` | Habilita la variante de `render()` que acepta una fuente y genera `TextQuad`s |
| *(sin flag)* | `render()` solo produce `GuiQuad`s geométricos |


Esta guía está diseñada para que cualquier programador pueda utilizar este sistema para construir herramientas complejas, como un **Ferrous Builder**, **Scene Builder** o editores especializados en otros workspaces.

---

## 1. Topología del Sistema y el Rol de Orchestrador

El ecosistema de UI está compuesto por múltiples crates. `ferrous_gui` actúa como el punto de entrada principal, reexportando y coordinando las piezas:

*   **`ferrous_gui` (El Orquestador)**: Proveedor central que reexporta tipos. Es el único crate que una aplicación final necesita importar para construir interfaces. Coordina el árbol de UI (`UiTree`) y el ciclo de vida de los widgets.
*   **`ferrous_ui_core`**: Contiene la definición base del trait `Widget<App>`, los contextos (`EventContext`, `DrawContext`, `LayoutContext`) y todos los componentes estándar (`Button`, `Slider`, `ColorPicker`, `Panel`, etc.).
*   **`ferrous_layout`**: Motor de posicionamiento. Basado en flexbox, procesa el árbol de nodos de la UI para calcular posiciones absolutas y dimensiones (`Rect`) de cada `NodeId`.
*   **`ferrous_events` / `ferrous_input`**: Manejo de eventos del teclado, ratón, toques, propagación, foco y hit-testing ("¿se hizo clic en este rectángulo?").
*   **`ferrous_ui_render`**: El backend de renderizado. Define el trait `ToBatches` para traducir abstracciones (`RenderCommand`) en quads y vértices (`GuiBatch`) que FerrousEngine renderiza usando `wgpu`.

---

## 2. Creando un Editor o Builder (Ejemplo: `FerrousBuilder`)

Para crear una herramienta gráfica compleja (como el editor principal o un visor de escena separado), necesitas definir una estructura de estado y conectarla al ecosistema de `FerrousApp`.

### Paso 1: Definir el Estado de la Aplicación

Tu aplicación dictará el tipo genérico con el que se instancian los widgets (por ejemplo, `Button<FerrousBuilder>`).

```rust
use ferrous_app::{App, AppContext, FerrousApp, DrawContext};
use ferrous_gui::{UiTree, Button, Style, Units, NodeId};

// Este es tu estado principal
pub struct FerrousBuilder {
    pub show_grid: bool,
    pub camera_speed: f32,
    
    // Guardamos los ID de los nodos para usarlos o referenciarlos después
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

### Paso 2: Configurar el Árbol de UI (`configure_ui`)

La construcción de la UI se realiza una sola vez de forma declarativa y se delega el control de estado a cierres (closures) reactivos usando `EventContext`. 

El layout ya **no** se define de forma absoluta en la creación del widget (`Button::new(x, y, w, h)` es obsoleto). Ahora confías en el sistema de Layout.

```rust
impl FerrousApp for FerrousBuilder {
    fn configure_ui(&mut self, ui: &mut UiTree<Self>) {
        // Crear un botón genérico tipado con nuestra aplicación
        let btn_grid = Button::new("Toggle Grid")
            .on_click(|ctx| {
                // ctx es &mut EventContext<'_, FerrousBuilder>
                // Mutar el estado directamente
                ctx.app.show_grid = !ctx.app.show_grid;
            });
            
        // Registrar en el árbol de UI y guardar el NodeId resultante
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
        // Lógica de juego, movimiento de cámara, actualización de escenas...
    }
}
```

---

## 3. Manejo de Eventos y Callbacks Reactivos

A diferencia de implementaciones legacy (donde los valores se chequeaban leyendo `RefCell` en cada frame), el nuevo `ferrous_gui` es impulsado por eventos directos. 

Cuando el layout hace hit de un clic o interacción sobre un widget, este gatilla el callback configurado y le pasa el `EventContext`. El `EventContext` contiene un puntero mutable hacia tu `App`.

### Sliders y Controles de Valor Constante
```rust
use ferrous_gui::Slider;

let speed_slider = Slider::new(1.0, 0.1, 10.0)
    .on_change(|ctx, new_value| {
        ctx.app.camera_speed = new_value;
    });
```

El estado es el único dueño de la verdad (Single Source of Truth), y los widgets informan sus cambios directamente hacia él.

---

## 4. Dibujado de la UI: Automático vs Manual

### Dibujado Automático (Recomendado)
El motor recorre el `UiTree`, computa las dimensiones mediante yoga/flexbox para cada `NodeId`, y emite comandos de renderizado (`RenderCommand`). El trait `ToBatches` los convierte en `GuiBatch` quads implícitamente.

### Dibujado Manual (Paneles Especializados)
En ocasiones, como en un Inspector de Materiales (`MaterialInspector`), puede ser necesario realizar el dibujo controlando exactamente el contexto:

```rust
use ferrous_gui::{DrawContext, ToBatches, Rect};

// En tu método draw_ui:
fn draw_ui(&mut self, dc: &mut ferrous_app::DrawContext<'_, '_>) {
    let font = dc.font;
    let gui = &mut *dc.gui;
    
    // 1. Dibujado primitivo de fondos o lineas
    gui.push_quad( /* ... GuiQuad manual ... */ );
    gui.draw_text(font, "Inspector", [20.0, 30.0], 14.0, [1.0, 1.0, 1.0, 1.0]);

    // 2. Extraer parámetros calculados por el layout
    let btn_id = self.grid_btn_id.unwrap();
    // Suponiendo que conoces dónde lo quieres dibujar
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

## 5. Migración de Código Antiguo (Legacy)

Al actualizar de versiones previas de `ferrous_gui` o construir código nuevo en este workspace con la memoria muscular antigua, ten en cuenta las siguientes **obsolescencias absolutas**:

1. **NO uses `RefCell` ni `Rc`** para guardar referencias de widgets vivos con la esperanza de leer si `.pressed` o `.value` cambió. Usa el API reactivo (`on_click()`, `on_change()`).
2. **NO pases dimensiones al constructor `new()`**. `Button::new(x, y, w, h)` ya no existe. El tamaño y posición son gobernados por `ferrous_layout::Style`.
3. **NO uses `PanelBuilder`**. Fue erradicado. La composición jerárquica ahora debe hacerse registrando sub-nodos y definiendo la relación de flexbox (`display: Display::Flex`, `flex_direction`).
4. **NO uses `gui.quads.push()` ni `TextBatch` separado**. `ferrous_ui_render::GuiBatch` ha sido unificado. Usa `gui.push_quad()` y `gui.draw_text()`. El layout MSDF ya compensa el padding.

---

## 6. Resumen de Flujo de Trabajo Moderno

1. Incluye `use ferrous_gui::*;` (Actúa como orquestador único).
2. Modela tu aplicación en base a estados transparentes (`struct MyBuilder`).
3. En `configure_ui`, inicializa componentes (vía `::new(...).on_event(|ctx| { ... })`).
4. Añade los componentes al `UiTree` con `add_node()`. Recibes un `NodeId`.
5. Delega la responsibilidad de posición a `set_node_style` usando variables de flexibilidad o pixeles en el árbol.
6. Corre el programa. Los callbacks escucharán el evento cuando se requiera. Las propiedades visuales fluirán del árbol general al frame render.
