# Roadmap

This page records what exists, not what is hoped for. A phase is done only when
its tests and its documentation are in the repository.

## Done

| Phase | What landed |
|---|---|
| 0 | G-0001 crate layout, the executable boundary rule, the docs section |
| 1 | `theme`: one linear `Color`, OKLCH seeds, 29 roles, dark/light/high-contrast, WCAG tests, RON files (G-0002, G-0003) |
| 2 | view tree in an arena, token styling, taffy 0.7.7 layout memoised by hash, display list with node ids, pointer routing, inspect/explain/debug |
| 3 | container breakpoints, `resp!` values, fluid clamps, auto-fill grids, `priority`, adaptive patterns, the 40-case responsive sweep |
| 4 | gestures, cached routing, focus scopes, shortcuts, momentum scroll, O(1) virtual list, tree navigation, grapheme-correct text editor (G-0004) |
| 5 | `guimaker_ferrous` mapping and text, the gallery CLI, the `FerrousApp` host adapter, tolerance goldens |
| 6 | 50 component stories and the component library: actions, inputs, display, containers, overlays |
| 7 | motion machinery, anchored overlays, table tracks, backdrop source, icon tone and size |
| 8 | the design lint and diagnostics; engine font fallback and multi-face atlas; real popup windows, popup correctness and cost, the layered present backend (G-0010) |

## In progress

**Phase 9 — documentation and the model-facing design pass.** The documentation
set in `crates/guimaker/docs/` is being completed: getting started, concepts,
components, theming, the backend contract, the screenshot and lint loop, the
design guide and this roadmap. The eight screen recipes, their gallery stories,
their goldens and `recipes.md` are in the tree. The recipe lane adds the
`recipes` module; the documentation lane changes no code.

Open from earlier phases and recorded rather than hidden:

- **9B performance revisit.** The responsive cold frame pays a second taffy pass
  (0.497 ms on the start screen vs 0.300 ms before Phase 3). It is inside the
  Phase 3 acceptance and is only revisited if the first-frame budget needs the
  0.2 ms back; the candidates are recorded in
  [responsive.md](responsive.md).
- **Popup pre-warming.** It stays opt-in: about +10.6 MB of idle RSS and 69.8 ms
  of idle work for a 23x lower first-open latency, with the pool capped at two.
  The measurement and the interaction with the pool are in
  [prewarming.md](prewarming.md).
- **Engine gaps.** Rounded clips, gradient borders, arbitrary SVG paths and a
  real off-screen opacity layer remain documented fallbacks in
  [guimaker_ferrous/backend.md](../guimaker_ferrous/backend.md). The GUIMaker
  API does not change when they land.

## Not started

- **Phase 10 — Motioner migration.** Motioner takes `guimaker` and
  `guimaker_ferrous` through its engine submodule; one engine copy, no patch.
- **Phase 11 — visual designer.** `crates/guimaker_designer` is the planned
  visual editor. It is not in the workspace, and no page here describes it as
  available.

## How a phase closes

A phase closes only when its tests pass, its `file:line` references resolve,
its architecture page and decisions are current, and its commands in this
documentation set have been run on the committed tree.
