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

The theme exposes seven token groups, each reachable through `Theme`:

| Field | Type group | What it holds |
|---|---|---|
| `theme.spacing(Spacing::S4)` | `Spacing` | the 4 px scale, `S0`..`S24` |
| `theme.typography` | `TypeRole` | type roles and the two densities |
| `theme.radii` | `Radius` | corner radii, `None`..`Full` |
| `theme.elevation` | `ElevationLevel` | the five shadow levels |
| `theme.borders` | `Borders` | hairline and regular border widths |
| `theme.motion` | `Duration`, `Easing` | durations and easing curves |
| `theme.content_width(ContentWidth::Md)` | `ContentWidth` | max content-column widths |

A `ContentWidth` (`sm` 480, `md` 640, `lg` 800, `xl` 1024, `prose` 672 px) is
the design value a `max_width` layout clamps a content column to, so prose and
forms keep a comfortable measure on wide screens.

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

The 29 roles:

```text
bg                    surface               surface_raised
surface_sunken        overlay               border
border_strong         text                  text_muted
text_faint            text_on_accent        text_on_accent_fill
accent                accent_hover          accent_pressed
accent_fill           accent_fill_hover     accent_fill_pressed
accent_subtle         focus_ring            selection
success               success_subtle        warning
warning_subtle        danger                danger_subtle
info                  info_subtle
```

Read a role with `theme.role(ColorRole::Accent)` or through the field, e.g.
`theme.colors.accent`. Every role can be overridden after construction:
`theme.colors.set(ColorRole::Accent, color)` or `theme.colors.accent = color`.

### Filled actions

`accent`, `accent_hover` and `accent_pressed` stay the roles for focus, links,
selection outlines and accent text. A filled (primary) button instead uses
`accent_fill` / `accent_fill_hover` / `accent_fill_pressed` as its background
with `text_on_accent_fill` (the light neutral) as its text. The guarantee is
>= 4.5:1 in all three states and all three themes. Views do not pick those
roles by hand: they call the single hook `theme.filled_action(AccentState::Rest
| Hover | Pressed)`, which returns the `FilledAction { fill, text }` pair.

## Contrast guarantees

WCAG contrast is tested on all three built-ins and the results are invariants
of the module, not suggestions:

- `text` >= 7:1 against its surface;
- `text_muted` >= 4.5:1;
- `text_faint` >= 3:1, large or secondary text only;
- `text_on_accent` >= 4.5:1 on all three accent states
  (`accent`, `accent_hover`, `accent_pressed`);
- `text_on_accent_fill` >= 4.5:1 on all three filled states
  (`accent_fill`, `accent_fill_hover`, `accent_fill_pressed`);
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

## Contrast table (generated)

`cargo test -p guimaker theme::tests::contrast_table -- --nocapture` prints
every role on every surface it may sit on. The output for the three built-ins
is the contract:

```text
== dark
dark          bg             text 18.35 muted  9.32 faint  4.30 ring 13.92
dark          bg             success         6.88
dark          bg             warning         9.85
dark          bg             danger          5.95
dark          bg             info            6.98
dark          surface        text 17.24 muted  8.76 faint  4.04 ring 13.08
dark          surface        success         6.46
dark          surface        warning         9.25
dark          surface        danger          5.59
dark          surface        info            6.56
dark          surface_raised text 15.88 muted  8.06 faint  3.72 ring 12.04
dark          surface_raised success         5.95
dark          surface_raised warning         8.52
dark          surface_raised danger          5.15
dark          surface_raised info            6.04
dark          surface_sunken text 17.80 muted  9.04 faint  4.17 ring 13.50
dark          surface_sunken success         6.67
dark          surface_sunken warning         9.55
dark          surface_sunken danger          5.77
dark          surface_sunken info            6.77
dark          accent         text_on_accent  6.27
dark          accent_hover   text_on_accent  7.36
dark          accent_pressed text_on_accent  5.32
dark          accent_fill    text_on_accent_fill  6.28
dark          accent_fill_hover text_on_accent_fill  5.27
dark          accent_fill_pressed text_on_accent_fill  7.50
== light
light         bg             text 18.81 muted  6.00 faint  3.95 ring 10.62
light         bg             success         6.15
light         bg             warning         5.59
light         bg             danger          6.63
light         bg             info            6.01
light         surface        text 17.24 muted  5.50 faint  3.62 ring  9.74
light         surface        success         5.64
light         surface        warning         5.12
light         surface        danger          6.08
light         surface        info            5.51
light         surface_raised text 18.81 muted  6.00 faint  3.95 ring 10.62
light         surface_raised success         6.15
light         surface_raised warning         5.59
light         surface_raised danger          6.63
light         surface_raised info            6.01
light         surface_sunken text 15.30 muted  4.88 faint  3.21 ring  8.64
light         surface_sunken success         5.00
light         surface_sunken warning         4.55
light         surface_sunken danger          5.39
light         surface_sunken info            4.89
light         accent         text_on_accent  5.00
light         accent_hover   text_on_accent  5.89
light         accent_pressed text_on_accent  4.60
light         accent_fill    text_on_accent_fill  6.27
light         accent_fill_hover text_on_accent_fill  5.27
light         accent_fill_pressed text_on_accent_fill  7.49
== high_contrast
high_contrast bg             text 20.70 muted 15.36 faint 11.08 ring 18.42
high_contrast bg             success        11.80
high_contrast bg             warning        13.37
high_contrast bg             danger          9.08
high_contrast bg             info           11.15
high_contrast surface        text 20.12 muted 14.92 faint 10.77 ring 17.90
high_contrast surface        success        11.47
high_contrast surface        warning        12.99
high_contrast surface        danger          8.83
high_contrast surface        info           10.83
high_contrast surface_raised text 19.12 muted 14.19 faint 10.23 ring 17.02
high_contrast surface_raised success        10.90
high_contrast surface_raised warning        12.35
high_contrast surface_raised danger          8.39
high_contrast surface_raised info           10.30
high_contrast surface_sunken text 20.46 muted 15.18 faint 10.95 ring 18.21
high_contrast surface_sunken success        11.66
high_contrast surface_sunken warning        13.21
high_contrast surface_sunken danger          8.98
high_contrast surface_sunken info           11.01
high_contrast accent         text_on_accent 10.26
high_contrast accent_hover   text_on_accent 11.80
high_contrast accent_pressed text_on_accent  8.87
high_contrast accent_fill    text_on_accent_fill  6.31
high_contrast accent_fill_hover text_on_accent_fill  5.29
high_contrast accent_fill_pressed text_on_accent_fill  7.53
```

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
