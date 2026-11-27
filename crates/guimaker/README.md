# GUIMaker

GUIMaker is the UI library of the FerrousEngine ecosystem. A screen is a tree
built from design tokens; the library lays it out, routes input and emits a
flat display list that the FerrousEngine backend draws.

The family is four crates in the plan. Three are in this workspace today:

| Crate | Contains |
|---|---|
| `crates/guimaker` | tokens and themes, view tree, layout, input and controllers, display list, lint, inspect |
| `crates/guimaker_ferrous` | display list -> `GuiBatch`, text through `ferrous_font`, textures and icons |
| `crates/guimaker_gallery` | stories, headless screenshots, tolerance goldens and the `FerrousApp` host adapter |

The fourth, `crates/guimaker_designer`, is the visual designer planned for a
later phase and is not in the workspace yet.

## Start here

```text
cargo run -p guimaker_gallery --example host     # a real window with the host adapter
cargo run -p guimaker_gallery -- stories         # every story name
cargo run -p guimaker_gallery -- shot start --size 1280x800 --scale 1.5 --theme light
```

The documentation set lives in `crates/guimaker/docs/`:

| Page | For |
|---|---|
| `docs/getting-started.md` | a window with a button in under 30 lines |
| `docs/concepts.md` | tree per frame, keys, messages, tokens |
| `docs/components.md` | every component and its gallery story |
| `docs/recipes.md` | the composed screens to start from |
| `docs/design-guide.md` | the directive rules a model follows before writing a screen |
| `docs/theming.md` | tokens, roles, contrast, density, overrides |
| `docs/backend.md` | what a host implements, `GuiBatch`, popups, DPI, input |
| `docs/screenshots.md` | screenshot and lint loop, tolerance and update log |
| `docs/roadmap.md` | what is done and what is not |

The detailed references are `docs/theme.md`, `docs/layout.md`,
`docs/responsive.md`, `docs/input.md`, `docs/text.md` and
`docs/prewarming.md`; decisions are in `docs/decisions/`, and the backend
mapping is `crates/guimaker_ferrous/docs/backend.md`.

## Rules that hold

- the engine draws, GUIMaker decides; the core names no engine drawing type;
- no engine crate may depend on a `guimaker*` crate;
- no literal design value outside `guimaker::theme`; `SizeSpec::Px` is the
  explicit escape hatch and the lint reports it;
- layout and paint depend only on the view, theme, viewport, scale and the
  injected clock, so the same inputs give a byte-identical screenshot;
- popups and popup pre-warming are opt-in; the library never opens a window on
  its own.

## Choosing a backend

The engine picks a per-OS default (Vulkan first on Windows today; the measured
choice is `G-0013`). A host can override it without touching GUIMaker:

```rust
use ferrous_engine::{App, Backend};

App::new(GuiHost::new(app))
    .with_backend(Backend::Dx12)          // one backend
    // .with_backends([wgpu::Backends::GL, wgpu::Backends::VULKAN])  // ordered fallback
    .run();
```

`WGPU_BACKEND=vulkan|dx12|gl|metal|webgpu` wins at run time, so a shipped
choice is overridable without a rebuild. Every compiled backend runs the same
view; a machine without an adapter for the requested backend stops with a named
`no usable GPU backend` error (exit 2), never a panic or a silent fallback.

DX12 uses FXC by default. `AppConfig::dx12_compiler` / `App::with_dx12_compiler`
select DXC from code and `FERROUS_DX12_COMPILER=fxc|dxc|dynamic_dxc` at run
time; DXC needs `dxcompiler.dll`/`dxil.dll` and the `dxc` cargo feature. See
`crates/ferrous_engine/docs/startup.md#choosing-a-backend-from-code`.

GL needs the non-bindless GUI image path (Phase 9D Lane A) before it can run a
GUIMaker view.

## Verify

```text
python scripts/check_encoding.py
python -m unittest discover -s scripts/tests
cargo test -p guimaker
cargo test -p guimaker_ferrous
cargo test -p guimaker_gallery
```

<!-- PHASE9D-PERFORMANCE:START -->

## Performance (Phase 9D)

Not measured yet; run `python bench/guimaker_compare/run_compare.py --confirmed-host-free --runs 5 --scenario idle6 --scenario idle30 --scenario hover_body --scenario hover_button --scenario typing --scenario resize --scenario menu --tier vulkan --tier dx12 --tier gl --interactive-tier vulkan --write-readme` (see [PROTOCOL.md](../../bench/guimaker_compare/PROTOCOL.md)) to fill this section.

<!-- PHASE9D-PERFORMANCE:END -->
