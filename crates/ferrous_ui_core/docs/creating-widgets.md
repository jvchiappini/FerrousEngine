# Creando Widgets Personalizados

Esta guía explica cómo crear un widget nuevo de principio a fin: el widget en `ferrous_ui_core`, su builder en `ferrous_gui`, y cómo exponerlo a través de `UiSystem`.

---

## Índice

1. [Arquitectura general](#1-arquitectura-general)
2. [Paso 1 — El Widget (`ferrous_ui_core`)](#2-paso-1--el-widget)
3. [Paso 2 — El Builder (`ferrous_gui`)](#3-paso-2--el-builder)
4. [Paso 3 — Factory method en `UiSystem`](#4-paso-3--factory-method-en-uisystem)
5. [Paso 4 — Exponer en el Prelude](#5-paso-4--exponer-en-el-prelude)
6. [Ejemplo completo — `BadgeWidget`](#6-ejemplo-completo)
7. [Referencia de la trait `Widget<App>`](#7-referencia-de-la-trait-widgetapp)
8. [Referencia de `RenderCommand`](#8-referencia-de-rendercommand)

---

## 1. Arquitectura general

```
ferrous_ui_core/src/widgets/mi_widget.rs   ← Lógica + dibujo del widget
ferrous_gui/src/builder/mi_widget.rs       ← API fluent para crearlo
ferrous_gui/src/system.rs                  ← Factory method ui.mi_widget(...)
```

Los tres niveles están separados por diseño:

| Capa            | Responsabilidad                                          |
|-----------------|----------------------------------------------------------|
| `ferrous_ui_core` | Estado, lógica de eventos, comandos de dibujo          |
| `ferrous_gui`     | Builder fluent, `UiSystem`, integración de layout      |
| Aplicación        | Llamadas a `ui.mi_widget().spawn(&mut ui)`             |

---

## 2. Paso 1 — El Widget

Crea `crates/ferrous_ui_core/src/widgets/mi_widget.rs`.

### Estructura mínima

```rust
use crate::{Widget, RenderCommand, DrawContext, BuildContext, EventContext,
            EventResponse, UiEvent};

pub struct MiWidget {
    // Campos de estado del widget
    pub texto: String,
}

impl MiWidget {
    pub fn new(texto: impl Into<String>) -> Self {
        Self { texto: texto.into() }
    }
}

impl<App> Widget<App> for MiWidget {
    fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
        // Fondo
        cmds.push(RenderCommand::Quad {
            rect: ctx.rect,
            color: ctx.theme.surface.to_array(),
            radii: [ctx.theme.border_radius; 4],
            flags: 0,
        });
        // Texto
        cmds.push(RenderCommand::Text {
            rect: ctx.rect,
            text: self.texto.clone(),
            color: ctx.theme.on_surface.to_array(),
            font_size: ctx.theme.font_size_base,
        });
    }
}
```

### Métodos opcionales de `Widget<App>`

| Método           | Cuándo implementarlo                                      |
|------------------|-----------------------------------------------------------|
| `build`          | Necesitas añadir nodos hijos al insertar el widget        |
| `update`         | Animaciones, timers, polling de estado                    |
| `calculate_size` | El widget tiene dimensiones intrínsecas (ej. texto)       |
| `on_event`       | El widget reacciona al mouse, teclado u otros eventos     |
| `scroll_offset`  | El widget es scrollable                                   |
| `reflect`        | Exponer propiedades al editor (usa `#[derive(FerrousWidget)]`) |

### Añadir hijos en `build`

Si tu widget necesita nodos hijos permanentes (ej. un panel con un label interno):

```rust
fn build(&mut self, ctx: &mut BuildContext<App>) {
    // El `node_id` de este widget ya existe en el árbol.
    // Añade hijos con ctx.add_child(...)
    let label = Box::new(Label::new("Hola"));
    ctx.add_child(label);
}
```

### Manejar eventos en `on_event`

```rust
fn on_event(&mut self, ctx: &mut EventContext<App>, event: &UiEvent) -> EventResponse {
    match event {
        UiEvent::MouseDown { .. } => {
            // ctx.app es tu estado de aplicación
            println!("Clic en ({}, {})", ctx.rect.x, ctx.rect.y);
            EventResponse::Consumed  // detiene la propagación
        }
        UiEvent::MouseEnter => {
            self.hovered = true;
            EventResponse::Redraw    // pide repintar este nodo
        }
        UiEvent::MouseLeave => {
            self.hovered = false;
            EventResponse::Redraw
        }
        _ => EventResponse::Ignored, // deja pasar el evento al padre
    }
}
```

#### Valores de `EventResponse`

| Variante    | Efecto                                             |
|-------------|----------------------------------------------------|
| `Ignored`   | El evento sigue propagándose hacia el padre        |
| `Consumed`  | El evento se detiene aquí                          |
| `Redraw`    | Pide repintar solo este nodo (sin recalcular layout)|

### Registrar el widget en `widgets/mod.rs`

```rust
// En crates/ferrous_ui_core/src/widgets/mod.rs
pub mod mi_widget;
pub use mi_widget::*;
```

---

## 3. Paso 2 — El Builder

Crea `crates/ferrous_gui/src/builder/mi_widget.rs`.

```rust
//! [`MiWidgetBuilder`] — builder fluent para MiWidget.

use ferrous_ui_core::{MiWidget, NodeId};

use crate::UiSystem;
use super::base::{BuilderBase, impl_builder_base};

pub struct MiWidgetBuilder<App: 'static> {
    pub(super) inner: MiWidget,
    pub(super) base: BuilderBase,
    pub(super) _app: std::marker::PhantomData<fn() -> App>,
}

// Genera: .at() .size() .width() .height() .fill() .child_of() .id()
impl_builder_base!(MiWidgetBuilder<App>);

impl<App: 'static> MiWidgetBuilder<App> {
    pub(crate) fn new(texto: impl Into<String>) -> Self {
        Self {
            inner: MiWidget::new(texto),
            base: BuilderBase::default(),
            _app: std::marker::PhantomData,
        }
    }

    // ── Métodos específicos del widget ──────────────────────────────────

    /// Cambia el texto mostrado.
    pub fn texto(mut self, t: impl Into<String>) -> Self {
        self.inner.texto = t.into();
        self
    }

    // ── spawn ────────────────────────────────────────────────────────────

    /// Instancia el widget en el `UiSystem` y devuelve su `NodeId`.
    pub fn spawn(self, ui: &mut UiSystem<App>) -> NodeId {
        let (style, explicit_parent, id_str) = self.base.into_style();
        let parent = explicit_parent.or_else(|| ui.current_parent());
        let id = ui.tree.add_node_with_id(Box::new(self.inner), parent, id_str);
        ui.tree.set_node_style(id, style);
        id
    }
}
```

### Registrar en `builder/mod.rs`

```rust
// En crates/ferrous_gui/src/builder/mod.rs
mod mi_widget;
pub use mi_widget::MiWidgetBuilder;
```

---

## 4. Paso 3 — Factory method en `UiSystem`

Abre `crates/ferrous_gui/src/system.rs` y añade el método factory:

```rust
use crate::builder::{ButtonBuilder, LabelBuilder, PanelBuilder, WidgetBuilder, MiWidgetBuilder};

impl<App: 'static> UiSystem<App> {
    // ... métodos existentes ...

    /// Crea un builder fluent para `MiWidget`.
    pub fn mi_widget(&self, texto: impl Into<String>) -> MiWidgetBuilder<App> {
        MiWidgetBuilder::new(texto)
    }
}
```

---

## 5. Paso 4 — Exponer en el Prelude

Si quieres que `use ferrous_gui::prelude::*` incluya tu builder:

```rust
// En crates/ferrous_gui/src/lib.rs — módulo prelude
pub mod prelude {
    pub use crate::UiSystem;
    pub use crate::builder::{ButtonBuilder, LabelBuilder, PanelBuilder, WidgetBuilder, MiWidgetBuilder};
    pub use ferrous_ui_core::{NodeId, Color, Alignment, DisplayMode};
}
```

---

## 6. Ejemplo completo

### Widget `Badge` — pastilla de texto con color personalizable

#### `ferrous_ui_core/src/widgets/badge.rs`

```rust
use crate::{Widget, RenderCommand, DrawContext, Color};

pub struct Badge {
    pub text: String,
    pub color: Option<Color>,
}

impl Badge {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into(), color: None }
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
}

impl<App> Widget<App> for Badge {
    fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
        let bg = self.color.unwrap_or(ctx.theme.primary);

        cmds.push(RenderCommand::Quad {
            rect: ctx.rect,
            color: bg.to_array(),
            radii: [ctx.rect.height / 2.0; 4], // píldora perfecta
            flags: 0,
        });
        cmds.push(RenderCommand::Text {
            rect: ctx.rect,
            text: self.text.clone(),
            color: ctx.theme.on_primary.to_array(),
            font_size: ctx.theme.font_size_base * 0.85,
        });
    }
}
```

#### `ferrous_gui/src/builder/badge.rs`

```rust
use ferrous_ui_core::{Badge, Color, NodeId};
use crate::UiSystem;
use super::base::{BuilderBase, impl_builder_base};

pub struct BadgeBuilder<App: 'static> {
    pub(super) inner: Badge,
    pub(super) base: BuilderBase,
    pub(super) _app: std::marker::PhantomData<fn() -> App>,
}

impl_builder_base!(BadgeBuilder<App>);

impl<App: 'static> BadgeBuilder<App> {
    pub(crate) fn new(text: impl Into<String>) -> Self {
        Self {
            inner: Badge::new(text),
            base: BuilderBase::default(),
            _app: std::marker::PhantomData,
        }
    }

    pub fn color(mut self, c: Color) -> Self {
        self.inner = self.inner.with_color(c);
        self
    }

    pub fn spawn(self, ui: &mut UiSystem<App>) -> NodeId {
        let (style, explicit_parent, id_str) = self.base.into_style();
        let parent = explicit_parent.or_else(|| ui.current_parent());
        let id = ui.tree.add_node_with_id(Box::new(self.inner), parent, id_str);
        ui.tree.set_node_style(id, style);
        id
    }
}
```

#### Uso final en la aplicación

```rust
use ferrous_gui::prelude::*;

// Badge suelto
ui.badge("Nuevo")
    .size(60.0, 22.0)
    .at(200.0, 50.0)
    .spawn(&mut ui);

// Badge dentro de un panel
ui.panel()
    .row()
    .gap(8.0)
    .padding(12.0)
    .spawn_with(&mut ui, |ui, _| {
        ui.label("Estado:").spawn(ui);
        ui.badge("Activo")
            .color(Color::from_rgb(0.2, 0.8, 0.3))
            .size(60.0, 22.0)
            .spawn(ui);
    });
```

---

## 7. Referencia de la trait `Widget<App>`

```rust
pub trait Widget<App> {
    /// Llamado una vez al insertar el widget en el árbol.
    /// Úsalo para añadir hijos o configurar el estilo inicial.
    fn build(&mut self, ctx: &mut BuildContext<App>) {}

    /// Llamado cada frame. Para animaciones, timers, polling.
    fn update(&mut self, ctx: &mut UpdateContext) {}

    /// Tamaño intrínseco sugerido al motor de layout.
    /// El builder puede sobreescribir este valor con `.size()`.
    fn calculate_size(&self, ctx: &mut LayoutContext) -> Vec2 { Vec2::ZERO }

    /// Genera los comandos de dibujo. Se cachea automáticamente
    /// entre frames si el nodo no está marcado como dirty.
    fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {}

    /// Reacciona a eventos de entrada. Devuelve `EventResponse`
    /// para indicar si el evento fue consumido o ignorado.
    fn on_event(&mut self, ctx: &mut EventContext<App>, event: &UiEvent) -> EventResponse {
        EventResponse::Ignored
    }
}
```

### Contextos disponibles

| Contexto        | Disponible en   | Campos clave                                    |
|-----------------|----------------|-------------------------------------------------|
| `BuildContext`  | `build`        | `tree`, `node_id`, `theme`                      |
| `UpdateContext` | `update`       | `delta_time`, `node_id`, `rect`, `theme`        |
| `LayoutContext` | `calculate_size` | `available_space`, `known_dimensions`, `theme` |
| `DrawContext`   | `draw`         | `rect`, `theme`, `node_id`                      |
| `EventContext`  | `on_event`     | `node_id`, `rect`, `theme`, `tree`, `app`       |

---

## 8. Referencia de `RenderCommand`

| Variante    | Úsala para                        | Campos                                              |
|-------------|-----------------------------------|-----------------------------------------------------|
| `Quad`      | Fondos, bordes, formas            | `rect`, `color: [f32;4]`, `radii: [f32;4]`, `flags`|
| `Text`      | Texto                             | `rect`, `text`, `color: [f32;4]`, `font_size`       |
| `Image`     | Imágenes/texturas (feature `assets`) | `rect`, `texture`, `uv0`, `uv1`, `color`         |
| `PushClip`  | Iniciar región de recorte         | `rect`                                              |
| `PopClip`   | Terminar región de recorte        | —                                                   |

### Colores

Los colores se pasan como `[f32; 4]` (RGBA normalizado 0.0–1.0). Usa `Color::to_array()`:

```rust
ctx.theme.primary.to_array()        // Color del tema
Color::from_rgb(1.0, 0.5, 0.0).to_array() // Naranja
```

### Radio de esquinas (radii)

```rust
radii: [ctx.theme.border_radius; 4]  // Uniforme desde el tema
radii: [8.0, 8.0, 0.0, 0.0]         // Solo esquinas superiores
radii: [rect.height / 2.0; 4]        // Píldora perfecta
```
