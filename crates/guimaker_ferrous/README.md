# guimaker_ferrous

`GUIMaker`'s FerrousEngine backend. It maps the engine-agnostic
`guimaker::display_list::DisplayList` onto `ferrous_ui_render::GuiBatch` and
implements `guimaker::text::TextSystem` with the real `ferrous_font` layout
tables.

The crate draws nothing on its own: it is a mapping, and every gap the pinned
engine cannot fill is a documented fallback (see `docs/backend.md`).

## Public API

```rust
use guimaker::display_list::DisplayList;
use guimaker::geom::Size;
use ferrous_engine::DrawContext;
use guimaker_ferrous::{encode, FerrousTextSystem, GuiMakerRenderer, TextureMap};

// Stateless: images without a registered texture are skipped.
pub fn encode(
    display_list: &DisplayList,
    viewport: Size,
    scale: f32,
    dc: &mut DrawContext<'_, '_>,
);

// Stateful: owns the image registry and the layer-opacity stack.
impl GuiMakerRenderer {
    pub fn new() -> Self;
    pub fn with_textures(textures: TextureMap) -> Self;
    pub fn textures(&self) -> &TextureMap;
    pub fn textures_mut(&mut self) -> &mut TextureMap;
    pub fn encode(
        &mut self,
        display_list: &DisplayList,
        viewport: Size,
        scale: f32,
        dc: &mut DrawContext<'_, '_>,
    );
}

// Image source ids for DrawCommand::Image { source: u64 }.
impl TextureMap {
    pub fn insert(&mut self, source: u64, texture: std::sync::Arc<ferrous_assets::Texture2d>)
        -> Option<std::sync::Arc<ferrous_assets::Texture2d>>;
    pub fn get(&self, source: u64) -> Option<&std::sync::Arc<ferrous_assets::Texture2d>>;
    pub fn remove(&mut self, source: u64) -> Option<std::sync::Arc<ferrous_assets::Texture2d>>;
}

// The TextSystem a host passes to Ui::paint.
pub struct FerrousTextSystem;
impl FerrousTextSystem {
    pub fn new() -> Self;
    pub fn with_source(source: Box<dyn GlyphSource>) -> Self;
}
```

`GuiMakerRenderer::encode` is the recommended entry point when a frame can
contain images or layers. `encode` is the one-call convenience for static
display lists.

## Text

`FerrousTextSystem` builds a GPU-free `ferrous_font::LayoutData` per
`(FontFamily, FontWeight)` face and shapes with real advances, `kern`/`GPOS`
pair kerning, tracking and line wrapping. `BundledGlyphs` returns the engine's
`Roboto-Regular` for every face; implement `GlyphSource` to supply the real
Inter and JetBrains Mono files per weight. The system never creates a GPU
atlas.

The backend draws the shaped `TextRun` line by line, using the engine font in
`DrawContext::font`. Runs with spans or letter tracking are emitted glyph by
glyph so each character gets its span colour and the tracking pitch; plain runs
use `GuiBatch::draw_text_baseline`.

## Tests

```text
cargo test -p guimaker_ferrous
```

The mapping tests run without a GPU on a plain `GuiBatch` (shapes, borders,
multiple shadows, gradients, clips, layers, images, icons and text via a fake
`BaselineGlyphs`). The text tests shape real text with the bundled font.

## Engine pin

The backend targets FerrousEngine `dd382bd`. Rounded clip shapes, a real
backdrop blur and gradient borders are engine gap-lane work and are not present
at that pin; the adapter uses the rectangular clip and skips backdrop blur.
See `docs/backend.md` for the full mapping and fallback list.
