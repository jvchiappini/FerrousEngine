#!/usr/bin/env python3
"""Fail the gate when GUIMaker code hard-codes design values.

Design values live in exactly one module, ``crates/guimaker/src/theme``.
Everywhere else a widget asks for a token (``Spacing::S4``, ``Radius::Md``,
``ColorRole::Accent``); the only pixel escape hatch is ``Px``/``Size::px``.

The guard scans the source of the GUIMaker crates and reports:

- a literal ``Color`` constructor (``Color::hex(``, ``Color::srgb(``, ...);
- a raw pixel number passed to a spacing or radius API
  (``.pad(12.0)``, ``.radius(6.0)``, ``.gap(8.0)``, ...);
- a raw-pixel spacing or radius constructor (``Spacing::Px(``, ...).

Rust tests (``#[cfg(test)]`` modules or single items), every ``tests/``
directory and ``crates/guimaker/src/theme/**`` are exempt. The script exits
1 on any violation and prints ``path:line: message`` lines.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

CRATE_DIRS = (
    "guimaker",
    "guimaker_ferrous",
    "guimaker_gallery",
    "guimaker_designer",
)

TEST_ATTRIBUTE = "#[cfg(test)]"

COLOR_CONSTRUCTOR_RE = re.compile(
    r"\bColor::(hex_alpha|hex|srgba|srgb|lineara|linear|oklcha|oklch)\s*\("
)

SPACING_RADIUS_CALL_RE = re.compile(
    r"\b(?:pad|pad_x|pad_y|gap|margin|radius|border_width|min_width|max_width)"
    r"\s*\("
)

RAW_PX_CONSTRUCTOR_RE = re.compile(
    r"\b(?:Spacing|Radius)::(?:Px|from_px)\s*\("
)

NUMERIC_LITERAL_RE = re.compile(
    r"^[+-]?(?:\d[\d_]*(?:\.[\d_]*)?|\.\d[\d_]*)"
    r"(?:[eE][+-]?\d+)?(?:[fF](?:32|64)?)?"
)

RAW_STRING_RE = re.compile(r'(?<![\w])(?:br|rb|cr|rc|r)(?P<hashes>#{0,255})"')


# ---------------------------------------------------------------------------
# Lexical helpers: comments, strings and braces
# ---------------------------------------------------------------------------


def _skip_line_comment(text: str, index: int) -> int:
    newline = text.find("\n", index)
    return len(text) if newline == -1 else newline + 1


def _skip_block_comment(text: str, index: int) -> int:
    depth = 1
    cursor = index + 2
    length = len(text)
    while cursor < length and depth:
        if text.startswith("/*", cursor):
            depth += 1
            cursor += 2
        elif text.startswith("*/", cursor):
            depth -= 1
            cursor += 2
        else:
            cursor += 1
    return cursor


def _skip_string(text: str, index: int) -> int:
    cursor = index + 1
    length = len(text)
    while cursor < length:
        char = text[cursor]
        if char == "\\":
            cursor += 2
        elif char == '"':
            return cursor + 1
        else:
            cursor += 1
    return length


def _skip_char(text: str, index: int) -> int:
    """Skip a char literal; report ``index + 1`` for a lifetime."""
    cursor = index + 1
    length = len(text)
    if cursor < length and text[cursor] == "\\":
        cursor += 2
        while cursor < length and text[cursor] != "'":
            cursor += 1
        return min(cursor + 1, length)
    if cursor + 1 < length and text[cursor + 1] == "'":
        return cursor + 2
    return index + 1


def _mask_literals(text: str) -> str:
    """Blank comments and string/char literal contents, keeping newlines."""
    chars = list(text)
    index = 0
    length = len(text)
    while index < length:
        raw_string = RAW_STRING_RE.match(text, index)
        if raw_string is not None:
            closing = '"' + "#" * len(raw_string.group("hashes"))
            found = text.find(closing, raw_string.end())
            end = length if found == -1 else found + len(closing)
        elif text.startswith("//", index):
            end = _skip_line_comment(text, index)
        elif text.startswith("/*", index):
            end = _skip_block_comment(text, index)
        elif text[index] == '"':
            end = _skip_string(text, index)
        elif text[index] == "'":
            end = _skip_char(text, index)
            if end == index + 1:
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


def _skip_trivia(text: str, index: int) -> int:
    length = len(text)
    while index < length and text[index] in " \t\r\n":
        index += 1
    return index


def _skip_attribute(text: str, index: int) -> int:
    """Skip ``#[...]`` starting at ``index``; return the index after ``]``."""
    depth = 0
    cursor = index + 1
    length = len(text)
    while cursor < length:
        char = text[cursor]
        if char == "[":
            depth += 1
        elif char == "]":
            depth -= 1
            if depth == 0:
                return cursor + 1
        cursor += 1
    return length


def _match_brace(text: str, open_index: int) -> int:
    """Return the index just past the brace matching ``text[open_index]``."""
    depth = 0
    cursor = open_index
    length = len(text)
    while cursor < length:
        char = text[cursor]
        if char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return cursor + 1
        cursor += 1
    return length


def _item_end(text: str, index: int) -> int:
    """Return the end of the item that starts at ``index``.

    A brace block is matched to its closing brace; an item without a body
    ends at the next semicolon.
    """
    length = len(text)
    cursor = index
    while cursor < length:
        char = text[cursor]
        if char == "{":
            return _match_brace(text, cursor)
        if char == ";":
            return cursor + 1
        cursor += 1
    return length


def _mask_test_code(text: str) -> str:
    """Blank every ``#[cfg(test)]`` module or single item, keeping newlines."""
    chars = list(text)
    cursor = 0
    length = len(text)
    while True:
        found = text.find(TEST_ATTRIBUTE, cursor)
        if found == -1:
            break
        position = found + len(TEST_ATTRIBUTE)
        position = _skip_trivia(text, position)
        while text.startswith("#[", position):
            position = _skip_trivia(text, _skip_attribute(text, position))
        end = _item_end(text, position)
        for index in range(found, end):
            if chars[index] != "\n":
                chars[index] = " "
        cursor = end
    return "".join(chars)


# ---------------------------------------------------------------------------
# Detection
# ---------------------------------------------------------------------------


def _line_of(text: str, index: int) -> int:
    return text.count("\n", 0, index) + 1


def _check_file(path: Path, root: Path) -> list[str]:
    text = path.read_text(encoding="utf-8")
    masked = _mask_test_code(_mask_literals(text))
    relative = path.relative_to(root).as_posix()
    violations: list[str] = []

    for match in COLOR_CONSTRUCTOR_RE.finditer(masked):
        violations.append(
            f"{relative}:{_line_of(text, match.start())}: "
            f"literal Color::{match.group(1)}( outside guimaker::theme; "
            "use a ColorRole or another theme token"
        )

    for match in SPACING_RADIUS_CALL_RE.finditer(masked):
        first_argument = masked[match.end() :].lstrip()
        if NUMERIC_LITERAL_RE.match(first_argument):
            name = match.group(0).split("(", 1)[0].strip()
            violations.append(
                f"{relative}:{_line_of(text, match.start())}: "
                f"raw pixel value passed to {name}( outside guimaker::theme; "
                "use a Spacing or Radius token"
            )

    for match in RAW_PX_CONSTRUCTOR_RE.finditer(masked):
        name = match.group(0).split("(", 1)[0].strip()
        violations.append(
            f"{relative}:{_line_of(text, match.start())}: "
            f"raw-pixel constructor {name}( outside guimaker::theme; "
            "use a token, and reserve Px::new for the explicit escape hatch"
        )

    return violations


# ---------------------------------------------------------------------------
# Tree scanning
# ---------------------------------------------------------------------------


def _is_exempt(path: Path, root: Path) -> bool:
    relative = path.relative_to(root)
    parts = relative.parts
    if "tests" in parts:
        return True
    return (
        len(parts) >= 4
        and parts[0] == "crates"
        and parts[1] == "guimaker"
        and parts[2] == "src"
        and parts[3] == "theme"
    )


def _source_files(root: Path) -> list[Path]:
    files: list[Path] = []
    for crate in CRATE_DIRS:
        source = root / "crates" / crate / "src"
        if not source.is_dir():
            continue
        for path in sorted(source.rglob("*.rs")):
            if not _is_exempt(path, root):
                files.append(path)
    return files


def scan(root: Path) -> tuple[int, list[str]]:
    """Return ``(files_scanned, violations)`` for a repository root."""
    root = Path(root).resolve()
    files = _source_files(root)
    violations: list[str] = []
    for path in files:
        violations.extend(_check_file(path, root))
    return len(files), violations


def collect_violations(root: Path) -> list[str]:
    """Return one ``path:line: message`` string per violation."""
    return scan(root)[1]


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Reject literal design values outside guimaker::theme."
    )
    parser.add_argument(
        "--root",
        type=Path,
        default=None,
        help="repository root (defaults to the script's parent checkout)",
    )
    arguments = parser.parse_args(argv)
    root = (
        arguments.root.resolve()
        if arguments.root is not None
        else Path(__file__).resolve().parents[1]
    )

    scanned, violations = scan(root)
    for violation in violations:
        print(violation)

    if violations:
        print(f"guimaker literal guard: {len(violations)} violation(s)")
        return 1

    print(f"guimaker literal guard: {scanned} files scanned, 0 violations")
    return 0


if __name__ == "__main__":
    sys.exit(main())
