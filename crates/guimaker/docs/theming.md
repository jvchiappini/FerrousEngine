# Theming

A theme is the whole visual contract in one value: tokens, 29 colour roles, type
roles, densities, breakpoints and the reduced-motion flag. Views ask for a role
or a token; they never name a colour or a pixel.

The reference for every field and every contrast number is [theme.md](theme.md).
This page is the short version: what to choose, what is guaranteed, and how to
override without breaking the guarantees.

## Tokens and roles

| Group | Examples | Rule |
|---|---|---|
| spacing | `Spacing::S0`..`S24` | 4 px grid; `theme.spacing(Spacing::S4)` resolves it |
| type | `TypeRole::Display`..`Code` | one family, weight, size and line height per role |
| radius | `Radius::None`..`Full` | small controls, fields, cards and pills share one curve |
| elevation | `ElevationLevel` 0..4 | contact + ambient shadow; dark themes also tint the surface |
| borders | hairline, regular | dividers inside a surface vs the edge of a control |
| motion | `Instant`, `Fast`, `Base`, `Slow` | duration plus easing; reduced motion collapses to `Instant` |
| width | `ContentWidth::Sm`..`Prose` | a reading or form column stops growing at its cap |

Colours are roles, not literals: `bg`, `surface`, `surface_raised`,
`surface_sunken`, `text`, `text_muted`, `text_faint`, `border`, `accent`,
`accent_fill`, the three `accent_fill_*` states, the semantic `success`,
`warning`, `danger`, `info` and their subtle washes, plus `focus_ring`,
`selection` and the two on-accent text roles. Read one with
`theme.role(ColorRole::Accent)` or `theme.colors.accent`.

A filled primary action is the one place where a view must not pick roles by
hand: call `theme.filled_action(AccentState::Rest | Hover | Pressed)` and use
the returned `FilledAction { fill, text }`. That hook is what keeps the
on-accent pairing valid in every theme.

## Contrast guarantees

These are invariants of the module, tested on all three built-ins:

- `text` has at least 7:1 against its surface;
- `text_muted` and the on-accent roles have at least 4.5:1;
- `text_faint` and the focus ring have at least 3:1;
- `focus_ring` is at least 3:1 against each of the four surfaces, with a
  surface-coloured gap so it never touches an accent fill.

The full table, per theme and per surface, is printed by
`cargo test -p guimaker theme::tests::contrast_table -- --nocapture` and is
reproduced in [theme.md](theme.md).

## Density and modes

`Density::Compact` is what the three built-in themes ship today.
`Density::Comfortable` opens the vertical rhythm for launchers and settings.
Density changes the type scale only: roles, radii, borders, elevation and motion
are independent, so one view works in both. `Theme::with_density(density)`
switches it.

The three built-in modes are `Theme::dark()`, `Theme::light()` and
`Theme::high_contrast()`. High contrast is a dark variant with its own neutral
ramp and compact density. `Theme::from_seed(seed, mode, density)` builds a
custom theme from a neutral and an accent hue; dark and light share the
generator.

## Overriding

Override a role after construction, then keep every view reading roles:

```rust
use guimaker::prelude::*;

let mut theme = Theme::dark();
theme.colors.accent = Color::oklch(0.68, 0.16, 250.0);
let accent = theme.colors.accent;
theme.colors.set(ColorRole::FocusRing, accent);
assert!(theme.colors.accent == theme.role(ColorRole::Accent));
```

An override is allowed to fail the contrast guarantees; the shipped themes and
`Theme::from_seed` are the tested configurations. If an override must ship,
run the lint on the resulting screens: `low_contrast` is blocking. The theme
file format, RON round-trip and why RON is G-0003; the fixed families and
weights are G-0002. Use [theme.md](theme.md) for serialisation and the exact
`Theme` methods.
