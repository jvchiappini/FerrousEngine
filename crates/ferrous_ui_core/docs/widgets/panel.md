# Panel

El `Panel` es el widget base para agrupar otros elementos visuales y proveer un fondo consistente (como un lienzo o tarjeta). Dentro del nuevo sistema retenido, todo `Node` en el `UiTree` actúa como un posible contenedor de layout de todos modos, pero el `Panel` otorga capacidades visuales (render de quad) de fondo.

## Construcción

```rust
use ferrous_ui_core::Panel;

// Creamos un panel con color RGBA
let _panel = Panel::new([0.1, 0.1, 0.15, 1.0])
    .with_radius(8.0);
```

## Layout y Hijos

El `Panel` no administra sus hijos o layout internamente; la propia estructura del `UiTree` y el crate `ferrous_layout` gestionan la agrupación jerárquica. Durante la fase de `Widget::build`, el usuario puede agregar hijos al panel a través del contexto:

```rust
use ferrous_ui_core::{Widget, BuildContext, Panel, Label, Button};

struct MyDialog;
impl Widget for MyDialog {
    fn build(&mut self, ctx: &mut BuildContext) {
        // Al agregar un hijo durante el build, éste quedará alojado 
        // bajo este dialog en el UiTree de manera retenida.
        ctx.add_child(Box::new(Panel::new([0.2, 0.2, 0.2, 1.0])));
        ctx.add_child(Box::new(Label::new("Diálogo de Opciones")));
        ctx.add_child(Box::new(Button::new("Cerrar")));
    }
}
```

## Estilizado con StyleBuilder

Para controlar sus márgenes, padding, y comportamiento flexbox, utiliza el `StyleBuilder` de `ferrous_ui_core`:

```rust
use ferrous_ui_core::StyleBuilder;

let style = StyleBuilder::new()
    .column()         // Hijos en vertical
    .padding_all(12.0)
    .gap(8.0)
    .center_items()
    .build();

// Más adelante, asignable a un node_id en el UiTree
tree.set_node_style(node_id, style);
```
