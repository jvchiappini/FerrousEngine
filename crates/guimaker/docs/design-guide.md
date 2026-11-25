# Design guide for models

This page is a rule set, not a discussion. Read it before writing a screen.

## Before writing a screen: pick the closest recipe

Do not assemble a screen from scratch when a recipe already matches it. Start
with the recipe and change its content; only then reach for components.

- **Launcher** — recent items plus the new-project form:
  `guimaker::recipe::start_screen(now_ms)` (story `launcher`).
- **New item** — a dialog over a dimmed workspace:
  `guimaker::recipes::new_item_dialog()` (story `new-item-dialog`).
- **Settings** — a sidebar and grouped forms:
  `guimaker::recipes::settings_screen()` (story `settings`).
- **File browser** — a grid of thumbnails:
  `guimaker::recipes::file_browser()` (story `file-browser`).
- **Empty first run** — one empty state with one primary action:
  `guimaker::recipes::empty_first_run()` (story `empty-state`).
- **Editor shell** — toolbar, sidebar, canvas, inspector and status bar:
  `guimaker::recipes::editor_shell()` (story `editor-shell`).
- **Command palette** — a keyboard command search over a dimmed workspace:
  `guimaker::recipes::command_palette_screen()` (story `command-palette`).
- **Toasts after an action** — a stack over the screen that ran it:
  `guimaker::recipes::toasts_after_action()` (story `toasts`).

The recipe index with the full constructors is [recipes.md](recipes.md). If no
recipe fits, compose components; do not invent a layout primitive. The adaptive
switches (collapsing sidebar, toolbar overflow, split panes, dialog-to-sheet,
responsive table) remain in `guimaker::patterns`.

## Hierarchy

1. **One primary action per group.** Exactly one `ButtonVariant::Primary` under
   a parent. Everything else is Secondary, Ghost, Danger or Link. The lint rule
   is `multiple_primary_actions` and it is blocking.
2. **One Display role per screen.** A screen has one `Display` (the window or
   page title), one `Title` per major section, one `Heading` per card or panel.
   `Subheading` is inside a section; `Body` is the default copy; `Label` is for
   controls and table headers; `Caption` is for helper and metadata;
   `Overline` is a short eyebrow, never a sentence; `Mono` and `Code` are for
   literal data.
3. **At most two text tones per block.** Use `Tone::Default` plus one of
   `Tone::Muted` or `Tone::Faint`. Do not mix Muted and Faint in one block, and
   do not use Accent/Status tones for ordinary copy. Status colours are for
   status only.
4. **One idea per block.** If a block needs two headings, it is two blocks. Use
   a section, card or divider to separate them.

## Spacing

Use only the spacing tokens. The scale is `S0`, `S1`, `S2`, `S3`, `S4`, `S5`,
`S6`, `S8`, `S10`, `S12`, `S16`, `S24`.

| Gap | Token | Use |
|---|---|---|
| inside a control | `S2` | icon-to-label, button internals, checkbox-to-label |
| between related items | `S3`-`S4` | label and field, rows in one group, actions in one row |
| between groups | `S6`-`S8` | cards in a column, form groups, block padding |
| between sections | `S12` and up | major page regions, distinct topics |
| page padding | `S6`-`S8` | the root container |

A control's height comes from `ControlSize` (`Sm`, `Md`, `Lg`), not from a
hand-written height. A card's padding is `S4`-`S6`; a section's vertical gap is
`S6`-`S8`. If two values feel close, use the same token rather than two
different ones: the lint reports `inconsistent_radius` and
`inconsistent_elevation`; treat both as defects.

## Cards or lists?

Use a **card** when the items are few (2-6), heterogeneous, and each one is read
or acted on independently: a project, a person, a summary, a media item. A card
has one heading, one short body and at most one action row. Do not put a card
inside a card.

Use a **list** when the items are many, homogeneous and scanned in order:
navigation, search results, queue rows. A list row is `list_item`; a list is
`list`. Use a **table** when the user compares the same columns across rows.

Do not use a card for a single row of text; that is a section or a list item.
Do not use a list for two unrelated blocks; that is a section with two cards.

## The loop

1. Pick the closest recipe.
2. Fill it with components and tokens.
3. `cargo run -p guimaker_gallery -- shot <story> --out out/check.png`.
4. `cargo run -p guimaker_gallery -- lint <story> --sweep`.
5. Fix the structure the finding names; do not silence it.
6. Check 360 px and 1920 px; then compare the golden.

See [screenshots.md](screenshots.md) for the exact commands and tolerance.

## Checklist

- [ ] The closest recipe was used, or there is a written reason none fits.
- [ ] One Primary action under each parent; the primary path is obvious.
- [ ] One Display title; type roles step down by level, not by taste.
- [ ] At most two text tones in any block; status tones only on status.
- [ ] `S2` inside controls, `S3`-`S4` between related items, `S6`-`S8` between
      groups, `S12+` between sections.
- [ ] Every clickable target is at least 24x24 logical pixels.
- [ ] Every icon-only action has a tooltip.
- [ ] Every list row has a stable `.key(...)`.
- [ ] No `SizeSpec::Px`, no off-grid spacing, no empty container.
- [ ] Sibling cards share one radius and one elevation.
- [ ] The screen is clean at 360 px and 1920 px, and `lint --sweep` is clean.
- [ ] The golden difference, if any, is intended and has a reason.
