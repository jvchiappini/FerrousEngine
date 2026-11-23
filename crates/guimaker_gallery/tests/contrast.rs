//! Display-list contrast and chip-fit checks.
//!
//! A label is only correct if it is readable on the fill actually painted behind
//! it. This walks the text runs that own a filled rectangle (the start recipe's
//! primary action and the component board's chips and actions), composites a
//! translucent wash over the theme backdrop, and applies the theme's contrast
//! guarantee for the role the label resolved to. It also checks that every such
//! run sits inside its pill.

use guimaker::display_list::{DisplayList, DrawCommand};
use guimaker::geom::{Rect, Size};
use guimaker::recipe::start_screen;
use guimaker::text::MetricsTextSystem;
use guimaker::theme::{Color, ColorRole, Theme};
use guimaker::view::{Element, NodeId};
use guimaker::{Frame, Ui};
use guimaker_gallery::story::Story;
use guimaker_gallery::Component;

const EPSILON: f32 = 1e-3;

fn paint<Msg: Clone>(view: &Element<Msg>, theme: &Theme) -> DisplayList {
    let mut ui: Ui<Msg> = Ui::new(theme.clone());
    ui.set_viewport(Size::new(1280.0, 800.0), 1.5);
    let _ = ui.set_time_ms(0.0);
    let mut text_system = MetricsTextSystem::new();
    let mut frame = Frame::new();
    ui.paint(view, &mut frame, &mut text_system);
    frame.display_list
}

fn role_of(theme: &Theme, color: Color) -> ColorRole {
    ColorRole::ALL
        .into_iter()
        .find(|role| theme.role(*role) == color)
        .expect("every painted text colour is a theme role")
}

/// The minimum ratio the theme promises for the role the label resolved to.
fn min_contrast(role: ColorRole) -> f32 {
    match role {
        ColorRole::Text => 7.0,
        ColorRole::TextMuted | ColorRole::TextOnAccent | ColorRole::TextOnAccentFill => 4.5,
        // `text_faint` and the saturated semantic roles are secondary labels;
        // the theme promises 3:1 for them on their wash.
        _ => 3.0,
    }
}

/// A straight-alpha composite of a linear colour over an opaque backdrop.
fn over(foreground: Color, backdrop: Color) -> Color {
    let alpha = foreground.alpha();
    let front = foreground.to_linear();
    let back = backdrop.to_linear();
    Color::lineara(
        front[0] * alpha + back[0] * (1.0 - alpha),
        front[1] * alpha + back[1] * (1.0 - alpha),
        front[2] * alpha + back[2] * (1.0 - alpha),
        1.0,
    )
}

fn fill_behind(list: &DisplayList, index: usize, node: NodeId) -> Option<(Rect, Color)> {
    list.commands[..index]
        .iter()
        .rev()
        .find_map(|command| match command {
            DrawCommand::Rect {
                node: owner,
                rect,
                color,
                ..
            } if *owner == node => Some((*rect, *color)),
            _ => None,
        })
}

fn inside(inner: Rect, outer: Rect) -> bool {
    inner.x >= outer.x - EPSILON
        && inner.y >= outer.y - EPSILON
        && inner.x + inner.width <= outer.x + outer.width + EPSILON
        && inner.y + inner.height <= outer.y + outer.height + EPSILON
}

fn check_labels(name: &str, list: &DisplayList, theme: &Theme, backdrop: Color) -> usize {
    let mut checked = 0;
    for (index, command) in list.commands.iter().enumerate() {
        let DrawCommand::Text {
            node, rect, run, ..
        } = command
        else {
            continue;
        };
        let Some((fill_rect, fill)) = fill_behind(list, index, *node) else {
            continue;
        };
        let surface = if fill.alpha() < 1.0 {
            over(fill, backdrop)
        } else {
            fill
        };
        let color = run.spans.first().map_or(Color::WHITE, |span| span.color);
        // Text and TextOnAccentFill share a colour in the dark palette; the
        // filled-action pairing is the role that owns the label there.
        let role = if color == theme.role(ColorRole::TextOnAccentFill) {
            ColorRole::TextOnAccentFill
        } else {
            role_of(theme, color)
        };
        let ratio = color.contrast_ratio(surface);
        let needed = min_contrast(role);
        assert!(
            ratio >= needed,
            "{name}: {} text is {ratio:.2}:1 on its fill, needs {needed:.1}:1",
            role.name()
        );
        assert!(
            inside(*rect, fill_rect),
            "{name}: the text run {rect:?} escapes its fill {fill_rect:?}"
        );
        checked += 1;
    }
    checked
}

#[test]
fn start_recipe_labels_meet_their_role_threshold() {
    let theme = Theme::light();
    let list = paint(&start_screen(0.0), &theme);
    let checked = check_labels("start-light", &list, &theme, theme.role(ColorRole::Bg));
    assert!(checked >= 2, "checked only {checked} filled labels");
    let primary_is_on_accent = list.commands.iter().any(|command| {
        matches!(
            command,
            DrawCommand::Text { run, .. }
                if run
                    .spans
                    .first()
                    .is_some_and(|span| span.color == theme.colors.text_on_accent_fill)
        )
    });
    assert!(
        primary_is_on_accent,
        "the filled accent label did not use text_on_accent_fill"
    );
}

#[test]
fn component_chips_fit_and_meet_their_role_threshold() {
    let theme = Theme::light();
    let list = Story::Components.display_list(&theme, Size::new(1280.0, 800.0), 1.5);
    let checked = check_labels("components-light", &list, &theme, theme.role(ColorRole::Bg));
    assert!(checked >= 7, "checked only {checked} filled labels");
}

/// Composites every fill that contains a text run, in paint order, then checks
/// the run's role threshold against that actual surface.
fn check_story_labels(name: &str, list: &DisplayList, theme: &Theme) -> usize {
    let mut checked = 0;
    for (index, command) in list.commands.iter().enumerate() {
        let DrawCommand::Text {
            node: _, rect, run, ..
        } = command
        else {
            continue;
        };
        let mut surface = theme.role(ColorRole::Bg);
        for earlier in &list.commands[..index] {
            if let DrawCommand::Rect {
                rect: fill_rect,
                color,
                ..
            } = earlier
            {
                if inside(*rect, *fill_rect) {
                    surface = over(*color, surface);
                }
            }
        }
        let color = run.spans.first().map_or(Color::WHITE, |span| span.color);
        let role = if color == theme.role(ColorRole::TextOnAccentFill) {
            ColorRole::TextOnAccentFill
        } else {
            role_of(theme, color)
        };
        let ratio = color.contrast_ratio(surface);
        let needed = min_contrast(role);
        assert!(
            ratio >= needed,
            "{name}: {} text {ratio:.2}:1 is below {needed:.1}:1 on its composited fill",
            role.name()
        );
        checked += 1;
    }
    assert!(checked > 0, "{name} painted no text to check");
    checked
}

#[test]
fn every_component_story_keeps_its_filled_labels_readable() {
    for component in Component::ALL {
        for theme in [Theme::light(), Theme::dark()] {
            let view = component.view();
            let list = paint(&view, &theme);
            let _ = check_story_labels(component.name(), &list, &theme);
        }
    }
}
