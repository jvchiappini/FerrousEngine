#!/usr/bin/env python3
"""Generate the GUIMaker Inter advance table.

Usage:
    python scripts/gen_guimaker_metrics.py path/to/Inter-Regular.ttf

The output is the body of `crates/guimaker/src/text/metrics.rs`: font units at
the font's units-per-em for printable ASCII plus the ellipsis, en dash, em dash
and right quote. The script is a developer tool, not a build dependency; it
needs `fontTools` (`pip install fonttools`).
"""

from __future__ import annotations

import sys

try:
    from fontTools.ttLib import TTFont
except ImportError as error:  # pragma: no cover - tool only
    raise SystemExit("gen_guimaker_metrics.py needs fontTools: pip install fonttools") from error


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__.strip())
        return 2
    font = TTFont(sys.argv[1])
    upem = font["head"].unitsPerEm
    hhea = font["hhea"]
    cmap = font.getBestCmap()
    hmtx = font["hmtx"]

    def advance(character: str) -> int:
        glyph = cmap.get(ord(character))
        return hmtx[glyph][0] if glyph else 0

    ascii_advances = [advance(chr(code)) for code in range(0x20, 0x7F)]
    print(f"UPEM = {upem:.0f}")
    print(f"ASCENT = {hhea.ascent:.0f}")
    print(f"DESCENT = {hhea.descent:.0f}")
    print(f"ELLIPSIS = {advance(chr(0x2026))}")
    print(f"EN_DASH = {advance(chr(0x2013))}")
    print(f"EM_DASH = {advance(chr(0x2014))}")
    print(f"RIGHT_QUOTE = {advance(chr(0x2019))}")
    print("ASCII_ADVANCES = [")
    for start in range(0, len(ascii_advances), 16):
        chunk = ", ".join(str(value) for value in ascii_advances[start : start + 16])
        print(f"    {chunk},")
    print("]")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
