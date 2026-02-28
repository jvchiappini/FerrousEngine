<!--
Documentation for the `ferrous_gui` crate.
This folder is intended to hold markdown files describing the public
API, design decisions and usage examples for all of the GUI components
provided by the crate.  Start with this README as the "home page"; each
sub‑component has its own document linked below.
-->

# ferrous_gui documentation

`ferrous_gui` is a lightweight immediate-style GUI layer used by the
FerrousEngine.  It provides a small collection of widgets, a very simple
layout system, and helpers for integrating with the rest of the engine
(input handling, rendering, etc.).

Most users will simply create a `Ui`, add widgets to it, and forward
winit events via `Ui::handle_window_event`.  The `Canvas`/`Widget`
abstractions are designed to be easy to extend if you need a custom
control.

## Crate structure and documentation

The layout of the documentation mirrors the logical structure of the
crate.  High‑level concepts and core APIs live at the top level; individual
widgets are grouped under `widgets/`.

```
docs/
├── README.md          # this file
├── api/               # core trait and Ui helpers
│   └── core.md
├── layout.md          # layout system reference
└── widgets/           # built-in widget documentation
    ├── button.md
    ├── slider.md
    └── textinput.md
```

## Quick start

Add the crate to your `Cargo.toml`:

```toml
[dependencies]
ferrous_gui = { path = "../ferrous_gui" }
```

Then initialise a UI object in your application:

```rust
let mut ui = ferrous_gui::Ui::new();
// add widgets, handle events and render each frame
```

See the `widgets` section for examples of adding controls and responding
to user input.

## Documentation overview

- **api/core.md** – explanation of the `Widget` trait, the `Canvas`
  container type, and the high‑level `Ui` helper that integrates with
  the engine’s event loop.
- **layout.md** – reference for the small flex‑box‑inspired layout
  system used by editor components and other containers.
- **widgets/** – individual documents for each supplied widget.  For
  example, to learn about styling and usage of buttons see
  `widgets/button.md`.

Each document is intended to be self‑contained; links between them help
you navigate the crate’s functionality.

The sections above provide a comprehensive starting point; additional
guidance will be added as the crate evolves.
