//! Renders the Phase 8B evidence shots: the mixed-script line at 1.0, 1.5 and
//! 2.0, and a dropdown anchored 20 px from the right edge.
//!
//! Run with `cargo run --release -p guimaker_gallery --example p8b_shots`.

use guimaker::components::text as display_text;
use guimaker::geom::Size;
use guimaker::prelude::*;
use guimaker::theme::{ColorRole, Theme};
use guimaker::view::Element;
use guimaker_ferrous::FerrousTextSystem;
use guimaker_gallery::shot::{render_display_list, write_png};

fn paint(
    view: &Element<()>,
    viewport: Size,
    scale: f32,
    theme: &Theme,
) -> guimaker::display_list::DisplayList {
    let mut ui: Ui<()> = Ui::new(theme.clone());
    ui.set_viewport(viewport, scale);
    let _ = ui.set_time_ms(0.0);
    let mut text = FerrousTextSystem::new();
    text.set_fallback_chain(theme.font_fallbacks.clone());
    let mut frame = Frame::new();
    ui.paint(view, &mut frame, &mut text);
    frame.display_list
}

fn mixed_scripts() -> Element<()> {
    Element::column()
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .pad(Spacing::S6)
        .gap(Spacing::S4)
        .bg(ColorRole::Bg)
        .child(display_line(
            "Intro \u{1F3AC} \u{52D5}\u{753B} \u{0629}\u{0628}\u{0631}\u{062C}\u{062A} \u{2713}",
        ))
        .child(display_line(
            "Fallback: CJK, Arabic and symbols resolve per glyph",
        ))
}

fn display_line(content: &str) -> Element<()> {
    Element::row().child(display_text::<()>(
        content,
        TypeRole::Heading,
        Tone::Default,
    ))
}

fn edge_menu(big: bool) -> Element<()> {
    let width = if big { 990.0 } else { 120.0 };
    Element::stack()
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .bg(ColorRole::Bg)
        .child(
            Element::row()
                .width(SizeSpec::Px(1000.0))
                .height(SizeSpec::Px(300.0))
                .justify(Justify::End)
                .pad_x(Spacing::S5)
                .child(
                    Element::overlay()
                        .child(
                            Element::row()
                                .width(SizeSpec::Px(80.0))
                                .height(SizeSpec::Px(32.0))
                                .bg(ColorRole::AccentFill),
                        )
                        .child(
                            Element::column()
                                .width(SizeSpec::Px(width))
                                .height(SizeSpec::Px(160.0))
                                .pad(Spacing::S2)
                                .gap(Spacing::S1)
                                .bg(ColorRole::SurfaceRaised)
                                .border(ColorRole::Border)
                                .radius(Radius::Sm)
                                .child(display_text::<()>("Rename", TypeRole::Label, Tone::Default))
                                .child(display_text::<()>(
                                    "Duplicate",
                                    TypeRole::Label,
                                    Tone::Default,
                                ))
                                .child(display_text::<()>("Delete", TypeRole::Label, Tone::Default))
                                .overlay_on(0, guimaker::overlay::Side::Bottom, 4.0),
                        ),
                ),
        )
}

fn main() {
    if let Err(error) = run() {
        eprintln!("p8b shots failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let theme = Theme::dark();
    let out = std::path::Path::new("out/shots/p8b");
    std::fs::create_dir_all(out)?;
    for (name, scale) in [("1.0", 1.0_f32), ("1.5", 1.5), ("2.0", 2.0)] {
        let viewport = Size::new(720.0, 160.0);
        let list = paint(&mixed_scripts(), viewport, scale, &theme);
        let image = render_display_list(&list, &theme, viewport, scale)?;
        let path = out.join(format!("mixed-scripts-{name}.png"));
        write_png(&path, &image)?;
        println!("{}", std::fs::canonicalize(&path)?.display());
    }
    let viewport = Size::new(1000.0, 300.0);
    let list = paint(&edge_menu(true), viewport, 1.0, &theme);
    let image = render_display_list(&list, &theme, viewport, 1.0)?;
    let path = out.join("menu-popup-near-edge.png");
    write_png(&path, &image)?;
    println!("{}", std::fs::canonicalize(&path)?.display());
    let list = paint(&edge_menu(false), viewport, 1.0, &theme);
    let image = render_display_list(&list, &theme, viewport, 1.0)?;
    let path = out.join("menu-in-window.png");
    write_png(&path, &image)?;
    println!("{}", std::fs::canonicalize(&path)?.display());
    Ok(())
}
