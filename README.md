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
to a dedicated `gh-pages` branch.  It’s no longer necessary to keep a
second branch – the CI workflow now regenerates `docs/` on every push
and commits those files back to `main`, and the Pages source is set to
**main / docs**.  You can verify or change the setting under
Settings → Pages; the “Branch” dropdown should read `main` and the
folder `/docs`.

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
