#!/usr/bin/env python3
"""Measure Phase 9 Lane F: GUIMaker view vs plain HTML + CSS.

Counts, for the start screen only:

* GUIMaker **view**: every line of ``start_screen`` in
  ``crates/guimaker/src/recipe.rs``, split into non-blank / non-comment
  lines, plus every numeric literal token outside strings. The general
  component builders the view calls (``field``, ``list_row``,
  ``nav_link``, ``content_column``, ``clock_hh_mm``) are library API,
  the analogue of HTML's built-in elements and CSS engine: their own
  line counts are reported separately and are not part of the view.
* HTML + CSS: the same counts for ``out/compare/start.html``, with the
  ``<style>`` block reported separately from the markup.

Exit code is 0 even when a target is missed: this is a report generator,
not a gate. The verdicts are printed on the last lines.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RECIPE = ROOT / "crates" / "guimaker" / "src" / "recipe.rs"
HTML = ROOT / "out" / "compare" / "start.html"

# The view itself: the recipe function an app author writes.
VIEW_FUNCTIONS = ["start_screen"]

# The general library builders the view calls. They are the analogue of the
# browser's `<input>`, `<li>` and `<button>` implementations, so they are
# reported for transparency but not counted as view code.
LIBRARY_FUNCTIONS = [
    ("crates/guimaker/src/components/display.rs", "field"),
    ("crates/guimaker/src/components/display.rs", "list_row"),
    ("crates/guimaker/src/components/actions.rs", "nav_link"),
    ("crates/guimaker/src/components/containers.rs", "content_column"),
    ("crates/guimaker/src/format.rs", "clock_hh_mm"),
]

# A Rust numeric token: integer or float, optional suffix; a word boundary
# before it keeps `f64`, `S2`, and `x2` out, and allows `0..5u64` to yield
# two tokens because `.` is not a word character.
NUMBER_RE = re.compile(
    r"\b\d[\d_]*(?:\.[\d_]+)?(?:[eE][+-]?\d+)?"
    r"(?:[iuf](?:8|16|32|64|128|size)?)?\b"
)
# CSS values carry units, so a number may be followed by `px`, `%`, ...;
# a digit that is part of an identifier (`h1`, `--s1`) is not a literal.
CSS_NUMBER_RE = re.compile(r"(?<![\w.#-])\d[\d_]*(?:\.[\d_]+)?(?:[a-zA-Z%]+)?")
HEX_COLOR_RE = re.compile(r"#[0-9A-Fa-f]{3,8}\b")


def mask_rust(text: str) -> str:
    """Blank comments and string/char literal contents, keep newlines."""
    chars = list(text)
    index = 0
    length = len(text)
    while index < length:
        if text.startswith("//", index):
            end = text.find("\n", index)
            end = length if end == -1 else end
        elif text.startswith("/*", index):
            end = text.find("*/", index + 2)
            end = length if end == -1 else end + 2
        elif text[index] == '"':
            cursor = index + 1
            while cursor < length:
                if text[cursor] == "\\":
                    cursor += 2
                elif text[cursor] == '"':
                    cursor += 1
                    break
                else:
                    cursor += 1
            end = min(cursor, length)
        elif text[index] == "'":
            # A char literal has a closing quote two or three chars away;
            # anything else is a lifetime and is left alone.
            if index + 2 < length and text[index + 2] == "'":
                end = index + 3
            elif index + 3 < length and text[index + 1] == "\\":
                end = text.find("'", index + 2) + 1 or length
            else:
                index += 1
                continue
        else:
            index += 1
            continue
        for position in range(index, end):
            if chars[position] != "\n":
                chars[position] = " "
        index = end
    return "".join(chars)


def rust_function_range(lines: list[str], name: str, path: Path) -> tuple[int, int]:
    """Return the 1-based inclusive line span of `fn name`, braces matched."""
    pattern = re.compile(rf"^\s*(?:pub(?:\([^)]*\))?\s+)?fn\s+{re.escape(name)}\b")
    start = None
    for number, line in enumerate(lines, 1):
        if pattern.search(line):
            start = number
            break
    if start is None:
        raise SystemExit(f"function {name} not found in {path}")
    masked = mask_rust("\n".join(lines))
    index = masked.find("{", sum(len(line) + 1 for line in lines[: start - 1]))
    if index == -1:
        raise SystemExit(f"function {name} has no body")
    depth = 0
    for cursor in range(index, len(masked)):
        if masked[cursor] == "{":
            depth += 1
        elif masked[cursor] == "}":
            depth -= 1
            if depth == 0:
                end_line = masked.count("\n", 0, cursor) + 1
                return start, end_line
    raise SystemExit(f"function {name} has an unbalanced body")


def count_rust(lines: list[str], span: tuple[int, int]) -> dict[str, object]:
    start, end = span
    body = lines[start - 1 : end]
    masked = mask_rust("\n".join(body)).splitlines()
    code_lines = 0
    literals: list[tuple[int, str]] = []
    for offset, (raw, clean) in enumerate(zip(body, masked)):
        stripped = raw.strip()
        if not stripped or stripped.startswith("//"):
            continue
        code_lines += 1
        for match in NUMBER_RE.finditer(clean):
            literals.append((start + offset, match.group(0)))
    return {"lines": code_lines, "literals": literals}


def split_html(text: str) -> tuple[list[str], list[str]]:
    """Split start.html into (markup lines, style-block lines)."""
    markup: list[str] = []
    css: list[str] = []
    in_style = False
    for line in text.splitlines():
        if "<style>" in line:
            in_style = True
            markup.append(line)
            continue
        if "</style>" in line:
            in_style = False
            markup.append(line)
            continue
        (css if in_style else markup).append(line)
    return markup, css


TEXT_NODE_RE = re.compile(r">[^<]*<")


def count_html_block(lines: list[str], strip_text: bool, css: bool) -> dict[str, object]:
    code_lines = 0
    numbers: list[str] = []
    colours: list[str] = []
    in_comment = False
    pattern = CSS_NUMBER_RE if css else NUMBER_RE
    for raw in lines:
        line = raw.strip()
        if in_comment:
            if "-->" in line:
                in_comment = False
            continue
        if line.startswith("<!--"):
            in_comment = "-->" not in line
            continue
        if line.startswith("/*"):
            in_comment = "*/" not in line
            continue
        if in_comment:
            if "*/" in line:
                in_comment = False
            continue
        if not line:
            continue
        code_lines += 1
        value_line = TEXT_NODE_RE.sub("><", line) if strip_text else line
        colours.extend(HEX_COLOR_RE.findall(value_line))
        numbers.extend(pattern.findall(HEX_COLOR_RE.sub(" ", value_line)))
    return {"lines": code_lines, "numbers": numbers, "colours": colours}


def split_css_token_block(css: list[str]) -> tuple[list[str], list[str]]:
    """Split the style block into the `:root{...}` token lines and the rest."""
    token_block: list[str] = []
    rules: list[str] = []
    inside = False
    for line in css:
        stripped = line.strip()
        if stripped == ":root{":
            inside = True
        (token_block if inside else rules).append(line)
        if inside and stripped == "}":
            inside = False
    return token_block, rules


def main() -> int:
    if not RECIPE.is_file() or not HTML.is_file():
        raise SystemExit(f"missing input: {RECIPE} / {HTML}")

    recipe_lines = RECIPE.read_text(encoding="utf-8").splitlines()
    spans = {
        name: rust_function_range(recipe_lines, name, RECIPE) for name in VIEW_FUNCTIONS
    }
    per_function = {name: count_rust(recipe_lines, span) for name, span in spans.items()}

    view_lines = sum(item["lines"] for item in per_function.values())
    view_literals = [
        (line, token)
        for item in per_function.values()
        for line, token in item["literals"]
    ]

    library: list[tuple[str, str, dict[str, object]]] = []
    for relative, name in LIBRARY_FUNCTIONS:
        path = ROOT / relative
        lines = path.read_text(encoding="utf-8").splitlines()
        span = rust_function_range(lines, name, path)
        library.append((relative, name, count_rust(lines, span)))

    markup, css = split_html(HTML.read_text(encoding="utf-8"))
    markup_counts = count_html_block(markup, strip_text=True, css=False)
    css_counts = count_html_block(css, strip_text=False, css=True)
    token_block, rules = split_css_token_block(css)
    token_counts = count_html_block(token_block, strip_text=False, css=True)
    rule_counts = count_html_block(rules, strip_text=False, css=True)
    html_combined = {
        "lines": markup_counts["lines"] + css_counts["lines"],
        "numbers": markup_counts["numbers"] + css_counts["numbers"],
        "colours": markup_counts["colours"] + css_counts["colours"],
    }

    raw_total = len(HTML.read_text(encoding="utf-8").splitlines())
    raw_nonblank = sum(1 for line in HTML.read_text(encoding="utf-8").splitlines() if line.strip())

    print("GUIMaker view (crates/guimaker/src/recipe.rs)")
    for name in VIEW_FUNCTIONS:
        span = spans[name]
        item = per_function[name]
        print(
            f"  {name:<15} lines {span[0]:>3}-{span[1]:<3} "
            f"code {item['lines']:>3}  numeric literals {len(item['literals']):>2}"
        )
    print(f"  view code lines ...................... {view_lines}")
    print(f"  numeric literals ...................... {len(view_literals)}")
    for line, token in view_literals:
        source = recipe_lines[line - 1].strip()
        print(f"    recipe.rs:{line}: {token!r}  |  {source}")

    print()
    print("Library components used by the view (not counted, like HTML tags)")
    for relative, name, item in library:
        print(
            f"  {name:<15} {relative:<46} "
            f"code {item['lines']:>3}  numeric literals {len(item['literals']):>2}"
        )

    print()
    print("HTML + CSS (out/compare/start.html)")
    print(f"  whole file, raw lines ................. {raw_total}")
    print(f"  whole file, non-blank lines ........... {raw_nonblank}")
    print(f"  markup, code lines .................... {markup_counts['lines']}")
    print(f"  CSS, code lines ....................... {css_counts['lines']}")
    print(f"  markup + CSS, code lines .............. {html_combined['lines']}")
    print(f"  markup numeric tokens ................. {len(markup_counts['numbers'])}")
    print(f"  CSS numeric tokens .................... {len(css_counts['numbers'])}")
    print(f"    in the :root token block ............. {len(token_counts['numbers'])}")
    print(f"    in the rules ......................... {len(rule_counts['numbers'])}")
    print(f"  CSS colour hex literals ............... {len(css_counts['colours'])}")
    print(f"  markup + CSS numeric tokens ........... {len(html_combined['numbers'])}")
    print(f"  markup + CSS value literals (numbers + colours) "
          f"{len(html_combined['numbers']) + len(html_combined['colours'])}")

    print()
    print("VERDICT")
    at_most = view_lines <= html_combined["lines"]
    print(
        f"  at most as many lines as HTML + CSS: "
        f"GUIMaker {view_lines} vs HTML+CSS {html_combined['lines']} (limit 139) -> "
        f"{'PASS' if at_most else 'FAIL'}"
    )
    print(
        f"  zero numeric literals in the GUIMaker view: {len(view_literals)} -> "
        f"{'PASS' if not view_literals else 'FAIL'}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
