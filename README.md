# FerrousEngine

This repository contains the source code for **FerrousEngine**, a
lightweight Rust-based game engine built around modular crates such as
`ferrous_core`, `ferrous_renderer`, `ferrous_gui`, and others.

The documentation website is generated with [MkDocs](https://www.mkdocs.org/)
using the Material theme.  Markdown from each crate is aggregated into
`docs/` by `scripts/build_docs.sh` and then rendered into `site/`.

All generation and publishing is handled by GitHub Actions – the
`build_docs.yml` workflow executes the script, builds the site, and
commits the resulting `docs/` tree back to `main`.  Pushes that only
modify `docs/` itself are ignored so the workflow doesn’t loop.
You never need to run the script locally; the action will pick up
changes to any crate’s markdown and keep the published site in sync.
Local invocation of `scripts/build_docs.sh` is only useful for
previewing the output before pushing.

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

By default the project had historically deployed the generated website
to a dedicated `gh-pages` branch.  That workflow (`.github/workflows/docs.yml`)
has been removed, since the current job handles everything and writes
into `main/docs` directly.  The Pages source should be configured to
“Branch: `main`” and “Folder: `/docs`”.  (GitHub also shows an
auto‑generated `pages-build-deployment` workflow, which is managed by
GitHub itself and can be ignored.)

When Pages is pointed at `main/docs`, you no longer need to run the
build script locally unless you want to preview the site.  The GitHub
Actions job will pick up any changes in the crate markdown and
update the published site automatically.

> ⚠️ The workflow ignores pushes that only modify files under `docs/`,
> which prevents an infinite update loop when the action commits its
> own changes.

For development or local preview you can run `mkdocs serve` from the
workspace root once the prerequisites are installed.

---

Refer to the individual crate directories for design notes and examples.
