# FerrousEngine

This repository contains the source code for **FerrousEngine**, a
lightweight Rust-based game engine built around modular crates such as
`ferrous_core`, `ferrous_renderer`, `ferrous_gui`, and others.

The documentation website is generated with [MkDocs](https://www.mkdocs.org/)
using the Material theme.  Markdown from each crate is aggregated into
`docs/` by `scripts/build_docs.sh` and then rendered into `site/`.

All generation and publishing is handled by GitHub Actions – the
`build_docs.yml` workflow executes the script, builds the site using
MkDocs, and deploys it directly to GitHub Pages. You never need to run
the script locally; the action will pick up changes to any crate's markdown
and keep the published site in sync. Local invocation of `scripts/build_docs.sh`
is only useful for previewing the output before pushing.

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

The documentation is automatically built and deployed using GitHub Actions.
To ensure this works, check your repository settings:
1. Go to **Settings > Pages**.
2. Under "Build and deployment", set **Source** to **GitHub Actions**.

For development or local preview you can run `mkdocs serve` from the
workspace root once the prerequisites are installed. (The `site/` and `docs/`
directories are ignored by git).

---

Refer to the individual crate directories for design notes and examples.
