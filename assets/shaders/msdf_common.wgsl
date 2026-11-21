// ─── Shared MSDF helpers ────────────────────────────────────────────────────
//
// Included (by string concatenation) by every shader that decodes a
// multi-channel signed-distance-field atlas: the GUI text shader and the Pure2D
// text shader.  Keep this file free of bindings, structs and entry points so it
// can be prepended to any WGSL module.

/// Median of the three MSDF channels.
///
/// The generator enables at least two channels per outline edge, so the median
/// recovers a single smooth distance that keeps sharp corners while still
/// anti-aliasing straight edges.
fn msdf_median(r: f32, g: f32, b: f32) -> f32 {
    return max(min(r, g), min(max(r, g), b));
}

/// Atlas cell size in texels for the glyph at `uv0..uv1`.
///
/// Works for both the font atlas and the icon atlas without a uniform: the UV
/// span is multiplied by the sampled texture's dimensions.
fn msdf_cell_texels(uv0: vec2<f32>, uv1: vec2<f32>, atlas_dim: vec2<f32>) -> f32 {
    return max(
        (uv1.x - uv0.x) * atlas_dim.x,
        (uv1.y - uv0.y) * atlas_dim.y,
    );
}

/// Distance-field range in screen pixels (Chlumsky's `screenPxRange`).
///
/// `quad_size_px` is the glyph quad size in screen pixels, `cell_texels` the
/// atlas cell size in texels and `px_range` the generator's encoded range (in
/// atlas texels, see `ferrous_font::FontAtlas::px_range`).  Never returns less
/// than 1 so anti-aliasing cannot collapse.
fn msdf_screen_px_range(
    quad_size_px: vec2<f32>,
    cell_texels: f32,
    px_range: f32,
) -> f32 {
    return max(px_range * (quad_size_px.x / max(cell_texels, 1.0)), 1.0);
}

/// MSDF coverage from the reconstructed distance.
///
/// Our generator encodes `inside = negative` (`sd < 0.5` inside); the canonical
/// msdfgen shader assumes the opposite sign, so the distance is measured as
/// `0.5 - sd` here.
fn msdf_opacity(sig_dist: f32, screen_px_range: f32) -> f32 {
    let screen_px_distance = screen_px_range * (0.5 - sig_dist);
    return clamp(screen_px_distance + 0.5, 0.0, 1.0);
}
