#!/usr/bin/env python3
"""Reject a UTF-8 BOM, cp1252 double-encoding and CRLF in new files.

Motioner's guard, adapted to this repository. The power tools that write files
on Windows (PowerShell ``Set-Content``, ``Out-File``, a text-mode editor) add a
BOM, turn LF into CRLF, or decode a UTF-8 file as cp1252 and save it again as
UTF-8. The last one doubles every non-ASCII character: an em dash, a
multiplication sign or an ellipsis becomes two or three visible characters.

- **BOM and mojibake** are checked in every tracked text file, read from the
  index (what the next commit contains) or from the working tree when the file
  differs, plus every new file.
- **CR** is only checked in **new** (untracked) files. The committed baseline
  is LF in the index but the working tree is a Windows checkout with CRLF, so a
  checked-out file legitimately contains CR; a brand-new file has no such
  excuse.

Binary files (``git`` reports ``-text``) are skipped.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

BOM = b"\xef\xbb\xbf"
# The sequences are written as escapes so this file does not trip its own
# guard: A-tilde and A-circumflex followed by a non-letter, and a-circumflex +
# euro anywhere (the second byte of a UTF-8 sequence decoded as cp1252).
MOJIBAKE_PREFIXES = ("\u00c3", "\u00c2")
MOJIBAKE_EURO = "\u00e2\u20ac"


def looks_binary(data: bytes) -> bool:
    """A NUL byte in the first 8 KiB means git treats the file as binary."""
    return b"\x00" in data[:8192]


def mojibake_lines(text: str) -> set[int]:
    """Zero-based line numbers with a double-encoded UTF-8 marker."""
    flagged: set[int] = set()
    number = 0
    for index, char in enumerate(text):
        if char == "\n":
            number += 1
        elif char in MOJIBAKE_PREFIXES:
            following = text[index + 1] if index + 1 < len(text) else ""
            if not following.isalpha():
                flagged.add(number)
        if text.startswith(MOJIBAKE_EURO, index):
            flagged.add(number)
    return flagged


def problems_for(data: bytes, path: str = "", check_cr: bool = False) -> list[str]:
    """The encoding findings of one file's bytes."""
    problems: list[str] = []
    if data.startswith(BOM):
        problems.append("UTF-8 BOM")
    if check_cr and b"\r" in data:
        problems.append("CR (CRLF line endings)")
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError:
        problems.append("not valid UTF-8")
        return problems
    flagged = sorted(mojibake_lines(text))
    if flagged:
        shown = ", ".join(str(number + 1) for number in flagged[:5])
        more = "" if len(flagged) <= 5 else f" (+{len(flagged) - 5} more)"
        problems.append(f"mojibake (double-encoded UTF-8) at line {shown}{more}")
    return problems


def run_git(root: Path, *args: str) -> str:
    """Runs git in ``root`` and returns its UTF-8 trimmed output."""
    return subprocess.check_output(
        ["git", *args], cwd=root, text=True, encoding="utf-8"
    ).strip()


def index_text_paths(root: Path) -> list[str]:
    """Tracked text paths from ``git ls-files --eol``."""
    paths: list[str] = []
    for line in run_git(root, "ls-files", "--eol").splitlines():
        if "\t" not in line:
            continue
        head, path = line.split("\t", 1)
        if "i/-text" in head.split():
            continue
        paths.append(path)
    return paths


def index_problems(root: Path, paths: list[str]) -> dict[str, list[str]]:
    """Reads the index blobs and reports BOM/mojibake findings per path."""
    findings: dict[str, list[str]] = {}
    if not paths:
        return findings
    request = "".join(f":{path}\n" for path in paths).encode("utf-8")
    output = subprocess.run(
        ["git", "cat-file", "--batch"],
        cwd=root,
        input=request,
        stdout=subprocess.PIPE,
        check=True,
    ).stdout
    position = 0
    for path in paths:
        end = output.index(b"\n", position)
        header = output[position:end].decode("utf-8", "replace")
        position = end + 1
        parts = header.split()
        if len(parts) != 3 or parts[1] != "blob":
            findings[path] = ["could not read the index blob"]
            continue
        size = int(parts[2])
        data = output[position : position + size]
        position += size + 1  # the newline after every blob
        problems = problems_for(data, path)
        if problems:
            findings[path] = problems
    return findings


def list_paths(root: Path, *args: str) -> list[str]:
    """The NUL-separated paths of one ``git ls-files``/``git diff`` call."""
    return [name for name in run_git(root, *args).split("\0") if name]


def worktree_problems(
    root: Path, paths: list[str], check_cr: bool
) -> dict[str, list[str]]:
    """Checks changed or new files in the working tree."""
    findings: dict[str, list[str]] = {}
    for path in paths:
        try:
            data = (root / path).read_bytes()
        except OSError:
            continue
        if looks_binary(data):
            continue
        problems = problems_for(data, path, check_cr=check_cr)
        if problems:
            findings[path] = problems
    return findings


def collect_problems(root: Path) -> dict[str, list[str]]:
    """Every encoding finding in the repository."""
    changed = set(
        list_paths(root, "diff", "--name-only", "--diff-filter=ACMRT", "-z")
    )
    untracked = list_paths(root, "ls-files", "--others", "--exclude-standard", "-z")
    findings = index_problems(
        root, [path for path in index_text_paths(root) if path not in changed]
    )
    for path, problems in worktree_problems(
        root, sorted(changed - set(untracked)), check_cr=False
    ).items():
        findings.setdefault(path, []).extend(
            problem for problem in problems if problem not in findings.get(path, [])
        )
    for path, problems in worktree_problems(
        root, sorted(untracked), check_cr=True
    ).items():
        findings.setdefault(path, []).extend(
            problem for problem in problems if problem not in findings.get(path, [])
        )
    return findings


def main() -> int:
    root = Path(__file__).resolve().parents[1]
    findings = collect_problems(root)
    if findings:
        print("Encoding violations (write files as UTF-8 without a BOM, LF only):")
        for path in sorted(findings):
            print(f"  - {path}: {', '.join(findings[path])}")
        return 1
    print("Encoding OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
