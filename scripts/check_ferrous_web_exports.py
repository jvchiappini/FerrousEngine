#!/usr/bin/env python3

from __future__ import annotations

import re
import sys
from pathlib import Path


def read_text(path: Path) -> str:
    if not path.exists():
        raise FileNotFoundError(f"Missing required file: {path}")
    return path.read_text(encoding="utf-8")


def extract_class_body(source: str, class_decl: str) -> str:
    start = source.find(class_decl)
    if start == -1:
        raise ValueError(f"Could not locate '{class_decl}'")

    brace_start = source.find("{", start)
    if brace_start == -1:
        raise ValueError(f"Could not locate opening brace for '{class_decl}'")

    depth = 0
    for idx in range(brace_start, len(source)):
        ch = source[idx]
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0:
                return source[brace_start + 1 : idx]

    raise ValueError(f"Could not locate closing brace for '{class_decl}'")


def parse_rust_api(rust_sources: str) -> set[str]:
    # Match #[wasm_bindgen(js_name = setPosition)] or #[wasm_bindgen(js_name = "setPosition")]
    # Group 1: js_name without quotes
    # Group 2: js_name with quotes
    # Group 3: rust method name
    pattern = re.compile(
        r"#\[wasm_bindgen(?:\(\s*js_name\s*=\s*(?:([a-zA-Z0-9_]+)|\"([a-zA-Z0-9_]+)\")\s*\))?\]\s*pub\s+fn\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*\(",
        re.MULTILINE,
    )
    
    methods = set()
    for js_name_no_quotes, js_name_quotes, rust_name in pattern.findall(rust_sources):
        if js_name_no_quotes:
            methods.add(js_name_no_quotes)
        elif js_name_quotes:
            methods.add(js_name_quotes)
        else:
            methods.add(rust_name)
            
    methods.discard("new")
    return methods


def parse_dts_api(dts: str, class_name: str) -> set[str]:
    try:
        body = extract_class_body(dts, f"export class {class_name}")
    except ValueError:
        return set()

    methods = set()
    for raw_line in body.splitlines():
        if not raw_line.startswith("    ") or raw_line.startswith("        "):
            continue
        line = raw_line.strip()
        if not line or line.startswith("[") or line.startswith("free"):
            continue
        if line.startswith("constructor"):
            continue
        method_match = re.match(r"([a-zA-Z_][a-zA-Z0-9_]*)\s*\(", line)
        if method_match:
            methods.add(method_match.group(1))
    return methods


def parse_js_api(js: str, class_name: str) -> set[str]:
    try:
        body = extract_class_body(js, f"export class {class_name}")
    except ValueError:
        return set()

    methods = set()
    for raw_line in body.splitlines():
        if not raw_line.startswith("    ") or raw_line.startswith("        "):
            continue
        line = raw_line.strip()
        if not line or line.startswith("__") or line.startswith("free"):
            continue
        method_match = re.match(r"([a-zA-Z_][a-zA-Z0-9_]*)\s*\(", line)
        if method_match:
            name = method_match.group(1)
            if name != "constructor":
                methods.add(name)
    return methods


def parse_dts_wasm_exports(dts: str, prefix: str) -> set[str]:
    pattern = re.compile(rf"readonly\s+{prefix}_([a-zA-Z0-9_]+):")
    exports = set(pattern.findall(dts))
    exports.discard("new")
    exports.discard("free")
    return exports


def report_diff(lhs_name: str, lhs: set[str], rhs_name: str, rhs: set[str]) -> list[str]:
    messages: list[str] = []
    missing = sorted(lhs - rhs)
    extra = sorted(rhs - lhs)
    if missing:
        messages.append(f"Missing in {rhs_name}: {', '.join(missing)}")
    if extra:
        messages.append(f"Extra in {rhs_name}: {', '.join(extra)}")
    if messages:
        messages.insert(0, f"{lhs_name} vs {rhs_name} mismatch")
    return messages


def main() -> int:
    root = Path(__file__).resolve().parents[1]
    src_dir = root / "crates" / "ferrous_web" / "src"
    dts_path = root / "crates" / "ferrous_web" / "pkg" / "ferrous_web.d.ts"
    js_path = root / "crates" / "ferrous_web" / "pkg" / "ferrous_web.js"

    rust_sources = "\n".join(
        read_text(path) for path in sorted(src_dir.rglob("*.rs"))
    )
    dts = read_text(dts_path)
    js = read_text(js_path)

    rust_api = parse_rust_api(rust_sources)
    
    # Check FerrousWebEngine
    dts_engine = parse_dts_api(dts, "FerrousWebEngine")
    js_engine = parse_js_api(js, "FerrousWebEngine")
    wasm_engine = parse_dts_wasm_exports(dts, "ferrouswebengine")
    
    # Check JsEntity
    dts_entity = parse_dts_api(dts, "JsEntity")
    js_entity = parse_js_api(js, "JsEntity")
    wasm_entity = parse_dts_wasm_exports(dts, "jsentity")
    
    combined_dts = dts_engine | dts_entity
    combined_js = js_engine | js_entity
    combined_wasm = wasm_engine | wasm_entity

    errors: list[str] = []
    errors.extend(report_diff("Rust API", rust_api, "TypeScript class API", combined_dts))
    errors.extend(report_diff("Rust API", rust_api, "Generated JS class API", combined_js))
    errors.extend(report_diff("Rust API", rust_api, "WASM export table", combined_wasm))

    if errors:
        print("[PR1 Check] Ferrous Web API export sync FAILED")
        print()
        for err in errors:
            print(f"- {err}")
        print()
        print("Fix by regenerating bindings, e.g.:")
        print("  ./scripts/build_ferrous_web.sh")
        return 1

    print("[PR1 Check] Ferrous Web API export sync OK")
    print(f"- Rust methods checked: {len(rust_api)}")
    print(f"- Matched in d.ts/js/wasm exports")
    return 0


if __name__ == "__main__":
    sys.exit(main())