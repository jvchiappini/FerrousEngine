# GUIMaker — the UI library of the FerrousEngine ecosystem

GUIMaker turns structure into a screen. A view is a tree built from tokens —
spacing, type roles, radii, elevation, colour roles, motion, density — laid
out by the library, routed by the library, and drawn by FerrousEngine. The
app writes what the screen *is*; it does not compute where anything goes.

```text
crates/guimaker            tokens, view tree, layout, input, controllers, display list, lint
crates/guimaker_ferrous    display list -> GuiBatch, text via ferrous_font, host adapter
crates/guimaker_gallery    storybook, screenshots, benchmarks
crates/guimaker_designer   the visual designer
```

Start with G-0001 (`decisions/G-0001.md`): crate layout, the rebuild model,
the text trait, the draw-command contract, the token boundary, the damage
contract and the dependency rule. The theme is the first concrete module:
`theme.md` is its reference, and G-0002 (`decisions/G-0002.md`) fixes the
typefaces while G-0003 (`decisions/G-0003.md`) fixes the theme file format.
The documentation set (getting started,
concepts, components, theming, backend contract, screenshots and lint, and
the model-facing design guide) is written in phase 9; this file is the entry
point until then.

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
  injected clock, so the same inputs give a byte-identical screenshot.
