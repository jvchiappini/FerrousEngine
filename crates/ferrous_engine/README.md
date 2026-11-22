# ferrous_engine

The application shell: window, GPU renderer, assets, ECS scheduler and the
`FerrousApp` trait in one frame loop. See [docs/README.md](docs/README.md) for
the full guide.

## Retained UI is draw-only (prompt 15 phase 4)

`ferrous_gui::UiSystem` and the `ferrous_ui_core` widgets now lay out and paint
only. The runner no longer dispatches pointer or key events to them and
`ferrous_events::EventManager` was removed. `on_click`/`on_change` callbacks do
not fire. Input, gestures, focus, shortcuts, scrolling and text editing live in
**`guimaker::input`**; new UI is built on GUIMaker.

Affected today: `crates/ferrous_editor` (`configure_ui` builds widgets that
draw but do not respond), any external desktop or web app implementing
`FerrousApp::configure_ui`, and web apps linked against
`ferrous_web`/`ferrous_gui` that use the trait. The port of `ferrous_editor` to
GUIMaker is in the Phase 11 backlog; retained-UI input is not being restored.
