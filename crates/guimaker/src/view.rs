//! The view tree: structure and intent, never coordinates.
//!
//! An app builds an [`Element`] tree from its state with the constructors in
//! this module (`Element::column`, `Element::row`, [`text`], …) and the
//! chainable modifiers (`.pad(S4)`, `.gap(S3)`, `.width(SizeSpec::Fill)`).
//! [`Tree::build`] flattens it into an arena of [`Node`]s; layout, input and the
//! display list work on that arena. No element stores a pixel position or a
//! colour value.

#![allow(clippy::cast_possible_truncation, clippy::too_many_lines)] // reason: arena indices are u32 and the builder/hash are one linear pass each
use std::sync::Arc;

use crate::hash::Fnv;
use crate::style::{
    Align, Columns, Direction, Edges, Justify, ScrollAxes, SizeSpec, Span, Style, TextSpec, Tone,
    Tooltip,
};
use crate::text::Ellipsis;
use crate::theme::{
    BorderWidth, ColorRole, ContentWidth, ElevationLevel, Radius, Spacing, TypeRole,
};

/// A stable identifier for a node inside one built tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(
    /// The arena index.
    pub u32,
);

impl NodeId {
    /// The arena index.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// A node's structural kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NodeKind {
    /// A vertical stack (`column`).
    Column,
    /// A horizontal stack (`row`).
    Row,
    /// Overlaid children (`stack`).
    Stack,
    /// A grid (`grid`).
    Grid,
    /// A centring container.
    Center,
    /// A flexible gap (`spacer`).
    Spacer,
    /// A hairline rule.
    Divider,
    /// A scroll container.
    Scroll,
    /// A wrapping flow.
    Wrap,
    /// Two panes with a divider.
    Split,
    /// A fixed-aspect container.
    Aspect,
    /// Measured, wrapped text.
    Text,
    /// An image placeholder drawn by the adapter.
    Image,
    /// An application-defined canvas rect.
    Canvas,
}

impl NodeKind {
    /// The kind's stable name for inspect and debug output.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Column => "column",
            Self::Row => "row",
            Self::Stack => "stack",
            Self::Grid => "grid",
            Self::Center => "center",
            Self::Spacer => "spacer",
            Self::Divider => "divider",
            Self::Scroll => "scroll",
            Self::Wrap => "wrap",
            Self::Split => "split",
            Self::Aspect => "aspect",
            Self::Text => "text",
            Self::Image => "image",
            Self::Canvas => "canvas",
        }
    }

    /// Whether the kind is a text node.
    #[must_use]
    pub const fn is_text(self) -> bool {
        matches!(self, Self::Text)
    }
}

/// A stable key used for the store and for debug paths.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    /// A static key written in the view.
    Static(&'static str),
    /// An application id, hashed into the path.
    Id(u64),
}

impl Key {
    fn hash_into(&self, hash: &mut Fnv) {
        match self {
            Self::Static(name) => {
                hash.write_u8(1);
                hash.write_str(name);
            }
            Self::Id(id) => {
                hash.write_u8(2);
                hash.write_u64(*id);
            }
        }
    }

    /// A printable name for inspect and debug output.
    #[must_use]
    pub fn display(&self) -> String {
        match self {
            Self::Static(name) => (*name).to_string(),
            Self::Id(id) => format!("#{id:016x}"),
        }
    }
}

/// A node in the flattened arena.
#[derive(Clone, Debug)]
pub struct Node<Msg> {
    /// The node's id (its arena index).
    pub id: NodeId,
    /// The structural kind.
    pub kind: NodeKind,
    /// The unresolved style tokens.
    pub style: Style,
    /// Text content for a text node.
    pub text: Option<TextSpec>,
    /// The stable key, when the view supplied one.
    pub key: Option<Key>,
    /// The message emitted by a click, when the node is interactive.
    pub action: Option<Msg>,
    /// Whether the node participates in hit testing.
    pub interactive: bool,
    /// The parent node.
    pub parent: Option<NodeId>,
    /// The children, in paint order.
    pub children: Vec<NodeId>,
    /// Depth from the root.
    pub depth: u16,
    /// A stable hash of the node's path from the root.
    pub path_hash: u64,
    /// A hash of kind + style + text + children, used to memoise layout.
    pub content_hash: u64,
    /// The node's index among its siblings.
    pub sibling_index: u32,
    /// An adapter-defined image source id.
    pub image: Option<u64>,
}

impl<Msg> Node<Msg> {
    /// The text of a text node, if any.
    #[must_use]
    pub fn text(&self) -> Option<&TextSpec> {
        self.text.as_ref()
    }
}

/// A declarative element. Build one with a constructor and chain modifiers.
#[derive(Clone, Debug)]
pub struct Element<Msg> {
    /// The structural kind.
    pub kind: NodeKind,
    /// The style tokens.
    pub style: Style,
    /// Text content.
    pub text: Option<TextSpec>,
    /// The children, in paint order.
    pub children: Vec<Element<Msg>>,
    /// The stable key.
    pub key: Option<Key>,
    /// The message emitted on click.
    pub action: Option<Msg>,
    /// An adapter-defined image source id.
    pub image: Option<u64>,
}

impl<Msg> Element<Msg> {
    /// Starts an element of a given kind.
    #[must_use]
    pub fn new(kind: NodeKind) -> Self {
        Self {
            kind,
            style: Style::default(),
            text: None,
            children: Vec::new(),
            key: None,
            action: None,
            image: None,
        }
    }

    /// A vertical stack.
    #[must_use]
    pub fn column() -> Self {
        Self::new(NodeKind::Column).flex(Direction::Column)
    }

    /// A horizontal stack.
    #[must_use]
    pub fn row() -> Self {
        Self::new(NodeKind::Row).flex(Direction::Row)
    }

    /// Overlaid children.
    #[must_use]
    pub fn stack() -> Self {
        Self::new(NodeKind::Stack).flex(Direction::Column)
    }

    /// A grid with a column rule.
    #[must_use]
    pub fn grid(columns: Columns) -> Self {
        let mut element = Self::new(NodeKind::Grid).flex(Direction::Column);
        element.style.columns = Some(columns);
        element
    }

    /// A container that centres its child.
    #[must_use]
    pub fn center() -> Self {
        let mut element = Self::new(NodeKind::Center).flex(Direction::Column);
        element.style.align = Align::Center;
        element.style.justify = Justify::Center;
        element.style.grow = 1.0;
        element
    }

    /// A flexible gap.
    #[must_use]
    pub fn spacer() -> Self {
        let mut element = Self::new(NodeKind::Spacer);
        element.style.grow = 1.0;
        element
    }

    /// A hairline divider.
    #[must_use]
    pub fn divider() -> Self {
        Self::new(NodeKind::Divider)
    }

    /// A scroll container.
    #[must_use]
    pub fn scroll(axes: ScrollAxes) -> Self {
        let mut element = Self::new(NodeKind::Scroll).flex(Direction::Column);
        element.style.scroll = axes;
        element.style.clip = true;
        element
    }

    /// A wrapping flow of children.
    #[must_use]
    pub fn wrap() -> Self {
        let mut element = Self::new(NodeKind::Wrap).flex(Direction::Row);
        element.style.wrap = true;
        element
    }

    /// Two panes with a divider between them.
    #[must_use]
    pub fn split(first: Element<Msg>, second: Element<Msg>) -> Self
    where
        Msg: Clone,
    {
        Self::row()
            .child(first)
            .child(Self::divider())
            .child(second)
    }

    /// A fixed-aspect container around a child.
    #[must_use]
    pub fn aspect(ratio: f32, child: Element<Msg>) -> Self {
        let mut element = Self::new(NodeKind::Aspect).flex(Direction::Column);
        element.style.aspect = Some(ratio);
        element.child(child)
    }

    /// An application-defined canvas rect.
    #[must_use]
    pub fn canvas() -> Self {
        let mut element = Self::new(NodeKind::Canvas);
        element.style.interactive = true;
        element
    }

    /// An image placeholder.
    #[must_use]
    pub fn image(source: u64) -> Self {
        let mut element = Self::new(NodeKind::Image);
        element.image = Some(source);
        element
    }

    fn flex(mut self, direction: Direction) -> Self {
        self.style.direction = direction;
        self
    }

    /// Appends a child.
    #[must_use]
    pub fn child(mut self, child: Element<Msg>) -> Self {
        self.children.push(child);
        self
    }

    /// Appends children.
    #[must_use]
    pub fn children(mut self, children: impl IntoIterator<Item = Element<Msg>>) -> Self {
        self.children.extend(children);
        self
    }

    /// Gives the node a stable static key.
    #[must_use]
    pub fn key(mut self, key: &'static str) -> Self {
        self.key = Some(Key::Static(key));
        self
    }

    /// Gives the node a stable application id as its key.
    #[must_use]
    pub fn key_id(mut self, id: u64) -> Self {
        self.key = Some(Key::Id(id));
        self
    }

    /// Sets padding on every edge.
    #[must_use]
    pub fn pad(mut self, padding: Spacing) -> Self {
        self.style.padding = Edges::all(padding);
        self
    }

    /// Sets horizontal padding.
    #[must_use]
    pub fn pad_x(mut self, padding: Spacing) -> Self {
        self.style.padding.left = padding;
        self.style.padding.right = padding;
        self
    }

    /// Sets vertical padding.
    #[must_use]
    pub fn pad_y(mut self, padding: Spacing) -> Self {
        self.style.padding.top = padding;
        self.style.padding.bottom = padding;
        self
    }

    /// Sets the gap between children.
    #[must_use]
    pub fn gap(mut self, gap: Spacing) -> Self {
        self.style.gap = gap;
        self
    }

    /// Sets cross-axis alignment.
    #[must_use]
    pub fn align(mut self, align: Align) -> Self {
        self.style.align = align;
        self
    }

    /// Sets main-axis distribution.
    #[must_use]
    pub fn justify(mut self, justify: Justify) -> Self {
        self.style.justify = justify;
        self
    }

    /// Sets the flex grow factor.
    #[must_use]
    pub fn grow(mut self, grow: f32) -> Self {
        self.style.grow = grow;
        self
    }

    /// Sets the flex shrink factor.
    #[must_use]
    pub fn shrink(mut self, shrink: f32) -> Self {
        self.style.shrink = shrink;
        self
    }

    /// Sets the width rule.
    #[must_use]
    pub fn width(mut self, width: SizeSpec) -> Self {
        self.style.width = width;
        self
    }

    /// Sets the height rule.
    #[must_use]
    pub fn height(mut self, height: SizeSpec) -> Self {
        self.style.height = height;
        self
    }

    /// Sets the minimum width.
    #[must_use]
    pub fn min_width(mut self, width: Spacing) -> Self {
        self.style.min_width = Some(width);
        self
    }

    /// Sets the minimum height.
    #[must_use]
    pub fn min_height(mut self, height: Spacing) -> Self {
        self.style.min_height = Some(height);
        self
    }

    /// Sets a maximum width from a spacing token or a content-column token.
    #[must_use]
    pub fn max_width(mut self, width: impl MaxWidthToken) -> Self {
        width.apply(&mut self.style);
        self
    }

    /// Sets the maximum height.
    #[must_use]
    pub fn max_height(mut self, height: Spacing) -> Self {
        self.style.max_height = Some(height);
        self
    }

    /// Sets a content column maximum.
    #[must_use]
    pub fn content_width(mut self, width: ContentWidth) -> Self {
        self.style.content_width = Some(width);
        self
    }

    /// Sets the background colour role.
    #[must_use]
    pub fn bg(mut self, role: ColorRole) -> Self {
        self.style.bg = Some(role);
        self
    }

    /// Sets the border colour role.
    #[must_use]
    pub fn border(mut self, role: ColorRole) -> Self {
        self.style.border = Some(role);
        self
    }

    /// Sets the border width token.
    #[must_use]
    pub fn border_width(mut self, width: BorderWidth) -> Self {
        self.style.border_width = width;
        self
    }

    /// Sets the corner radius token.
    #[must_use]
    pub fn radius(mut self, radius: Radius) -> Self {
        self.style.radius = radius;
        self
    }

    /// Sets the elevation level.
    #[must_use]
    pub fn elevation(mut self, elevation: ElevationLevel) -> Self {
        self.style.elevation = elevation;
        self
    }

    /// Sets the opacity.
    #[must_use]
    pub fn opacity(mut self, opacity: f32) -> Self {
        self.style.opacity = opacity.clamp(0.0, 1.0);
        self
    }

    /// Clips children to the node's rectangle.
    #[must_use]
    pub fn clip(mut self) -> Self {
        self.style.clip = true;
        self
    }

    /// Sets the default text tone.
    #[must_use]
    pub fn tone(mut self, tone: Tone) -> Self {
        self.style.tone = tone;
        self
    }

    /// Sets the default text role.
    #[must_use]
    pub fn text_role(mut self, role: TypeRole) -> Self {
        self.style.text_role = role;
        self
    }

    /// Clamps text to a number of lines.
    #[must_use]
    pub fn lines(mut self, lines: u16) -> Self {
        self.style.max_lines = Some(lines);
        if let Some(text) = &mut self.text {
            text.max_lines = Some(lines);
        }
        self
    }

    /// Sets the ellipsis mode.
    #[must_use]
    pub fn ellipsis(mut self, ellipsis: Ellipsis) -> Self {
        self.style.ellipsis = ellipsis;
        if let Some(text) = &mut self.text {
            text.ellipsis = ellipsis;
        }
        self
    }

    /// Uses tabular digits.
    #[must_use]
    pub fn tabular(mut self) -> Self {
        self.style.tabular = true;
        self
    }

    /// Hides the node and its subtree.
    #[must_use]
    pub fn hidden(mut self) -> Self {
        self.style.hidden = true;
        self
    }

    /// Disables the node.
    #[must_use]
    pub fn disabled(mut self) -> Self {
        self.style.disabled = true;
        self
    }

    /// Attaches a tooltip.
    #[must_use]
    pub fn tooltip(mut self, text: &'static str) -> Self {
        self.style.tooltip = Some(Tooltip {
            text: Arc::from(text),
        });
        self
    }

    /// Sets the aspect ratio (width / height).
    #[must_use]
    pub fn aspect_ratio(mut self, ratio: f32) -> Self {
        self.style.aspect = Some(ratio);
        self
    }

    /// Sets the scroll axes.
    #[must_use]
    pub fn scroll_axes(mut self, axes: ScrollAxes) -> Self {
        self.style.scroll = axes;
        self.style.clip = self.style.clip || axes.clips();
        self
    }

    /// Enables wrapping.
    #[must_use]
    pub fn flow_wrap(mut self) -> Self {
        self.style.wrap = true;
        self
    }

    /// Makes the node a hit-test target.
    #[must_use]
    pub fn interactive(mut self) -> Self {
        self.style.interactive = true;
        self
    }

    /// Emits `message` when the node is clicked and makes it interactive.
    #[must_use]
    pub fn on_click(mut self, message: Msg) -> Self {
        self.action = Some(message);
        self.style.interactive = true;
        self
    }

    /// Replaces the text content.
    #[must_use]
    pub fn text_spec(mut self, text: TextSpec) -> Self {
        self.style.max_lines = text.max_lines;
        self.style.ellipsis = text.ellipsis;
        self.text = Some(text);
        self
    }
}

/// A width token accepted by [`Element::max_width`].
pub trait MaxWidthToken {
    /// Applies the token to a style.
    fn apply(self, style: &mut Style);
}

impl MaxWidthToken for Spacing {
    fn apply(self, style: &mut Style) {
        style.max_width = Some(self);
    }
}

impl MaxWidthToken for ContentWidth {
    fn apply(self, style: &mut Style) {
        style.content_width = Some(self);
    }
}

/// A plain text element.
#[must_use]
pub fn text<Msg>(content: impl Into<Arc<str>>) -> Element<Msg> {
    let mut element = Element::new(NodeKind::Text);
    element.text = Some(TextSpec::plain(content));
    element
}

/// A rich text element.
#[must_use]
pub fn text_spans<Msg>(spans: impl IntoIterator<Item = Span>) -> Element<Msg> {
    let mut element = Element::new(NodeKind::Text);
    element.text = Some(TextSpec {
        spans: spans.into_iter().collect(),
        max_lines: None,
        ellipsis: Ellipsis::None,
    });
    element
}

/// A flattened view tree.
#[derive(Clone, Debug)]
pub struct Tree<Msg> {
    /// The arena, in pre-order.
    pub nodes: Vec<Node<Msg>>,
    /// The root, when the view had one element.
    pub root: Option<NodeId>,
    /// A hash of the whole view.
    pub view_hash: u64,
    /// A hash of the parent/child structure only.
    pub structure_hash: u64,
}

impl<Msg> Tree<Msg> {
    /// Builds a tree from a root element.
    #[must_use]
    pub fn build(root: &Element<Msg>) -> Self
    where
        Msg: Clone,
    {
        let mut tree = Self {
            nodes: Vec::new(),
            root: None,
            view_hash: 0,
            structure_hash: 0,
        };
        tree.rebuild(root);
        tree
    }

    /// Rebuilds in place, reusing the arena's capacity.
    pub fn rebuild(&mut self, root: &Element<Msg>)
    where
        Msg: Clone,
    {
        self.nodes.clear();
        let id = self.build_element(root, None, 0, 0, 0);
        self.root = Some(id);
        self.view_hash = self.nodes[id.index()].content_hash;
        let mut structure = Fnv::new();
        for node in &self.nodes {
            structure.write_u32(node.id.0);
            structure.write_u32(node.parent.map_or(u32::MAX, |parent| parent.0));
            structure.write_u32(node.children.len() as u32);
            for child in &node.children {
                structure.write_u32(child.0);
            }
        }
        self.structure_hash = structure.finish();
    }

    fn build_element(
        &mut self,
        element: &Element<Msg>,
        parent: Option<NodeId>,
        depth: u16,
        sibling_index: u32,
        parent_path: u64,
    ) -> NodeId
    where
        Msg: Clone,
    {
        let id = NodeId(self.nodes.len() as u32);
        let mut path = Fnv(parent_path ^ 0x9e37_79b9_7f4a_7c15);
        if let Some(key) = &element.key {
            key.hash_into(&mut path);
        } else {
            path.write_u8(0);
            path.write_u32(sibling_index);
        }
        let path_hash = path.finish();
        self.nodes.push(Node {
            id,
            kind: element.kind,
            style: element.style.clone(),
            text: element.text.clone(),
            key: element.key.clone(),
            action: element.action.clone(),
            interactive: element.style.interactive || element.action.is_some(),
            parent,
            children: Vec::with_capacity(element.children.len()),
            depth,
            path_hash,
            content_hash: 0,
            sibling_index,
            image: element.image,
        });

        let mut children = Vec::with_capacity(element.children.len());
        for (index, child) in element.children.iter().enumerate() {
            children.push(self.build_element(child, Some(id), depth + 1, index as u32, path_hash));
        }

        let mut hash = Fnv::new();
        hash.write_u8(kind_discriminant(element.kind));
        hash_style(&element.style, &mut hash);
        if let Some(text) = &element.text {
            hash.write_u8(1);
            hash_text(text, &mut hash);
        } else {
            hash.write_u8(0);
        }
        if let Some(key) = &element.key {
            hash.write_u8(1);
            key.hash_into(&mut hash);
        } else {
            hash.write_u8(0);
        }
        match element.image {
            Some(image) => {
                hash.write_u8(1);
                hash.write_u64(image);
            }
            None => hash.write_u8(0),
        }
        hash.write_u32(children.len() as u32);
        for child in &children {
            hash.write_u64(self.nodes[child.index()].content_hash);
        }
        let node = &mut self.nodes[id.index()];
        node.children = children;
        node.content_hash = hash.finish();
        id
    }

    /// The root node.
    #[must_use]
    pub fn root(&self) -> Option<&Node<Msg>> {
        self.root.map(|id| &self.nodes[id.index()])
    }

    /// A node by id.
    #[must_use]
    pub fn node(&self, id: NodeId) -> Option<&Node<Msg>> {
        self.nodes.get(id.index())
    }

    /// A node by id, mutably.
    #[must_use]
    pub fn node_mut(&mut self, id: NodeId) -> Option<&mut Node<Msg>> {
        self.nodes.get_mut(id.index())
    }

    /// The number of nodes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the tree is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Finds a node whose static key equals `key`.
    #[must_use]
    pub fn find_key(&self, key: &str) -> Option<NodeId> {
        self.nodes
            .iter()
            .find(|node| matches!(&node.key, Some(Key::Static(name)) if *name == key))
            .map(|node| node.id)
    }

    /// The stable debug path of a node (`start/recent/0`, `start/header`).
    #[must_use]
    pub fn path(&self, id: NodeId) -> String {
        let mut parts = Vec::new();
        let mut current = Some(id);
        while let Some(node_id) = current {
            let node = &self.nodes[node_id.index()];
            parts.push(match &node.key {
                Some(key) => key.display(),
                None => format!("#{}", node.sibling_index),
            });
            current = node.parent;
        }
        parts.reverse();
        parts.join("/")
    }
}

/// Hashes an element without building the arena.
///
/// The frame loop uses it to skip a rebuild when the view is unchanged: the
/// hash is over the same fields as [`Tree::build`], so a match means the arena
/// would be identical.
#[must_use]
pub fn hash_element<Msg>(element: &Element<Msg>) -> u64 {
    let mut hash = Fnv::new();
    hash.write_u8(kind_discriminant(element.kind));
    hash_style(&element.style, &mut hash);
    match &element.text {
        Some(text) => {
            hash.write_u8(1);
            hash_text(text, &mut hash);
        }
        None => hash.write_u8(0),
    }
    match &element.key {
        Some(key) => {
            hash.write_u8(1);
            key.hash_into(&mut hash);
        }
        None => hash.write_u8(0),
    }
    match element.image {
        Some(image) => {
            hash.write_u8(1);
            hash.write_u64(image);
        }
        None => hash.write_u8(0),
    }
    hash.write_u32(element.children.len() as u32);
    for child in &element.children {
        hash.write_u64(hash_element(child));
    }
    hash.finish()
}

fn kind_discriminant(kind: NodeKind) -> u8 {
    match kind {
        NodeKind::Column => 0,
        NodeKind::Row => 1,
        NodeKind::Stack => 2,
        NodeKind::Grid => 3,
        NodeKind::Center => 4,
        NodeKind::Spacer => 5,
        NodeKind::Divider => 6,
        NodeKind::Scroll => 7,
        NodeKind::Wrap => 8,
        NodeKind::Split => 9,
        NodeKind::Aspect => 10,
        NodeKind::Text => 11,
        NodeKind::Image => 12,
        NodeKind::Canvas => 13,
    }
}

fn hash_style(style: &Style, hash: &mut Fnv) {
    hash.write_u8(match style.direction {
        Direction::Row => 0,
        Direction::Column => 1,
    });
    for edge in [
        style.padding.top,
        style.padding.right,
        style.padding.bottom,
        style.padding.left,
    ] {
        hash.write_u32(edge as u32);
    }
    hash.write_u32(style.gap as u32);
    hash.write_u8(match style.align {
        Align::Start => 0,
        Align::Center => 1,
        Align::End => 2,
        Align::Stretch => 3,
        Align::Baseline => 4,
    });
    hash.write_u8(match style.justify {
        Justify::Start => 0,
        Justify::Center => 1,
        Justify::End => 2,
        Justify::SpaceBetween => 3,
    });
    hash.write_f32(style.grow);
    hash.write_f32(style.shrink);
    hash_size_spec(style.width, hash);
    hash_size_spec(style.height, hash);
    for value in [
        style.min_width,
        style.min_height,
        style.max_width,
        style.max_height,
    ] {
        match value {
            Some(value) => {
                hash.write_u8(1);
                hash.write_u32(value as u32);
            }
            None => hash.write_u8(0),
        }
    }
    match style.content_width {
        Some(value) => {
            hash.write_u8(1);
            hash.write_u8(content_width_discriminant(value));
        }
        None => hash.write_u8(0),
    }
    for role in [style.bg, style.border] {
        match role {
            Some(role) => {
                hash.write_u8(1);
                hash.write_u32(role_discriminant(role));
            }
            None => hash.write_u8(0),
        }
    }
    hash.write_u32(border_width_discriminant(style.border_width));
    hash.write_u32(radius_discriminant(style.radius));
    hash.write_u8(style.elevation.level());
    hash.write_f32(style.opacity);
    hash.write_bool(style.clip);
    hash.write_bool(style.hidden);
    hash.write_u32(tone_discriminant(style.tone));
    hash.write_u32(type_role_discriminant(style.text_role));
    hash.write_bool(style.tabular);
    match style.max_lines {
        Some(lines) => {
            hash.write_u8(1);
            hash.write_u32(u32::from(lines));
        }
        None => hash.write_u8(0),
    }
    hash.write_u8(match style.ellipsis {
        Ellipsis::None => 0,
        Ellipsis::End => 1,
        Ellipsis::Middle => 2,
    });
    match style.aspect {
        Some(ratio) => {
            hash.write_u8(1);
            hash.write_f32(ratio);
        }
        None => hash.write_u8(0),
    }
    match style.columns {
        Some(columns) => {
            hash.write_u8(1);
            match columns {
                Columns::Fixed(count) => {
                    hash.write_u8(0);
                    hash.write_u32(u32::from(count));
                }
                Columns::AutoFill(spacing) => {
                    hash.write_u8(1);
                    hash.write_u32(spacing as u32);
                }
            }
        }
        None => hash.write_u8(0),
    }
    hash.write_bool(style.wrap);
    hash.write_u8(match style.scroll {
        ScrollAxes::None => 0,
        ScrollAxes::Vertical => 1,
        ScrollAxes::Horizontal => 2,
        ScrollAxes::Both => 3,
    });
    hash.write_bool(style.interactive);
    hash.write_bool(style.disabled);
    match &style.tooltip {
        Some(tooltip) => {
            hash.write_u8(1);
            hash.write_str(&tooltip.text);
        }
        None => hash.write_u8(0),
    }
}

fn hash_size_spec(spec: SizeSpec, hash: &mut Fnv) {
    match spec {
        SizeSpec::Auto => hash.write_u8(0),
        SizeSpec::Hug => hash.write_u8(1),
        SizeSpec::Fill => hash.write_u8(2),
        SizeSpec::Fraction(value) => {
            hash.write_u8(3);
            hash.write_f32(value);
        }
        SizeSpec::Token(spacing) => {
            hash.write_u8(4);
            hash.write_u32(spacing as u32);
        }
        SizeSpec::Px(value) => {
            hash.write_u8(5);
            hash.write_f32(value);
        }
    }
}

fn hash_text(text: &TextSpec, hash: &mut Fnv) {
    hash.write_u32(text.spans.len() as u32);
    for span in &text.spans {
        hash.write_str(&span.text);
        match span.tone {
            Some(tone) => {
                hash.write_u8(1);
                hash.write_u32(tone_discriminant(tone));
            }
            None => hash.write_u8(0),
        }
        match span.role {
            Some(role) => {
                hash.write_u8(1);
                hash.write_u32(type_role_discriminant(role));
            }
            None => hash.write_u8(0),
        }
        hash.write_bool(span.strong);
        hash.write_bool(span.mono);
        hash.write_bool(span.link);
    }
    match text.max_lines {
        Some(lines) => {
            hash.write_u8(1);
            hash.write_u32(u32::from(lines));
        }
        None => hash.write_u8(0),
    }
    hash.write_u8(match text.ellipsis {
        Ellipsis::None => 0,
        Ellipsis::End => 1,
        Ellipsis::Middle => 2,
    });
}

fn content_width_discriminant(value: ContentWidth) -> u8 {
    match value {
        ContentWidth::Sm => 0,
        ContentWidth::Md => 1,
        ContentWidth::Lg => 2,
        ContentWidth::Xl => 3,
        ContentWidth::Prose => 4,
    }
}

fn role_discriminant(role: ColorRole) -> u32 {
    ColorRole::ALL
        .iter()
        .position(|candidate| *candidate == role)
        .unwrap_or(0) as u32
}

fn tone_discriminant(tone: Tone) -> u32 {
    match tone {
        Tone::Default => 0,
        Tone::Muted => 1,
        Tone::Faint => 2,
        Tone::Accent => 3,
        Tone::Success => 4,
        Tone::Warning => 5,
        Tone::Danger => 6,
        Tone::Info => 7,
    }
}

fn type_role_discriminant(role: TypeRole) -> u32 {
    role.index() as u32
}

fn radius_discriminant(radius: Radius) -> u32 {
    Radius::ALL
        .iter()
        .position(|candidate| *candidate == radius)
        .unwrap_or(0) as u32
}

fn border_width_discriminant(width: BorderWidth) -> u32 {
    match width {
        BorderWidth::None => 0,
        BorderWidth::Hairline => 1,
        BorderWidth::Regular => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::{text, Element, Tree};
    use crate::style::{SizeSpec, Tone};
    use crate::theme::{Radius, Spacing, TypeRole};

    #[derive(Clone, Debug, PartialEq)]
    enum Msg {
        Open(u64),
    }

    fn view() -> Element<Msg> {
        Element::column()
            .key("start")
            .pad(Spacing::S6)
            .gap(Spacing::S4)
            .child(text::<Msg>("Motioner").text_role(TypeRole::Title))
            .child(
                Element::row()
                    .key("actions")
                    .gap(Spacing::S2)
                    .child(
                        text::<Msg>("Open")
                            .key("open")
                            .on_click(Msg::Open(1))
                            .radius(Radius::Sm)
                            .bg(crate::theme::ColorRole::AccentFill)
                            .tone(Tone::Default),
                    )
                    .child(Element::spacer()),
            )
    }

    #[test]
    fn flattens_in_preorder_with_stable_paths() {
        let tree = Tree::build(&view());
        assert_eq!(tree.len(), 5);
        let root = tree.root().expect("root");
        assert_eq!(root.kind.name(), "column");
        assert_eq!(tree.path(root.id), "start");
        let actions = tree.find_key("actions").expect("actions");
        assert_eq!(tree.path(actions), "start/actions");
        assert_eq!(tree.path(actions).split('/').count(), 2);
        let open = tree.find_key("open").expect("open");
        assert_eq!(tree.path(open), "start/actions/open");
        assert_eq!(tree.nodes[open.index()].action, Some(Msg::Open(1)));
        assert!(tree.nodes[open.index()].interactive);
    }

    #[test]
    fn hashes_are_stable_and_content_sensitive() {
        let first = Tree::build(&view());
        let second = Tree::build(&view());
        assert_eq!(first.view_hash, second.view_hash);
        let changed = Element::<Msg>::column()
            .key("start")
            .pad(Spacing::S6)
            .gap(Spacing::S8);
        assert_ne!(Tree::build(&changed).view_hash, first.view_hash);
    }

    #[test]
    fn size_escape_hatch_is_explicit() {
        let element = Element::<Msg>::row().width(SizeSpec::Px(13.0));
        assert_eq!(element.style.width, SizeSpec::Px(13.0));
    }
}
