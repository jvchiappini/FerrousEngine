# GUIMaker — the UI library of the FerrousEngine ecosystem

GUIMaker turns structure into a screen. A view is a tree built from tokens —
spacing, type roles, radii, elevation, colour roles, motion, density — laid out
by the library, routed by the library, and drawn by FerrousEngine. The app
writes what the screen *is*; it does not compute where anything goes.

```text
crates/guimaker            tokens, view tree, layout, input, controllers, display list, lint
crates/guimaker_ferrous    display list -> GuiBatch, text via ferrous_font, host-free mapping
crates/guimaker_gallery    stories, screenshots, goldens, the FerrousApp host adapter
crates/guimaker_designer   the visual designer (planned, not in the workspace yet)
```

## Documentation map

- [Getting started](getting-started.md) — a window with a button in under 30
  lines, built on the gallery host.
- [Concepts](concepts.md) — the tree per frame, keys, messages and tokens.
- [Components](components.md) — every component, grouped, with its story.
- [Recipes](recipes.md) — the composed screens to start from.
- [Design guide for models](design-guide.md) — the directive rules to follow
  before writing a screen.
- [Theming](theming.md) — tokens, roles, contrast guarantees, density and
  overrides; the full reference is [theme.md](theme.md).
- [Backend contract](backend.md) — what a host implements, the `GuiBatch`
  mapping, popups, DPI and input; the detailed mapping is
  [guimaker_ferrous/backend.md](../guimaker_ferrous/backend.md).
- [Screenshots and the lint loop](screenshots.md) — `shot`, `golden`, `lint`,
  tolerance and the update log.
- [Roadmap](roadmap.md) — what is done, in progress and not started.

The reference pages are [layout.md](layout.md), [responsive.md](responsive.md),
[input.md](input.md), [text.md](text.md) and [prewarming.md](prewarming.md).
Decisions live in [decisions/README.md](decisions/README.md); start with G-0001
(`decisions/G-0001.md`) for the crate layout and the rebuild model.

## Status

Phases 0 through 8 are implemented: the theme, the view tree and layout,
responsive values, input and controllers, the backend and gallery, the
components, motion and overlays, the design lint and diagnostics, and the
engine popup and font work. Phase 9 is the documentation and model-facing design
pass; the visual designer and the Motioner migration are not started. See
[roadmap.md](roadmap.md) for the exact stage and the open work.

Rules that hold from the first commit:

- the engine draws, GUIMaker decides; the core names no engine drawing type;
- no engine crate may depend on a `guimaker*` crate;
- no literal design value outside `guimaker::theme`; `Px` is the only
  explicit escape hatch and the lint reports it;
- the literal guard (`scripts/check_guimaker_literals.py`) fails the gate
  when a GUIMaker crate calls a literal `Color` constructor or passes a raw
  pixel number to a spacing/radius API outside `guimaker::theme`; tests and
  the theme are exempt;
- layout and paint depend only on the view, theme, viewport, scale and the
  injected clock, so the same inputs give a byte-identical screenshot;
- popups and popup pre-warming are opt-in; the library never opens a window on
  its own.
