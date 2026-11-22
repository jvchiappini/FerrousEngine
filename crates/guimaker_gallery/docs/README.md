# GUIMaker gallery

The gallery is the runnable face of GUIMaker:

- it hosts named **stories** built from the real view tree and theme;
- it renders any story to a **PNG** at a chosen logical size, scale and theme,
  headlessly, through the real GUI renderer;
- it keeps **tolerance goldens** in the repository and compares them with a
  per-channel and pixel-count budget;
- it provides the `FerrousApp` **host adapter** so an ordinary windowed app can
  adopt GUIMaker in about one screen of glue.

Start with the CLI:

```text
cargo run -p guimaker_gallery -- stories
cargo run -p guimaker_gallery -- shot start --size 1280x800 --scale 1.5 \
    --theme light --out out/p5-start-light.png
```

The full command reference, the input mapping, the golden tolerance and the
documented fallbacks are in [gallery.md](gallery.md).

The windowed demo is `cargo run -p guimaker_gallery --example host`.
