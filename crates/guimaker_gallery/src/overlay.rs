//! A deterministic, GPU-free debug overlay for design-lint findings.
//!
//! The overlay is a pure command builder: it reads the painted [`Ui`] and the
//! findings of one diagnostic lint pass and returns a short list of
//! [`DrawCommand`]s. It never mutates the UI, never allocates a text system and
//! never runs on the normal frame path — a host turns it on for a debugging
//! session and appends the commands to the frame it is about to draw.
//!
//! The vocabulary is deliberately small: an overlap is a red outline, an
//! overflow is an orange one, and any other finding is a strong hairline. The
//! sentence of the element under the cursor is drawn as a real text run, so the
//! overlay reads like a tooltip rather than a second layout.

use std::sync::Arc;

use glam::Vec2;
use guimaker::display_list::{DrawCommand, TextRun, TextSpanStyle};
use guimaker::geom::Rect;
use guimaker::lint::Finding;
use guimaker::text::{Line, TextStyle};
use guimaker::theme::{BorderWidth, ColorRole, Radius, Spacing, Theme, TypeRole};
use guimaker::view::NodeId;
use guimaker::Ui;

/// The node id overlay commands carry.
///
/// The commands do not belong to a layout node, so they use a sentinel that no
/// arena can reach. The adapter only stamps it onto the batch entry.
const OVERLAY_NODE: NodeId = NodeId(u32::MAX);

/// The debug overlay state a host owns.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DebugOverlay {
    /// Whether the overlay draws at all.
    pub enabled: bool,
    /// The pointer position in logical pixels, used for the explanation.
    pub cursor: Vec2,
}

impl DebugOverlay {
    /// Builds a disabled overlay at the origin.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds the overlay commands for `ui` and its `findings`.
    ///
    /// Returns an empty list when the overlay is disabled. Every finding gets
    /// an outline; the finding under [`Self::cursor`] (by path first, then by
    /// rectangle) also gets its sentence drawn next to the pointer.
    #[must_use]
    pub fn commands<Msg: Clone>(&self, ui: &Ui<Msg>, findings: &[Finding]) -> Vec<DrawCommand> {
        if !self.enabled {
            return Vec::new();
        }
        let theme = ui.theme();
        let mut commands = Vec::with_capacity(findings.len() + 1);
        for finding in findings {
            commands.push(outline(ui, theme, finding));
        }
        if let Some((sentence, anchor)) = self.explanation(ui, findings) {
            commands.push(text_command(theme, self.cursor, anchor, sentence));
        }
        commands
    }

    /// The sentence to show and the rect it should sit next to.
    fn explanation<Msg: Clone>(
        &self,
        ui: &Ui<Msg>,
        findings: &[Finding],
    ) -> Option<(String, Rect)> {
        let id = node_at(ui, self.cursor)?;
        let anchor = ui.layout().rect(id);
        let path = ui.tree().path(id);
        let under = findings
            .iter()
            .find(|finding| finding.path == path)
            .or_else(|| {
                findings
                    .iter()
                    .find(|finding| finding.rect.contains(self.cursor))
            })?;
        Some((under.sentence(), anchor))
    }
}

/// One finding's outline command.
fn outline<Msg: Clone>(ui: &Ui<Msg>, theme: &Theme, finding: &Finding) -> DrawCommand {
    let width = theme.borders.resolve(BorderWidth::Regular);
    let corner = theme.radii.resolve(Radius::None, 0.0);
    DrawCommand::Border {
        node: node_for_path(ui, &finding.path),
        rect: finding.rect,
        color: outline_color(theme, finding),
        width,
        sides: [width; 4],
        radii: [corner; 4],
    }
}

/// The outline colour of a rule: overlap red, overflow orange, the rest strong.
fn outline_color(theme: &Theme, finding: &Finding) -> guimaker::theme::Color {
    let name = format!("{:?}", finding.rule).to_ascii_lowercase();
    let role = if name.contains("overlap") {
        ColorRole::Danger
    } else if name.contains("overflow") {
        ColorRole::Warning
    } else {
        ColorRole::BorderStrong
    };
    theme.role(role)
}

/// The readable text run for `sentence`.
fn text_command(theme: &Theme, cursor: Vec2, anchor: Rect, sentence: String) -> DrawCommand {
    let style = TextStyle::from_type(theme.typography.get(TypeRole::Caption), false);
    let offset = theme.spacing(Spacing::S2);
    let length = sentence.len();
    let rect = Rect::new(
        cursor.x + offset,
        cursor.y + offset,
        anchor.width.max(style.line_height),
        style.line_height,
    );
    let line = Line {
        start: 0,
        end: length,
        width: rect.width,
        baseline: style.ascent(),
        glyph_start: 0,
        glyph_end: 0,
    };
    let span = TextSpanStyle {
        start: 0,
        end: length,
        color: theme.role(ColorRole::Text),
        strong: false,
        mono: false,
        link: false,
    };
    DrawCommand::Text {
        node: OVERLAY_NODE,
        rect,
        run: TextRun {
            text: Arc::from(sentence),
            lines: Arc::from([line]),
            style,
            spans: Arc::from([span]),
        },
    }
}

/// The node id for a stable path, or the overlay sentinel when it is gone.
fn node_for_path<Msg: Clone>(ui: &Ui<Msg>, path: &str) -> NodeId {
    ui.tree()
        .nodes
        .iter()
        .find(|node| ui.tree().path(node.id) == path)
        .map_or(OVERLAY_NODE, |node| node.id)
}

/// The deepest visible node containing `point`.
fn node_at<Msg: Clone>(ui: &Ui<Msg>, point: Vec2) -> Option<NodeId> {
    ui.tree()
        .nodes
        .iter()
        .filter(|node| {
            let visible = ui
                .layout()
                .resolved(node.id)
                .is_none_or(|style| !style.hidden);
            visible && ui.layout().rect(node.id).contains(point)
        })
        .max_by_key(|node| node.depth)
        .map(|node| node.id)
}

#[cfg(test)]
mod tests {
    use super::DebugOverlay;
    use glam::Vec2;
    use guimaker::display_list::DrawCommand;
    use guimaker::geom::Size;
    use guimaker::lint::{lint, Finding};
    use guimaker::style::SizeSpec;
    use guimaker::text::MetricsTextSystem;
    use guimaker::theme::Spacing;
    use guimaker::theme::{ColorRole, Theme};
    use guimaker::view::Element;
    use guimaker::{Frame, Ui};

    fn painted(view: &Element<()>) -> Ui<()> {
        let mut ui: Ui<()> = Ui::new(Theme::dark());
        ui.set_viewport(Size::new(800.0, 600.0), 1.0);
        let mut frame = Frame::new();
        let mut text = MetricsTextSystem::new();
        ui.paint(view, &mut frame, &mut text);
        ui
    }

    #[test]
    fn overlap_is_red_and_the_cursor_shows_its_sentence() {
        // A negative gap makes two siblings overlap on purpose. Unlike the
        // avatar group, which declares its deliberate overlap layer, this row
        // does not, so the lint must flag it. The overlay has to outline it in
        // the danger role and the cursor inside the seam has to explain it.
        let view: Element<()> = Element::row()
            .key("row")
            .child(
                Element::row()
                    .key("a")
                    .width(SizeSpec::Token(Spacing::S16))
                    .height(SizeSpec::Token(Spacing::S16)),
            )
            .child(
                Element::row()
                    .key("b")
                    .width(SizeSpec::Token(Spacing::S16))
                    .height(SizeSpec::Token(Spacing::S16))
                    .overlay_to(0, Vec2::new(-8.0, 0.0)),
            );
        let ui = painted(&view);
        let findings = lint(&ui);
        assert!(
            !findings.is_empty(),
            "the negative-gap row must lint:\n{}",
            ui.inspect()
        );

        let a = ui.tree().find_key("a").expect("node a");
        let b = ui.tree().find_key("b").expect("node b");
        let seam = ui.layout().rect(a).intersect(ui.layout().rect(b));
        assert!(!seam.is_empty(), "the seam is empty");
        let cursor = Vec2::new(seam.x + seam.width * 0.5, seam.y + seam.height * 0.5);

        let overlay = DebugOverlay {
            enabled: true,
            cursor,
        };
        let commands = overlay.commands(&ui, &findings);
        let danger = ui.theme().role(ColorRole::Danger);
        assert!(
            commands.iter().any(|command| matches!(
                command,
                DrawCommand::Border { color, .. } if *color == danger
            )),
            "no red outline: {commands:?}"
        );

        let expected: Vec<String> = findings.iter().map(Finding::sentence).collect();
        assert!(
            commands.iter().any(|command| match command {
                DrawCommand::Text { run, .. } => expected
                    .iter()
                    .any(|sentence| run.text.contains(sentence.as_str())),
                _ => false,
            }),
            "no explanation text: {commands:?}"
        );
    }

    #[test]
    fn a_disabled_overlay_draws_nothing() {
        let view: Element<()> = Element::canvas().key("only");
        let ui = painted(&view);
        let findings = lint(&ui);
        assert!(DebugOverlay::default().commands(&ui, &findings).is_empty());
    }
}
