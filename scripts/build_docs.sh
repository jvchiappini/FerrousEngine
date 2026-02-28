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
    if [ -d "$crate/docs" ]; then
        dest="docs/$(basename \"$crate\")"
        mkdir -p "$dest"
        cp -R "$crate/docs/"* "$dest/"
    fi
done

# if mkdocs is available, build HTML output using the material theme
if command -v mkdocs >/dev/null; then
    mkdocs build -d site
    echo "Generated static site in site/"
else
    echo "mkdocs not found; source markdown files available under docs/"
fi
