#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CRATE_DIR="$ROOT_DIR/crates/ferrous_web"
OUT_DIR="$CRATE_DIR/pkg"
PROFILE="${1:-release}"

if ! command -v wasm-pack >/dev/null 2>&1; then
  echo "[build_ferrous_web] wasm-pack is required."
  echo "Install: cargo install wasm-pack"
  exit 1
fi

if [[ "$PROFILE" != "release" && "$PROFILE" != "dev" ]]; then
  echo "Usage: $0 [release|dev]"
  exit 1
fi

echo "[build_ferrous_web] Building ferrous_web ($PROFILE)..."

if [[ "$PROFILE" == "release" ]]; then
  wasm-pack build "$CRATE_DIR" \
    --target web \
    --out-dir "$OUT_DIR" \
    --out-name ferrous_web \
    --release
else
  wasm-pack build "$CRATE_DIR" \
    --target web \
    --out-dir "$OUT_DIR" \
    --out-name ferrous_web \
    --dev
fi

echo "[build_ferrous_web] Running export sync check..."
python3 "$ROOT_DIR/scripts/check_ferrous_web_exports.py"

echo "[build_ferrous_web] Done."