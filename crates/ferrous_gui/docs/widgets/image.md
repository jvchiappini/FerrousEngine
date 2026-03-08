# Image widget

Displays a textured rectangle inside the UI.

This widget is only available when the `assets` feature of `ferrous_gui` is
enabled.  Under the hood it simply wraps a handle to a
[`ferrous_assets::Texture2d`](../../ferrous_assets/src/texture.rs) and
emits a `RenderCommand::Image` that is later converted by the GUI renderer
into a textured quad.

## Example

```rust
use ferrous_gui::{Ui, Image};
use std::sync::Arc;

let texture: Arc<ferrous_assets::Texture2d> = /* load from asset manager */;
let mut ui = Ui::new();
let img = Image::new([10.0, 10.0, 64.0, 64.0], texture.clone())
    .with_uv([0.0,0.0],[1.0,1.0])        // optional subregion
    .with_color([1.0,1.0,1.0,1.0]);      // tint
ui.add(img);
```

The batch API also exposes helpers when `assets` is enabled:

```rust
quad_batch.image(10.0, 10.0, 64.0, 64.0, texture.clone(),
                 [0.0,0.0],[1.0,1.0], [1.,1.,1.,1.]);
```

### SVG icons

If the `svg` feature of `ferrous_assets` is enabled you can rasterize an
SVG file on the fly and create an `Image` widget in one step:

```rust
let img = Image::from_svg_file([0.0,0.0,32.0,32.0],
    "assets/icons/star.svg", &device, &queue, 32, 32)?;
ui.add(img);
```

The texture is generated at the requested pixel dimensions using the
resvg/tiny-skia pipeline and cached in the asset system like any other
texture.  This is convenient for button icons and other resolution‑independent
graphics.

Texture slots are automatically deduplicated per-batch; attempting to draw
more than `MAX_TEXTURE_SLOTS` (currently 8) will panic.
