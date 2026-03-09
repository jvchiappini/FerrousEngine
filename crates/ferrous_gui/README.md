# Ferrous GUI

The **2D Widget Toolkit and Orchestrator** for [FerrousEngine](https://github.com/your-repo/FerrousEngine).

`ferrous_gui` unifies all core UI logic into a single cohesive facade. It replaces legacy components with a strongly-typed generic architecture, using `ferrous_layout` for positioning, `ferrous_events` for lifecycle, and `ferrous_ui_render` for backend GPU commands.

## Features

- **Facade Orchestrator**: Exported components, traits, and contexts into a single entry point! `use ferrous_gui::*;` is all you need.
- **Node-Based UI Tree**: Replaces old absolute-positioned components (`x, y, w, h`) with `NodeId`s.
- **Yoga / Flexbox Layout**: Employs `ferrous_layout` for responsive and automated dimension resolution.
- **Generic Architecture**: `Widget<App>` receives `EventContext<App>` callbacks that safely mutate your defined `FerrousApp` state directly.
- **No Shared `RefCell<Rc<...>>` Hell**: States are now handled via `struct MyApp` state mutators on an event (`on_change`, `on_click`).

## Using `ferrous_gui` to build Tools

Whether you build a `Ferrous Builder`, `Scene Builder`, or a simple inspector plugin, you model your layout programmatically using generic implementations of common standard UI widgets: `Button`, `Slider`, `ColorPicker`, etc.

### Documentation

Check out our comprehensive [Architecture and Getting Started Guide](docs/README.md) inside `docs/` to see exactly how to drop legacy assumptions (like `PanelBuilder`) and embrace modern, fast `UiTree` construction.

## Basic Example

```rust
use ferrous_app::{App, AppContext, FerrousApp};
use ferrous_gui::{UiTree, Button, Style, Units};

struct MyBuilder {
    grid_enabled: bool,
}

impl FerrousApp for MyBuilder {
    fn configure_ui(&mut self, ui: &mut UiTree<Self>) {
        let btn = Button::new("Toggle Grid")
            .on_click(|ctx| {
                // Safely toggle properties directly!
                ctx.app.grid_enabled = !ctx.app.grid_enabled;
            });
            
        let node_id = ui.add_node(Box::new(btn), None);
        
        ui.set_node_style(node_id, Style {
            size: (Units::Px(120.0), Units::Px(30.0)),
            ..Default::default()
        });
    }
    
    // update(), setup(), draw_ui() ... 
}
```

See [docs/README.md](docs/README.md) for full context, drawing logic, and component integrations.
