# ferrous_web

WebAssembly bindings for `FerrousWebEngine`.

## PR1: API export sync guard

This crate keeps generated bindings in `pkg/` checked against the Rust wasm API.

### Build and regenerate bindings

From workspace root:

```bash
./scripts/build_ferrous_web.sh
```

Or dev profile:

```bash
./scripts/build_ferrous_web.sh dev
```

### Run sync check only

```bash
python3 ./scripts/check_ferrous_web_exports.py
```

The check validates method parity for `FerrousWebEngine` between:

- `crates/ferrous_web/src/lib.rs`
- `crates/ferrous_web/pkg/ferrous_web.js`
- `crates/ferrous_web/pkg/ferrous_web.d.ts`

If it fails, regenerate bindings with the build script above.