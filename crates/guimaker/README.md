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

## Verify

```text
python scripts/check_encoding.py
python -m unittest discover -s scripts/tests
cargo test -p guimaker
cargo test -p guimaker_ferrous
cargo test -p guimaker_gallery
```
