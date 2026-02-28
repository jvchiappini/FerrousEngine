#!/usr/bin/env bash
# Helper script builds documentation site using mkdocs and per-crate markdown.

set -euo pipefail

# clean old directories
rm -rf docs site
mkdir -p docs

# copy workspace README to the homepage if it exists
if [ -f README.md ]; then
    cp README.md docs/index.md
fi

# aggregate documentation from each crate
for crate in crates/*; do
    name=$(basename "$crate")
    dest="docs/$name"

    if [ -d "$crate/docs" ]; then
        mkdir -p "$dest"
        cp -R "$crate/docs/"* "$dest/"
    else
        # no dedicated documentation directory; fall back to the crate's
        # top‑level README if one exists.  this ensures each crate shows up
        # in the generated site with at least a placeholder page.
        if [ -f "$crate/README.md" ]; then
            mkdir -p "$dest"
            cp "$crate/README.md" "$dest/"
        fi
    fi
done

# if mkdocs is available, build HTML output using the material theme
if command -v mkdocs >/dev/null; then
    mkdocs build -d site
    echo "Generated static site in site/"
else
    echo "mkdocs not found; source markdown files available under docs/"
fi
