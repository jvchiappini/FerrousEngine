//! Every face's metrics are normalised to em, so a mixed atlas does not mix
//! units even when the faces use different `units_per_em`.
//!
//! `FontParser` divides advances, bounds, kerning and the font metrics by that
//! face's own `units_per_em` (`parser.rs:1327`, `:1352`, `:675`, `:647`), so
//! `LayoutData` metrics are in em and `layout` only multiplies by the point
//! size. This test proves it with a real system face whose units differ from
//! Roboto's 2048.

use ferrous_font::baked::BakedAtlas;
use ferrous_font::layout_data::LayoutData;
use ferrous_font::parser::FontParser;
use ferrous_font::system;

const ROBOTO: &[u8] = include_bytes!("../../../assets/fonts/Roboto-Regular.ttf");
const GLYPH: char = 'H';

fn advance_em(bytes: &[u8]) -> Option<(u16, f32, f32)> {
    let parser = FontParser::new(bytes.to_vec()).ok()?;
    let index = parser.get_glyph_index(GLYPH);
    if index == 0 {
        return None;
    }
    let data = LayoutData::from_bytes(bytes, [GLYPH]).ok()?;
    let advance = data.metrics().get(&GLYPH)?.advance;
    Some((
        parser.units_per_em,
        parser.get_glyph_advance_by_index(index),
        advance,
    ))
}

#[test]
fn a_face_with_a_different_units_per_em_stays_in_em_units() {
    let Some((roboto_upem, roboto_parser, roboto_advance)) = advance_em(ROBOTO) else {
        panic!("Roboto must parse");
    };
    assert_eq!(roboto_upem, 2048);
    assert_eq!(roboto_parser, roboto_advance, "parser advance is em");

    let differing = system::font_files().iter().find_map(|path| {
        let bytes = std::fs::read(path).ok()?;
        let (upem, parser_advance, advance) = advance_em(&bytes)?;
        (upem != roboto_upem).then_some((upem, parser_advance, advance, bytes))
    });
    let Some((upem, parser_advance, advance, bytes)) = differing else {
        eprintln!("skipping: this host has no system glyf face with another units_per_em");
        return;
    };
    assert_ne!(upem, roboto_upem);
    assert_eq!(parser_advance, advance, "LayoutData keeps the em advance");
    // An em advance is a fraction of the em, not a raw font unit (which would
    // be hundreds or thousands).
    assert!(
        (0.05..=2.0).contains(&advance),
        "advance {advance} for a {upem} upem face is not in em"
    );

    // The composite atlas copies the owning face's already-em metrics, so the
    // mixed atlas holds the fallback's advance unchanged (no second scaling by
    // the primary's units).
    let primary = FontParser::new(ROBOTO.to_vec()).expect("roboto");
    let fallback = FontParser::new(bytes.clone()).expect("fallback");
    let faces: Vec<(&FontParser, Vec<char>)> =
        vec![(&primary, Vec::new()), (&fallback, vec![GLYPH])];
    let atlas = BakedAtlas::build_multi(&faces, 32).expect("bake");
    let composite = atlas.metrics.get(&GLYPH).expect("glyph").advance;
    assert_eq!(
        composite, advance,
        "the composite must not re-scale a fallback advance"
    );
}
