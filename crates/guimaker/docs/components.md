# Components

Every component is a pure function of its data and its message: it returns an
`Element<Msg>` and never reads interaction state. Hover, press, focus, selection
and disabled are resolved by the paint layer from the keyed store. The sources
are `components/actions.rs`, `components/inputs.rs`,
`components/display.rs`, `components/containers.rs` and
`components/overlays.rs`; `components/mod.rs:30` defines the three
`ControlSize` values.

The **story** column is the name the gallery CLI accepts for that component
(`cargo run -p guimaker_gallery -- shot <story>`). A component with no separate
story is exercised inside the story named in the row.

Two names are shared with Phase 9 recipes: the CLI `empty-state` and
`command-palette` open the recipe screens. The component variants keep their own
component-story goldens (`empty-state`, `command-palette`); the recipe goldens
use the `-screen` suffix. Use `Component::parse` or the golden suite to reach
the component variants.

## Actions

| Component | Story | Use it for |
|---|---|---|
| `button` | `button` | The one action a block exists to perform; variants are Primary, Secondary, Ghost, Danger and Link. |
| `button_with` | `button` | A button with a leading icon, a size or a loading state. |
| `icon_button` | `icon-button` | A compact toolbar action; the label is the accessible name and the tooltip. |
| `toggle_button` | `toggle-button` | An on/off formatting or mode action such as Bold. |
| `button_group` | `button-group` | A segmented single-choice control for two to five short options. |
| `split_button` | `split-button` | A primary action with an attached menu of related actions. |

## Inputs

| Component | Story | Use it for |
|---|---|---|
| `text_field` | `text-field` | One line of text with a label, helper, error, icons and a clear action. |
| `number_field` | `number-field` | A numeric value with steppers and drag-to-change. |
| `textarea` | `textarea` | Multi-line text such as notes or a description. |
| `checkbox` | `checkbox` | An independent boolean that applies immediately. |
| `radio_group` | `radio-group` | One choice out of a short, mutually exclusive list. |
| `switch` | `switch` | A setting that takes effect immediately without a Save action. |
| `slider` | `slider` | One value in a bounded range where approximate input is fine. |
| `range_slider` | `range-slider` | A lower and upper bound in one control. |
| `select` | `select` | One choice from a list that is short enough to browse. |
| `combobox` | `combobox` | A searchable select whose list is long or filterable. |
| `color_swatch` | `color-swatch` | A named colour choice with a visible swatch. |
| `path_field` | `path-field` | A file or folder path with a Browse action. |
| `form_row` | `form-row` | A labelled field that stacks below the breakpoint. |

## Display

| Component | Story | Use it for |
|---|---|---|
| `components::text` | `text` | A run of text at one type role and one tone. |
| `paragraph` | `text` | Prose clamped to a line count with an end ellipsis. |
| `heading` | `heading` | Level 1-3 page, section or block title, with an optional overline. |
| `badge` | `badge` | A small status or count pill. |
| `tag` | `tag` | A removable filter, label or category chip. |
| `kbd` | `kbd` | One key cap in a shortcut hint. |
| `kbd_combo` | `kbd` | A full chord such as Ctrl+Shift+K. |
| `avatar` | `avatar` | One person or entity identity mark. |
| `avatar_group` | `avatar` | An overlapping team or participant list. |
| `thumbnail` | `thumbnail` | A media preview with an optional badge. |
| `progress` | `progress` | Determinate work on a horizontal track. |
| `progress_ring` | `progress-ring` | Determinate work where a compact ring fits better. |
| `spinner` | `spinner` | Indeterminate work with no known progress. |
| `skeleton` | `skeleton` | A placeholder that holds the shape of loading content. |
| `empty_state` | component golden (CLI `empty-state` is the recipe) | The guidance shown when a view has no content. |
| `callout` | `callout` | An inline status message with an optional action. |

## Containers

| Component | Story | Use it for |
|---|---|---|
| `card` | `card` | A raised surface that groups one subject. |
| `list_item` | `list-item` | One selectable row with icon, title, subtitle and trailing content. |
| `list` | `list` | A vertical collection with selection and keyboard navigation. |
| `table` / `table_view` | `table` | Header plus rows, sortable and virtualised for long data. |
| `tabs` | `tabs` | Peer views that share one region and one context. |
| `accordion` | `accordion` | A collapsible section in a long settings or details page. |
| `section` | `section` | A titled block with a capped content column. |
| `nav_item` | `sidebar` | One navigation row inside a sidebar. |
| `sidebar` | `sidebar` | The primary navigation column of an app shell. |
| `toolbar` | `toolbar` | A horizontal strip of actions above the content. |
| `status_bar` | `status-bar` | A thin row of state and counts at the bottom of the window. |
| `app_shell` | `app-shell` | The outer frame that arranges title, tools, navigation, body and status. |

## Overlays

| Component | Story | Use it for |
|---|---|---|
| `layer_stack` | `layer-stack` | The z-ordered root that hosts stacked overlay layers. |
| `trap` | `dialog` | A focus trap around a modal subtree; the dialog uses it. |
| `tooltip` | `tooltip` | A short hint on hover or keyboard focus. |
| `popover` | `popover` | An anchored panel for a small set of controls or details. |
| `menu` | `menu` | A vertical list of commands, including a context menu. |
| `context_menu` | `menu` | The same menu opened at a pointer position. |
| `dialog` | `dialog` | A modal decision or a blocking form. |
| `command_palette` | component golden (CLI `command-palette` is the recipe) | Keyboard-first search over commands. |
| `toast` | `toast` | A transient notification that does not block. |
| `toast_stack` | `toast` | A column of toasts with one dismissal handler. |

## Foundations

| Component | Story | Use it for |
|---|---|---|
| `icons` module / `Icon` enum | `icons` | The vector icon set every action uses. |
| `icon_box` | `icons` | A sized, tone-aware icon inside a control; `label_text` and `icon_surface` are its siblings. |
| `control sizes` (`Sm`, `Md`, `Lg`) | `button` | The three heights, paddings and label roles a control can take. |

For composed screens, use [recipes.md](recipes.md) rather than assembling
these primitives from scratch.
