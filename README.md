# FerrousEngine

**FerrousEngine** is a modern, highly modular game engine constructed from the ground up in Rust. It utilizes a strict multi-crate architecture to ensure that the application lifecycle, rendering, game logic, UI, and asset management remain decoupled. FerrousEngine is perfect as a foundation for lightweight games, custom 3D editors, compute-shader toy environments (like raymarching), and native desktop tooling.

## Architecture & Multi-Crate Structure

FerrousEngine is split into specialized crates, each with an exact, tightly-scoped domain:

* **`ferrous_app`**: The application shell and runtime orchestration. It exposes the engine builder pattern (`App::new().run()`) and handles the main event loop cleanly over `winit`. Users implement the `FerrousApp` trait (`setup`, `update`, `draw_ui`, `draw_3d`) to interact seamlessly with the frame lifecycle.
* **`ferrous_core`**: The foundational headless layer. It defines 3D transforms (`Transform`), color utilities (`Color`), timing helpers (`Time`, metrics), keyboard/mouse states (`InputState`), and the logical entity container (`World`). It relies heavily on `glam` for math and has **zero GPU or windowing dependencies**.
* **`ferrous_renderer`**: The graphics and Hardware Abstraction Layer built entirely on `wgpu`. It implements an advanced **Render Graph** based on the `RenderPass` trait using a two-phase `prepare -> execute` structure. Out of the box, it supports configurable 3D Geometry (`WorldPass`), 2D GUI overlays (`UiPass`), off-screen MSAA Render Targets, and fully independent `ComputePass` capabilities para GPU compute/raymarching tasks.
* **`ferrous_gui`**: A hybrid immediate/retained-mode 2D GUI framework. It supports an extensive suite of widgets including Interactive Buttons, Sliders, Text Inputs, Color Pickers, and Containers. Also supports flex-like declarative layouts (`Row`, `Column`) and custom engine widgets like the `ViewportWidget` (which embeds the 3D `ferrous_renderer` scene directly into the GUI).
* **`ferrous_assets`**: The asset loading and caching system. Currently optimized for TTF/OTF font rasterization and caching, ensuring text rendering across the engine is performant and reliable.
* **`ferrous_editor`**: The reference tool built *with* FerrousEngine. It serves as an active sandbox that consumes all the above crates, demonstrating how to bootstrap an application that displays a 3D interactive viewport embedded inside a tool-oriented GUI.

## Documentation

Every major crate within `FerrousEngine` includes its own dedicated `docs/` folder. This ensures that the documentation for the renderer stays with the renderer, and the logic docs stay with the core. 

The documentation website unifies all of these markdown files and is generated with [MkDocs](https://www.mkdocs.org/) using the Material theme. All markdown files from each crate's `docs/` folder are aggregated into the root `docs/` folder via `scripts/build_docs.sh` and then built into `site/`.



For detailed architectural notes, custom pipeline extending, and API references, browse the individual `docs/` directories inside the sub-crates.
