# FerrousEngine

This repository contains the source code for **FerrousEngine**, a
lightweight Rust-based game engine built around modular crates such as
`ferrous_core`, `ferrous_renderer`, `ferrous_gui`, and others.

The documentation website is generated with [MkDocs](https://www.mkdocs.org/)
using the Material theme.  Markdown from each crate is aggregated into
`docs/` by `scripts/build_docs.sh` and then rendered into `site/`.

On the public repository a GitHub Actions workflow (see
`.github/workflows/build_docs.yml`) automatically runs the build and
publishes the resulting `site/` contents to GitHub Pages.  You don't
need to run the script locally unless you want to preview changes on
your own machine.

```sh
cd c:\Users\jvchi\CARPETAS\FerrousEngine
scripts\build_docs.sh
```

### Documentation structure

- `docs/index.md` &ndash; this file, used as the homepage for the
  generated site.  It is automatically created by the build script from
  this README.
- `docs/ferrous_gui` &ndash; API docs and widget references for the GUI
  crate (copied from `crates/ferrous_gui/docs`).

Additional crates may add their own `docs/` directory and the script
will include them automatically.

### GitHub Pages configuration

By default the project has historically deployed the generated website
to a dedicated `gh-pages` branch.  It's perfectly fine to continue
doing that, but if you prefer to serve directly from `main` you can
change the repository settings on GitHub to publish from the `main`
branch and the `docs/` folder.  After doing so you no longer need to
push the `gh-pages` branch; `scripts/build_docs.sh` will still create
the `docs/` tree which GitHub Pages will serve.

> ⚠️ Remember to run the build script and commit the resulting
> `docs/` files before pushing to `main` when using that configuration.

For development or local preview you can run `mkdocs serve` from the
workspace root once the prerequisites are installed.

---

Refer to the individual crate directories for design notes and examples.
