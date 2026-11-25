//! Overlay components: layers, tooltips, popovers, menus, dialogs, palettes and
//! toasts.
//!
//! Overlays are pure trees like every other component: the host composes them
//! over a base view with [`layer_stack`] and owns the open/closed state. A modal
//! is made modal by the host pushing a focus scope and an overlay path:
//! `ui.input_mut().push_focus_scope(FocusScope { root, trap: true })` and
//! `ui.input_mut().push_overlay(path)` when it opens, then
//! `ui.input_mut().pop_focus_scope()` and `ui.input_mut().pop_overlay()` when it
//! closes. `route` already handles `Esc` while an overlay is open, and it paints
//! the keyboard focus ring on every interactive row, so focus-visible needs no
//! per-row flag.
//!
//! ```
//! use guimaker::prelude::*;
//! let layer: Element<()> = trap(dialog("Rename", text("Layer"), [button("OK", ButtonVariant::Primary, ())], DialogSize::Sm));
//! assert!(matches!(layer.key, Some(Key::Static("overlay"))));
//! ```

use crate::components::{
    icon_box, icon_button, label_text, ButtonVariant, ControlSize, StateSpec, Status,
};
use crate::icons::Icon;
use crate::style::{Align, Cursor, Justify, SizeSpec, Tone};
use crate::text::Ellipsis;
use crate::theme::{ColorRole, ContentWidth, Duration, ElevationLevel, Radius, Spacing, TypeRole};
use crate::view::{text, Element};

const HOVER_INTENT_MS: f64 = 600.0;

/// The side an overlay opens on.
///
/// ```
/// use guimaker::prelude::*;
/// let placement = Placement::Bottom.flipped(false);
/// assert_eq!(placement, Placement::Top);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Placement {
    /// Above the anchor.
    Top,
    /// Below the anchor.
    #[default]
    Bottom,
    /// To the left of the anchor.
    Left,
    /// To the right of the anchor.
    Right,
}

impl Placement {
    /// Chooses the side that has room.
    ///
    /// The vertical (`Top`/`Bottom`) and horizontal (`Left`/`Right`) axes are
    /// preserved: a `Bottom` placement becomes `Top` when there is no space
    /// below, and a `Right` placement becomes `Left` when there is no space to
    /// the right.
    ///
    /// ```
    /// use guimaker::prelude::*;
    /// assert_eq!(Placement::Bottom.flipped(false), Placement::Top);
    /// assert_eq!(Placement::Right.flipped(true), Placement::Right);
    /// ```
    #[must_use]
    pub const fn flipped(self, space_below: bool) -> Self {
        match self {
            Self::Top | Self::Bottom => {
                if space_below {
                    Self::Bottom
                } else {
                    Self::Top
                }
            }
            Self::Left | Self::Right => {
                if space_below {
                    Self::Right
                } else {
                    Self::Left
                }
            }
        }
    }
}

/// Marks a layer as the active overlay and keys it `"overlay"`.
///
/// The key is the stable path the host passes to
/// `ui.input_mut().push_overlay(path)` after the tree is built. While an overlay
/// path is pushed, `Esc` closes it in `route`; the host then calls
/// `ui.input_mut().pop_focus_scope()` and clears its modal state.
///
/// ```
/// use guimaker::prelude::*;
/// let layer: Element<()> = trap(Element::column());
/// assert!(matches!(layer.key, Some(Key::Static("overlay"))));
/// ```
#[must_use]
pub fn trap<Msg: Clone>(element: Element<Msg>) -> Element<Msg> {
    element.key("overlay")
}

/// Stacks a base view and every overlay layer over it, in paint order.
///
/// ```
/// use guimaker::prelude::*;
/// let view: Element<()> = layer_stack(Element::column(), [Element::row()]);
/// assert_eq!(view.kind, NodeKind::Stack);
/// assert_eq!(view.children.len(), 2);
/// ```
#[must_use]
pub fn layer_stack<Msg: Clone>(
    base: Element<Msg>,
    layers: impl IntoIterator<Item = Element<Msg>>,
) -> Element<Msg> {
    let mut stack = Element::stack().child(base);
    for layer in layers {
        // A stack stretches every child to its own box, so wrap a layer in a
        // full-size, top-left anchor to keep the panel at its natural size.
        let anchored = Element::column()
            .width(SizeSpec::Fill)
            .height(SizeSpec::Fill)
            .align(Align::Start)
            .justify(Justify::Start)
            .child(layer);
        stack = stack.child(anchored);
    }
    stack
}

/// A reusable collection of overlay layers over a base view.
///
/// ```
/// use guimaker::prelude::*;
/// let mut stack: LayerStack<()> = LayerStack::new();
/// stack.push(Element::row());
/// assert_eq!(stack.len(), 1);
/// ```
#[derive(Clone, Debug)]
pub struct LayerStack<Msg> {
    layers: Vec<Element<Msg>>,
}

impl<Msg> Default for LayerStack<Msg> {
    fn default() -> Self {
        Self { layers: Vec::new() }
    }
}

impl<Msg> LayerStack<Msg> {
    /// An empty layer stack.
    ///
    /// ```
    /// use guimaker::prelude::*;
    /// let stack: LayerStack<()> = LayerStack::new();
    /// assert!(stack.is_empty());
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Pushes a layer over the previous ones.
    ///
    /// ```
    /// use guimaker::prelude::*;
    /// let mut stack: LayerStack<()> = LayerStack::new();
    /// stack.push(Element::row());
    /// assert_eq!(stack.len(), 1);
    /// ```
    pub fn push(&mut self, layer: Element<Msg>) {
        self.layers.push(layer);
    }

    /// Pops the topmost layer.
    ///
    /// ```
    /// use guimaker::prelude::*;
    /// let mut stack: LayerStack<()> = LayerStack::new();
    /// stack.push(Element::row());
    /// assert!(stack.pop().is_some());
    /// ```
    #[must_use]
    pub fn pop(&mut self) -> Option<Element<Msg>> {
        self.layers.pop()
    }

    /// The number of layers.
    ///
    /// ```
    /// use guimaker::prelude::*;
    /// let stack: LayerStack<()> = LayerStack::new();
    /// assert_eq!(stack.len(), 0);
    /// ```
    #[must_use]
    pub const fn len(&self) -> usize {
        self.layers.len()
    }

    /// Whether there are no layers.
    ///
    /// ```
    /// use guimaker::prelude::*;
    /// let stack: LayerStack<()> = LayerStack::new();
    /// assert!(stack.is_empty());
    /// ```
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }
}

impl<Msg: Clone> LayerStack<Msg> {
    /// Builds the full stack with `base` at the bottom.
    ///
    /// ```
    /// use guimaker::prelude::*;
    /// let mut stack: LayerStack<()> = LayerStack::new();
    /// stack.push(Element::row());
    /// let view = stack.view(Element::column());
    /// assert_eq!(view.children.len(), 2);
    /// ```
    #[must_use]
    pub fn view(&self, base: Element<Msg>) -> Element<Msg> {
        layer_stack(base, self.layers.iter().cloned())
    }
}

fn place_overlay<Msg>(
    anchor: Element<Msg>,
    overlay: Element<Msg>,
    placement: Placement,
) -> Element<Msg> {
    match placement {
        Placement::Top => Element::column()
            .align(Align::Start)
            .gap(Spacing::S1)
            .child(overlay)
            .child(anchor),
        Placement::Bottom => Element::column()
            .align(Align::Start)
            .gap(Spacing::S1)
            .child(anchor)
            .child(overlay),
        Placement::Left => Element::row()
            .align(Align::Center)
            .gap(Spacing::S1)
            .child(overlay)
            .child(anchor),
        Placement::Right => Element::row()
            .align(Align::Center)
            .gap(Spacing::S1)
            .child(anchor)
            .child(overlay),
    }
}

fn tooltip_bubble<Msg>(text_value: &'static str) -> Element<Msg> {
    Element::row()
        .align(Align::Center)
        .pad_x(Spacing::S2)
        .pad_y(Spacing::S1)
        .bg(ColorRole::SurfaceRaised)
        .border(ColorRole::Border)
        .radius(Radius::Sm)
        .elevation(ElevationLevel::E2)
        .transition(Duration::Fast)
        .child(
            text::<Msg>(text_value)
                .text_role(TypeRole::Caption)
                .tone(Tone::Default),
        )
}

/// Attaches a hover tooltip to an anchor on the requested side.
///
/// The bubble is a small raised surface with a caption label; the host decides
/// when `open` is true, usually from [`TooltipState::is_open`] on the injected
/// clock.
///
/// ```
/// use guimaker::prelude::*;
/// let view: Element<()> = tooltip(text("Hover me"), "Tip", true, Placement::Bottom);
/// assert_eq!(view.kind, NodeKind::Column);
/// ```
#[must_use]
pub fn tooltip<Msg: Clone>(
    anchor: Element<Msg>,
    text_value: &'static str,
    open: bool,
    placement: Placement,
) -> Element<Msg> {
    if !open {
        return anchor;
    }
    place_overlay(anchor, tooltip_bubble(text_value), placement)
}

/// The hover-intent timer of a [`tooltip`].
///
/// The host stores the instant the pointer entered the anchor in `opened_at`;
/// the tooltip only opens once the pointer has rested for `delay_ms` on the
/// injected clock.
///
/// ```
/// use guimaker::prelude::*;
/// let state = TooltipState { delay_ms: 600.0, opened_at: Some(0.0) };
/// assert!(state.is_open(600.0));
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TooltipState {
    /// The hover-intent delay in milliseconds.
    pub delay_ms: f64,
    /// The instant the pointer entered the anchor, on the injected clock.
    pub opened_at: Option<f64>,
}

impl Default for TooltipState {
    fn default() -> Self {
        Self {
            delay_ms: HOVER_INTENT_MS,
            opened_at: None,
        }
    }
}

impl TooltipState {
    /// Whether the hover-intent delay has elapsed.
    ///
    /// ```
    /// use guimaker::prelude::*;
    /// let state = TooltipState { delay_ms: 600.0, opened_at: Some(100.0) };
    /// assert!(!state.is_open(699.0));
    /// assert!(state.is_open(700.0));
    /// ```
    #[must_use]
    pub fn is_open(&self, now_ms: f64) -> bool {
        match self.opened_at {
            Some(opened) => now_ms - opened >= self.delay_ms,
            None => false,
        }
    }
}

fn popover_panel<Msg>(content: Element<Msg>) -> Element<Msg> {
    Element::column()
        .pad(Spacing::S4)
        .bg(ColorRole::SurfaceRaised)
        .border(ColorRole::Border)
        .radius(Radius::Md)
        .elevation(ElevationLevel::E2)
        .transition(Duration::Fast)
        .child(content)
}

/// Attaches a popover panel to an anchor on the requested side.
///
/// ```
/// use guimaker::prelude::*;
/// let view: Element<()> = popover(text("Anchor"), text("Content"), true, Placement::Right);
/// assert_eq!(view.kind, NodeKind::Row);
/// ```
#[must_use]
pub fn popover<Msg: Clone>(
    anchor: Element<Msg>,
    content: Element<Msg>,
    open: bool,
    placement: Placement,
) -> Element<Msg> {
    if !open {
        return anchor;
    }
    place_overlay(anchor, popover_panel(content), placement)
}

/// One row of a [`menu`].
///
/// ```
/// use guimaker::prelude::*;
/// let entry: MenuItem<()> = MenuItem::item("Open", ());
/// assert_eq!(entry.label, "Open");
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct MenuItem<Msg> {
    /// The visible label, or `""` for a separator.
    pub label: &'static str,
    /// An optional keyboard shortcut shown at the trailing edge.
    pub shortcut: Option<&'static str>,
    /// `Some(true)` draws a check, `Some(false)` reserves its space.
    pub checked: Option<bool>,
    /// Whether the row is disabled.
    pub disabled: bool,
    /// Whether a divider is drawn before this row.
    pub separator_before: bool,
    /// The message emitted when the row is activated.
    pub message: Option<Msg>,
    /// The rows of a submenu, opened to the right.
    pub submenu: Vec<MenuItem<Msg>>,
}

impl<Msg> MenuItem<Msg> {
    /// A plain action row.
    ///
    /// ```
    /// use guimaker::prelude::*;
    /// let entry: MenuItem<()> = MenuItem::item("Open", ());
    /// assert!(entry.message.is_some());
    /// ```
    #[must_use]
    pub fn item(label: &'static str, message: Msg) -> Self {
        Self {
            label,
            shortcut: None,
            checked: None,
            disabled: false,
            separator_before: false,
            message: Some(message),
            submenu: Vec::new(),
        }
    }

    /// A checkable action row with its current state.
    ///
    /// ```
    /// use guimaker::prelude::*;
    /// let entry: MenuItem<()> = MenuItem::checkable("Loop", true, ());
    /// assert_eq!(entry.checked, Some(true));
    /// ```
    #[must_use]
    pub fn checkable(label: &'static str, checked: bool, message: Msg) -> Self {
        Self {
            label,
            shortcut: None,
            checked: Some(checked),
            disabled: false,
            separator_before: false,
            message: Some(message),
            submenu: Vec::new(),
        }
    }

    /// A divider between two groups of rows.
    ///
    /// ```
    /// use guimaker::prelude::*;
    /// let divider: MenuItem<()> = MenuItem::separator();
    /// assert!(divider.separator_before);
    /// ```
    #[must_use]
    pub fn separator() -> Self {
        Self {
            label: "",
            shortcut: None,
            checked: None,
            disabled: false,
            separator_before: true,
            message: None,
            submenu: Vec::new(),
        }
    }

    /// Adds a shortcut label to the row.
    ///
    /// ```
    /// use guimaker::prelude::*;
    /// let entry: MenuItem<()> = MenuItem::item("Save", ()).with_shortcut("Ctrl+S");
    /// assert_eq!(entry.shortcut, Some("Ctrl+S"));
    /// ```
    #[must_use]
    pub fn with_shortcut(mut self, shortcut: &'static str) -> Self {
        self.shortcut = Some(shortcut);
        self
    }

    /// Gives the row a submenu that opens to the right.
    ///
    /// ```
    /// use guimaker::prelude::*;
    /// let entry: MenuItem<()> = MenuItem::item("More", ()).with_submenu(vec![MenuItem::item("Undo", ())]);
    /// assert_eq!(entry.submenu.len(), 1);
    /// ```
    #[must_use]
    pub fn with_submenu(mut self, submenu: Vec<Self>) -> Self {
        self.submenu = submenu;
        self
    }

    fn is_separator(&self) -> bool {
        self.label.is_empty()
            && self.message.is_none()
            && self.submenu.is_empty()
            && self.checked.is_none()
    }
}

fn menu_spec() -> StateSpec {
    StateSpec {
        bg: None,
        hover_bg: Some(ColorRole::Surface),
        pressed_bg: Some(ColorRole::SurfaceSunken),
        selected_bg: Some(ColorRole::AccentSubtle),
        disabled_bg: None,
        border: None,
        hover_border: None,
        tone: Tone::Default,
        hover_tone: Some(Tone::Default),
        pressed_tone: Some(Tone::Default),
        selected_tone: Some(Tone::Default),
        disabled_tone: Some(Tone::Faint),
        elevation_hover: None,
    }
}

fn menu_row<Msg: Clone>(item: &MenuItem<Msg>) -> Element<Msg> {
    let size = ControlSize::Md;
    let spec = menu_spec();
    let mut row = spec.apply(
        Element::row()
            .width(SizeSpec::Fill)
            .gap(Spacing::S2)
            .align(Align::Center)
            .pad_x(Spacing::S2)
            .pad_y(Spacing::S1)
            .min_height(size.height())
            .radius(Radius::Xs)
            .cursor(Cursor::Pointer)
            .transition(Duration::Fast)
            .selected(item.checked == Some(true)),
    );
    // The check column is always reserved, so every label starts at one x and
    // a checked row cannot shift its neighbours.
    row = row.child(match item.checked {
        Some(true) => icon_box(Icon::Check, size, spec),
        Some(false) | None => Element::row()
            .width(SizeSpec::Token(size.icon()))
            .height(SizeSpec::Token(size.icon())),
    });
    row = row.child(
        label_text(item.label, size.label_role(), spec)
            .priority(1)
            .lines(1)
            .ellipsis(Ellipsis::End),
    );
    row = row.child(Element::spacer());
    // One fixed-width, right-aligned shortcut column for the whole menu.
    let mut shortcut_cell = Element::row()
        .key("menu_shortcut")
        .width(SizeSpec::Token(Spacing::S16))
        .justify(Justify::End)
        .align(Align::Center);
    if let Some(shortcut) = item.shortcut {
        shortcut_cell = shortcut_cell.child(
            text::<Msg>(shortcut)
                .text_role(TypeRole::Caption)
                .tone(Tone::Faint),
        );
    }
    row = row.child(shortcut_cell);
    // The chevron slot is reserved too, so the shortcut column never moves.
    row = row.child(if item.submenu.is_empty() {
        Element::row()
            .width(SizeSpec::Token(size.icon()))
            .height(SizeSpec::Token(size.icon()))
    } else {
        icon_box(Icon::ChevronRight, size, spec)
    });
    if let Some(message) = item.message.clone() {
        row = row.on_click(message);
    } else if !item.submenu.is_empty() {
        row = row.interactive();
    }
    if item.disabled {
        row = row.disabled();
    }
    row
}

fn menu_panel<Msg: Clone>(items: &[MenuItem<Msg>], open_submenu: Option<usize>) -> Element<Msg> {
    // A menu is content-sized, never a fixed box: `Hug` asks the layout to run
    // the real text measurement and take the widest label (including the
    // check, shortcut and chevron columns and every gap the row reserves) with
    // the theme minimum as a floor. `Hug` on the height sums the real row and
    // divider heights, so no empty band is left under the last item. Callers
    // that need an explicit box still override both with `.width()`/`.height()`.
    let mut panel = Element::column()
        .width(SizeSpec::Hug)
        .height(SizeSpec::Hug)
        .pad(Spacing::S1)
        .gap(Spacing::S0)
        .min_width(Spacing::S24)
        .max_width(ContentWidth::Sm)
        .bg(ColorRole::SurfaceRaised)
        .border(ColorRole::Border)
        .radius(Radius::Md)
        .elevation(ElevationLevel::E3);
    for (index, item) in items.iter().enumerate() {
        if item.separator_before {
            panel = panel.child(Element::divider());
        }
        if item.is_separator() {
            continue;
        }
        let entry = menu_row(item);
        let entry = if open_submenu == Some(index) && !item.submenu.is_empty() {
            // The submenu is an overlay anchored to this row. It is absolutely
            // positioned inside the stack, so it never adds height (the gap
            // under "Export" came from laying it out inline).
            Element::overlay().width(SizeSpec::Fill).child(entry).child(
                menu(&item.submenu, None).overlay_to(0, glam::Vec2::new(Spacing::S1.px(), 0.0)),
            )
        } else {
            entry
        };
        panel = panel.child(entry);
    }
    panel
}

/// A vertical menu panel with checks, shortcuts and nested submenus.
///
/// `open_submenu` is the index of the row whose submenu is shown, if any.
///
/// ```
/// use guimaker::prelude::*;
/// let items = [MenuItem::item("Open", ()), MenuItem::separator()];
/// let panel: Element<()> = menu(&items, None);
/// assert_eq!(panel.kind, NodeKind::Column);
/// ```
#[must_use]
pub fn menu<Msg: Clone>(items: &[MenuItem<Msg>], open_submenu: Option<usize>) -> Element<Msg> {
    menu_panel(items, open_submenu).key("menu")
}

/// The same menu panel, documented for a pointer position.
///
/// The host keeps the pointer position from the press that opened the menu and
/// places this panel there in an overlay layer; the panel itself is position
/// agnostic.
///
/// ```
/// use guimaker::prelude::*;
/// let items = [MenuItem::item("Copy", ()), MenuItem::item("Paste", ())];
/// let panel: Element<()> = context_menu(&items, None);
/// assert_eq!(panel.kind, NodeKind::Column);
/// ```
#[must_use]
pub fn context_menu<Msg: Clone>(
    items: &[MenuItem<Msg>],
    open_submenu: Option<usize>,
) -> Element<Msg> {
    menu_panel(items, open_submenu).key("context_menu")
}

/// The three dialog widths.
///
/// ```
/// use guimaker::prelude::*;
/// assert_eq!(DialogSize::Lg.width(), ContentWidth::Lg);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DialogSize {
    /// A narrow confirmation.
    Sm,
    /// The default dialog.
    #[default]
    Md,
    /// A wide form or preview.
    Lg,
}

impl DialogSize {
    /// The content width the dialog card clamps to.
    ///
    /// ```
    /// use guimaker::prelude::*;
    /// assert_eq!(DialogSize::Sm.width(), ContentWidth::Sm);
    /// ```
    #[must_use]
    pub const fn width(self) -> ContentWidth {
        match self {
            Self::Sm => ContentWidth::Sm,
            Self::Md => ContentWidth::Md,
            Self::Lg => ContentWidth::Lg,
        }
    }
}

/// A modal dialog: a scrim over the viewport and a centred, blurred card.
///
/// The host pushes a trapping focus scope and the layer's overlay path while
/// the dialog is open (see [`trap`]) and pops both when it closes.
///
/// ```
/// use guimaker::prelude::*;
/// let view: Element<()> = dialog("Rename", text("Layer"), [button("Save", ButtonVariant::Primary, ())], DialogSize::Sm);
/// assert_eq!(view.kind, NodeKind::Stack);
/// ```
#[must_use]
pub fn dialog<Msg: Clone>(
    title: &str,
    body: Element<Msg>,
    actions: impl IntoIterator<Item = Element<Msg>>,
    size: DialogSize,
) -> Element<Msg> {
    let scrim = Element::row()
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .bg(ColorRole::Overlay);
    let mut action_row = Element::row()
        .width(SizeSpec::Fill)
        .justify(Justify::End)
        .gap(Spacing::S2);
    for action in actions {
        action_row = action_row.child(action.transition(Duration::Fast));
    }
    let card = Element::column()
        .key("dialog_card")
        .width(SizeSpec::Fill)
        .height(SizeSpec::Hug)
        .align(Align::Stretch)
        .gap(Spacing::S4)
        .pad(Spacing::S6)
        .max_width(size.width())
        .align_self(Align::Center)
        .backdrop_blur(Spacing::S6)
        .bg(ColorRole::SurfaceRaised)
        .border(ColorRole::Border)
        .radius(Radius::Lg)
        .elevation(ElevationLevel::E4)
        .child(text::<Msg>(title).text_role(TypeRole::Heading).priority(1))
        .child(body)
        .child(action_row);
    let centred = Element::column()
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .align(Align::Center)
        .justify(Justify::Center)
        .child(card);
    Element::stack()
        .key("dialog")
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .child(scrim)
        .child(centred)
}

/// A case-insensitive subsequence match for the command palette.
///
/// ```
/// use guimaker::prelude::*;
/// assert!(fuzzy_match("sf", "Save File"));
/// assert!(!fuzzy_match("xyz", "Save File"));
/// ```
#[must_use]
pub fn fuzzy_match(query: &str, text_value: &str) -> bool {
    let mut text_chars = text_value.chars().flat_map(char::to_lowercase);
    for query_char in query.chars().flat_map(char::to_lowercase) {
        if !text_chars.by_ref().any(|text_char| text_char == query_char) {
            return false;
        }
    }
    true
}

fn palette_row<Msg: Clone>(
    label: &'static str,
    shortcut: &'static str,
    message: Msg,
    selected: bool,
    index: usize,
) -> Element<Msg> {
    let size = ControlSize::Md;
    let spec = menu_spec();
    let mut row = spec.apply(
        Element::row()
            .gap(Spacing::S2)
            .align(Align::Center)
            .pad_x(size.pad_x())
            .pad_y(Spacing::S1)
            .min_height(size.height())
            .radius(Radius::Xs)
            .cursor(Cursor::Pointer)
            .transition(Duration::Fast)
            .selected(selected)
            .on_click(message),
    );
    row = row.child(
        label_text(label, size.label_role(), spec)
            .priority(1)
            .lines(1)
            .ellipsis(Ellipsis::End),
    );
    row = row.child(Element::spacer());
    if !shortcut.is_empty() {
        row = row.child(
            text::<Msg>(shortcut)
                .text_role(TypeRole::Caption)
                .tone(if selected { Tone::Default } else { Tone::Muted }),
        );
    }
    if selected {
        row.tab_index(0)
    } else {
        row.tab_index(i32::try_from(index + 1).unwrap_or(i32::MAX))
    }
}

/// A centred, keyboard-first command palette with fuzzy filtering.
///
/// `selected` is the index in the filtered result list; the selected row takes
/// tab index zero and the rest follow. Each row is clickable, so `Enter` and
/// `Space` activate the focused row through the router.
///
/// ```
/// use guimaker::prelude::*;
/// let view: Element<()> = command_palette("save", [("Save", "Ctrl+S", ())], 0);
/// assert_eq!(view.kind, NodeKind::Column);
/// ```
#[must_use]
pub fn command_palette<Msg: Clone>(
    query: &str,
    items: impl IntoIterator<Item = (&'static str, &'static str, Msg)>,
    selected: usize,
) -> Element<Msg> {
    let size = ControlSize::Md;
    let mut search = Element::row()
        .key("command_search")
        .gap(Spacing::S2)
        .align(Align::Center)
        .pad_x(size.pad_x())
        .pad_y(Spacing::S2)
        .child(icon_box(Icon::Search, size, StateSpec::tone(Tone::Muted)));
    search = if query.is_empty() {
        search.child(
            text::<Msg>("Search commands")
                .text_role(size.label_role())
                .tone(Tone::Faint),
        )
    } else {
        search.child(text::<Msg>(query).text_role(size.label_role()))
    };
    let mut results = Element::column().width(SizeSpec::Fill);
    for (index, (label, shortcut, message)) in items
        .into_iter()
        .filter(|(label, _, _)| query.is_empty() || fuzzy_match(query, label))
        .enumerate()
    {
        results = results.child(palette_row(
            label,
            shortcut,
            message,
            index == selected,
            index,
        ));
    }
    Element::column()
        .key("command_palette")
        .width(SizeSpec::Fill)
        .max_width(ContentWidth::Md)
        .align_self(Align::Center)
        .pad(Spacing::S1)
        .gap(Spacing::S1)
        .bg(ColorRole::SurfaceRaised)
        .border(ColorRole::Border)
        .radius(Radius::Md)
        .elevation(ElevationLevel::E3)
        .child(search)
        .child(results)
}

/// A toast notification.
///
/// ```
/// use guimaker::prelude::*;
/// let toast = Toast::new("Saved", Status::Success);
/// assert_eq!(toast.status, Status::Success);
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct Toast {
    /// The primary line.
    pub title: String,
    /// An optional secondary line.
    pub body: Option<String>,
    /// The semantic status the toast reports.
    pub status: Status,
}

impl Toast {
    /// A toast with a title and a status and no body.
    ///
    /// ```
    /// use guimaker::prelude::*;
    /// let toast = Toast::new("Saved", Status::Success);
    /// assert_eq!(toast.title, "Saved");
    /// assert!(toast.body.is_none());
    /// ```
    #[must_use]
    pub fn new(title: impl Into<String>, status: Status) -> Self {
        Self {
            title: title.into(),
            body: None,
            status,
        }
    }

    /// Adds a secondary line.
    ///
    /// ```
    /// use guimaker::prelude::*;
    /// let toast = Toast::new("Saved", Status::Success).with_body("All changes stored");
    /// assert_eq!(toast.body.as_deref(), Some("All changes stored"));
    /// ```
    #[must_use]
    pub fn with_body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self
    }
}

fn toast_icon(status: Status) -> Icon {
    match status {
        Status::Info => Icon::Bell,
        Status::Success => Icon::Check,
        Status::Warning => Icon::Warning,
        Status::Danger => Icon::Error,
    }
}

/// One toast row: a status icon, a title and body, and an optional dismiss.
///
/// ```
/// use guimaker::prelude::*;
/// let value = Toast::new("Saved", Status::Success);
/// let view: Element<()> = toast(&value, Some(()));
/// assert_eq!(view.kind, NodeKind::Row);
/// ```
#[must_use]
pub fn toast<Msg: Clone>(value: &Toast, dismiss: Option<Msg>) -> Element<Msg> {
    let size = ControlSize::Md;
    let status_spec = StateSpec::tone(value.status.tone());
    let mut copy = Element::column().gap(Spacing::S1).grow(1.0).child(
        text::<Msg>(value.title.as_str())
            .text_role(TypeRole::BodyStrong)
            .tone(Tone::Default),
    );
    if let Some(body) = value.body.as_deref() {
        copy = copy.child(
            text::<Msg>(body)
                .text_role(TypeRole::Caption)
                .tone(Tone::Muted),
        );
    }
    let mut row = Element::row()
        .gap(Spacing::S2)
        .align(Align::Start)
        .pad(Spacing::S3)
        .width(SizeSpec::Fill)
        .bg(ColorRole::SurfaceRaised)
        .border(ColorRole::Border)
        .radius(Radius::Md)
        .elevation(ElevationLevel::E2)
        .transition(Duration::Fast)
        .child(icon_box(toast_icon(value.status), size, status_spec))
        .child(copy);
    if let Some(message) = dismiss {
        row = row.child(icon_button(
            Icon::Close,
            "Dismiss",
            ButtonVariant::Ghost,
            ControlSize::Sm,
            message,
        ));
    }
    row
}

/// A column of toasts aligned to the trailing edge.
///
/// ```
/// use guimaker::prelude::*;
/// let view: Element<usize> = toast_stack([Toast::new("One", Status::Info)], |index| index);
/// assert_eq!(view.kind, NodeKind::Column);
/// ```
#[must_use]
pub fn toast_stack<Msg: Clone>(
    toasts: impl IntoIterator<Item = Toast>,
    dismiss: impl Fn(usize) -> Msg,
) -> Element<Msg> {
    let mut column = Element::column()
        .key("toast_stack")
        .align(Align::End)
        .align_self(Align::End)
        .gap(Spacing::S2)
        .width(SizeSpec::Fill)
        .max_width(ContentWidth::Sm);
    for (index, value) in toasts.into_iter().enumerate() {
        column = column.child(toast(&value, Some(dismiss(index))));
    }
    column
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)] // reason: timer and token values are exact literals in tests
    use super::{
        command_palette, dialog, fuzzy_match, layer_stack, menu, popover, toast, toast_stack,
        tooltip, DialogSize, LayerStack, MenuItem, Placement, Toast, TooltipState,
    };
    use crate::prelude::*;

    fn has_backdrop_blur<Msg>(element: &Element<Msg>) -> bool {
        element.style.backdrop_blur == Some(Spacing::S6)
            || element.children.iter().any(has_backdrop_blur)
    }

    fn has_dismiss<Msg>(element: &Element<Msg>) -> bool {
        let dismisses = element.action.is_some()
            && element
                .style
                .tooltip
                .as_ref()
                .is_some_and(|tooltip| &*tooltip.text == "Dismiss");
        dismisses || element.children.iter().any(has_dismiss)
    }

    fn count_actions<Msg>(element: &Element<Msg>) -> usize {
        usize::from(element.action.is_some())
            + element.children.iter().map(count_actions).sum::<usize>()
    }

    fn count_selected<Msg>(element: &Element<Msg>) -> usize {
        usize::from(element.style.selected)
            + element.children.iter().map(count_selected).sum::<usize>()
    }

    fn collect_text<Msg>(element: &Element<Msg>, out: &mut Vec<String>) {
        if let Some(spec) = &element.text {
            out.push(spec.text());
        }
        for child in &element.children {
            collect_text(child, out);
        }
    }

    #[test]
    fn fuzzy_match_is_a_case_insensitive_subsequence() {
        assert!(fuzzy_match("", "anything"));
        assert!(fuzzy_match("sf", "Save File"));
        assert!(fuzzy_match("SVE", "save"));
        assert!(!fuzzy_match("ess", "save"));
        assert!(!fuzzy_match("xyz", "Save File"));
    }

    #[test]
    fn layer_stack_puts_the_base_first_and_layers_over_it() {
        let view: Element<()> =
            layer_stack(Element::row().key("base"), [Element::column().key("layer")]);
        assert_eq!(view.kind, NodeKind::Stack);
        assert_eq!(view.children.len(), 2);
        assert_eq!(view.children[0].key, Some(Key::Static("base")));
        assert_eq!(view.children[1].children[0].key, Some(Key::Static("layer")));

        let mut layers: LayerStack<()> = LayerStack::new();
        layers.push(Element::row());
        assert_eq!(layers.len(), 1);
        assert!(layers.pop().is_some());
        assert!(layers.is_empty());
    }

    #[test]
    fn tooltip_delay_uses_the_injected_clock() {
        let state = TooltipState::default();
        assert_eq!(state.delay_ms, 600.0);
        assert!(state.opened_at.is_none());

        let started = TooltipState {
            delay_ms: 600.0,
            opened_at: Some(100.0),
        };
        assert!(!started.is_open(699.0));
        assert!(started.is_open(700.0));
        assert!(started.is_open(900.0));
    }

    #[test]
    fn placement_flips_to_the_side_that_has_room() {
        assert_eq!(Placement::Bottom.flipped(false), Placement::Top);
        assert_eq!(Placement::Top.flipped(true), Placement::Bottom);
        assert_eq!(Placement::Right.flipped(false), Placement::Left);
        assert_eq!(Placement::Left.flipped(true), Placement::Right);
        assert_eq!(Placement::Bottom.flipped(true), Placement::Bottom);
    }

    #[test]
    fn a_tooltip_opens_on_the_requested_side() {
        let top: Element<()> = tooltip(text("hover"), "Tip", true, Placement::Top);
        assert_eq!(top.kind, NodeKind::Column);
        assert_eq!(top.children[0].style.bg, Some(ColorRole::SurfaceRaised));

        let left: Element<()> = tooltip(text("hover"), "Tip", true, Placement::Left);
        assert_eq!(left.kind, NodeKind::Row);
        assert_eq!(left.children[0].style.bg, Some(ColorRole::SurfaceRaised));
    }

    #[test]
    fn a_popover_places_its_panel_below_the_anchor() {
        let view: Element<()> = popover(text("anchor"), text("content"), true, Placement::Bottom);
        assert_eq!(view.kind, NodeKind::Column);
        assert_eq!(view.children[1].style.bg, Some(ColorRole::SurfaceRaised));
    }

    #[test]
    fn menu_renders_separators_checks_and_submenus() {
        let items = [
            MenuItem::checkable("Loop", true, ()),
            MenuItem::separator(),
            MenuItem::item("Open", ())
                .with_shortcut("Ctrl+O")
                .with_submenu(vec![MenuItem::item("Recent", ())]),
        ];
        let view: Element<()> = menu(&items, Some(2));
        assert_eq!(view.kind, NodeKind::Column);
        assert_eq!(view.children.len(), 3);
        assert!(view.children[0].style.selected);
        assert_eq!(view.children[1].kind, NodeKind::Divider);
        assert_eq!(
            view.children[2].kind,
            NodeKind::Overlay,
            "an open submenu is an anchored overlay, not an inline row"
        );
        assert_eq!(count_selected(&view), 1);
    }

    #[test]
    fn dialog_has_a_blurred_backdrop() {
        let view: Element<()> = dialog(
            "Rename",
            text("Layer"),
            [button("Save", ButtonVariant::Primary, ())],
            DialogSize::Sm,
        );
        assert_eq!(view.kind, NodeKind::Stack);
        assert!(has_backdrop_blur(&view));
    }

    #[test]
    fn command_palette_filters_and_marks_the_selection() {
        let view: Element<()> = command_palette(
            "sf",
            [("Save File", "Ctrl+S", ()), ("Open", "Ctrl+O", ())],
            0,
        );
        let mut texts = Vec::new();
        collect_text(&view, &mut texts);
        assert!(texts.iter().any(|value| value == "Save File"));
        assert!(!texts.iter().any(|value| value == "Open"));
        assert_eq!(count_actions(&view), 1);
        assert_eq!(count_selected(&view), 1);
    }

    #[test]
    fn a_toast_dismiss_button_carries_a_tooltip_and_action() {
        let value = Toast::new("Saved", Status::Success).with_body("All changes stored");
        let view: Element<usize> = toast(&value, Some(7));
        assert_eq!(view.kind, NodeKind::Row);
        assert!(has_dismiss(&view));
        assert_eq!(count_actions(&view), 1);

        let stack: Element<usize> = toast_stack([Toast::new("One", Status::Info)], |index| index);
        assert_eq!(stack.kind, NodeKind::Column);
        assert_eq!(stack.children.len(), 1);
        assert!(has_dismiss(&stack));
    }
}
