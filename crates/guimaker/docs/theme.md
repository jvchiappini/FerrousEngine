# GUIMaker theme

`guimaker::theme` is the only module in the library with literal design
values. Everything a screen draws — a space, a type size, a radius, a shadow,
a colour — is named here as a token and reached through the `Theme`, never as
a number in view code.

In phase 1 this module is the whole of the public surface: tokens, colour
roles and the built-in themes. There is no view tree, no component and no
layout yet; the view tree and layout arrive in phase 2. This page is a
reference for the module as it stands, not for a UI that does not exist yet.

## Tokens

The theme exposes six token groups, each a field on `Theme`:

| Field | Type group | What it holds |
|---|---|---|
| `theme.spacing(Spacing::S4)` | `Spacing` | the 4 px scale, `S0`..`S24` |
| `theme.typography` | `TypeRole` | type roles and the two densities |
| `theme.radii` | `Radius` | corner radii, `None`..`Full` |
| `theme.elevation` | `ElevationLevel` | the five shadow levels |
| `theme.borders` | `Borders` | hairline and regular border widths |
| `theme.motion` | `Duration`, `Easing` | durations and easing curves |

### Spacing

The spacing scale is on the 4 px grid: `S0`, `S1`, `S2`, `S3`, `S4`, `S5`,
`S6`, `S8`, `S10`, `S12`, `S16`, `S24`. The value of each token is a multiple
of 4 px that the layout layer snaps to the physical grid. `theme.spacing(Spacing::S4)`
returns the resolved pixel value for that token.

### Type roles and densities

The type roles are Display, Title, Heading, Subheading, Body, BodyStrong,
Label, Caption, Overline, Mono and Code. Each role resolves to a family, a
weight, a size and a line height through `theme.typography.get(TypeRole::Body)`.

Two densities are defined. `Density::Compact` tightens the vertical rhythm for
editors that pack many controls per unit of area; `Density::Comfortable` opens
it for launchers and settings. Density changes the type scale only: the colour
roles, radii, borders, elevation and motion are density-independent, so the
same view works in both.


Families and weights are fixed by G-0002: Inter at 400/500/600/700 for the
sans roles, JetBrains Mono at 400/500 for Mono and Code.

### Radius

`Radius::None`, `Xs`, `Sm`, `Md`, `Lg`, `Xl` and `Full`. The scale gives small
controls, fields, cards and pills a consistent curve; `Full` is the pill
radius used by fully rounded controls.

### Elevation

`ElevationLevel` runs 0 to 4. Each level above zero is drawn as two soft
shadows — a tight contact shadow and a wider ambient one — so a raised
surface reads as a surface, not as a drop shadow. In dark themes the level
also blends a surface tint, because a shadow alone is invisible against a
dark background.

### Borders

Two border widths: hairline and regular. Hairline is for dividers inside a
surface; regular is for the edge of an interactive control.

### Motion

Durations are `Instant`, `Fast` (90 ms), `Base` (140 ms) and `Slow` (240 ms).
Easings are linear, standard, emphasized and overshoot. A transition names
both: `Duration::Base` with `Easing::Standard` for most state changes.

## Colour

There is one `Color` type. Its constructors name the colour space they take,
so a value cannot be read as sRGB when it is linear:

```text
Color::srgb(r, g, b)          Color::srgba(r, g, b, a)
Color::linear(r, g, b)        Color::lineara(r, g, b, a)
Color::oklch(l, c, h)         Color::oklcha(l, c, h, a)
Color::hex(0xRRGGBB)          Color::hex_alpha(0xRRGGBBAA)
```

Conversion between spaces happens once, inside the type. A double-encoded
sRGB value — the D-0021 bug — is unrepresentable because there is no path
that applies the sRGB transfer function twice.

Roles are generated from a `ColorSeed`. A seed carries a neutral hue and
chroma plus an accent hue and chroma. `ColorSeed::new(neutral_hue,
neutral_chroma, accent_hue, accent_chroma)` builds a custom one;
`ColorSeed::dark()` and `ColorSeed::light()` are the built-in seeds. From the
seed the theme generates an OKLCH neutral ramp and the accent states.

The 25 roles:

```text
bg                surface           surface_raised     surface_sunken
overlay           border            border_strong      text
text_muted        text_faint        text_on_accent     accent
accent_hover      accent_pressed    accent_subtle      focus_ring
selection         success           success_subtle     warning
warning_subtle    danger            danger_subtle      info
info_subtle
```

Read a role with `theme.role(ColorRole::Accent)` or through the field, e.g.
`theme.colors.accent`. Every role can be overridden after construction:
`theme.colors.set(ColorRole::Accent, color)` or `theme.colors.accent = color`.

## Contrast guarantees

WCAG contrast is tested on all three built-ins and the results are invariants
of the module, not suggestions:

- `text` >= 7:1 against its surface;
- `text_muted` >= 4.5:1;
- `text_faint` >= 3:1, large or secondary text only;
- `text_on_accent` >= 4.5:1 on all three accent states
  (`accent`, `accent_hover`, `accent_pressed`);
- `focus_ring` >= 3:1 against the four surfaces.

The focus ring is the hard case. It is drawn with a surface-coloured gap —
an outline offset — so the ring never touches an accent fill. One opaque ring
colour cannot be >= 3:1 against both the dark surfaces and a mid-bright
accent. Take a dark surface at relative luminance 0.01 and an accent at 0.25.
Contrast is `(L1 + 0.05) / (L2 + 0.05)`. Against the surface the ring needs
`(R + 0.05) / 0.06 >= 3`, so `R >= 0.13`. Against the accent it needs
`0.30 / (R + 0.05) >= 3`, so `R <= 0.05`. Both cannot hold, so the gap is the
only correct answer: the surface colour separates the ring from the fill, and
the ring is only ever measured against the surface it covers.

## Built-in themes and seeds

```text
Theme::dark()
Theme::light()
Theme::high_contrast()
Theme::from_seed(seed, mode, density)
```

`Theme::from_seed` takes a `ColorSeed`, a `ThemeMode` (dark or light) and a
`Density`, and returns a full theme with an app's own accent. Dark and light
share that generator; `Theme::high_contrast()` calls
`Theme::high_contrast_with_seed` and its own high-contrast neutral ramp. A
custom theme and a built-in share the same role builder.

## Theme files

A theme serializes to RON with `Theme::to_ron()` and deserializes with
`Theme::from_ron()`. Files are `*.theme.ron`; the designer edits them by hand.
Colours travel as 8-bit hex, so a loaded theme re-serialises to the same
canonical text (the round-trip is idempotent to the file, not bit-identical in
floating point). The format, and why it is RON, is G-0003.

## Example

```rust
use guimaker::theme::{Spacing, Theme, TypeRole};

let theme = Theme::dark();
let pad = theme.spacing(Spacing::S4);
let body = theme.typography.get(TypeRole::Body);
```

That is the whole of phase 1: a theme you can read tokens from. The view tree
and layout that consume these tokens arrive in phase 2.
