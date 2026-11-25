# Concepts

GUIMaker is a declarative UI library: the app says what the screen *is*, and the
library decides where it goes. Four ideas hold the whole model together.

## One tree per frame

A screen is an `Element<Msg>` tree built from `view::text`, the `components`
constructors and the token-taking modifiers. `Ui::paint` is a pure function of
that tree, the keyed store, the theme, the viewport, the scale and this frame's
input. It runs five steps:

1. rebuild the arena when the view hash changed;
2. lay out on taffy, memoised by a content hash;
3. route queued input through the laid-out tree;
4. paint a flat `DisplayList`, reusing unchanged nodes' commands;
5. return messages and the damage list.

A still frame reuses the previous display list and allocates nothing. The same
inputs produce a byte-identical list, which is what makes screenshots
reproducible. See [layout.md](layout.md) for the tree, the layout pass, inspect,
explain and debug data.

## Keys

Interaction state lives in a store keyed by a stable id path. Explicit
`.key("name")` names a node; list rows must have one so a reorder cannot move a
hover or a caret to another row. Derived paths cover the rest. A key is not a
style: it is the identity that lets hover, press, focus, scroll and text-edit
state survive a rebuild.

## Messages

The tree is built as `Ui<Msg>`. An action carries a `Msg`; the next frame
returns it in `Frame::messages` (Elm style). A host feeds each message back to
the model's `update`, then rebuilds the view. There are no callbacks that
capture app state. `InputEvent` is the platform layer: a host translates
pointer, wheel, key, text, modifier and focus events and calls
`Ui::push_event`; `paint` routes them. See [input.md](input.md).

## Tokens

Every design value is a token in `guimaker::theme`: spacing, type roles, radii,
elevation, border widths, motion, content widths and the 29 colour roles.
Styling APIs take tokens (`pad(Spacing::S4)`, `text_role(TypeRole::Body)`,
`tone(Tone::Muted)`, `bg(ColorRole::Surface)`), never numbers. `SizeSpec::Px` is
the only explicit escape hatch and the design lint reports it.

The theme is data: it serialises to RON, so a custom theme and a built-in one
run the same code. See [theme.md](theme.md) for the reference and
[theming.md](theming.md) for how to choose and override roles.

## The display list

Paint emits engine-agnostic commands: rounded rects, borders, shadows,
gradients, text runs, images, icons, clips, layers and a backdrop-blur request.
Every command carries its `NodeId`, so a host can correlate a draw with the
view tree. `guimaker_ferrous` is the only crate that knows how to draw; a host
passes the list to `encode` or `GuiMakerRenderer::encode` with the engine's
`GuiBatch` and `Font`. See [backend.md](backend.md).
