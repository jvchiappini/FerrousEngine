"""Tests for ``scripts/check_encoding.py``."""

from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import check_encoding as encoding  # noqa: E402


class MojibakeTest(unittest.TestCase):
    def test_clean_utf8_has_no_problems(self) -> None:
        data = "café — ±3 … ok\n".encode("utf-8")
        self.assertEqual(encoding.problems_for(data), [])

    def test_bom_is_reported(self) -> None:
        problems = encoding.problems_for(b"\xef\xbb\xbfok\n")
        self.assertEqual(problems, ["UTF-8 BOM"])

    def test_crlf_is_only_reported_when_asked(self) -> None:
        data = b"one\r\ntwo\r\n"
        self.assertEqual(encoding.problems_for(data), [])
        self.assertEqual(
            encoding.problems_for(data, check_cr=True),
            ["CR (CRLF line endings)"],
        )

    def test_invalid_utf8_is_reported(self) -> None:
        self.assertEqual(
            encoding.problems_for(b"\xff\xfe\x00"), ["not valid UTF-8"]
        )

    def test_cp1252_round_trip_is_reported(self) -> None:
        # An em dash decoded as cp1252 and saved as UTF-8.
        broken = "\u00e2\u20ac\u201d".encode("utf-8")
        problems = encoding.problems_for(broken)
        self.assertEqual(len(problems), 1)
        self.assertIn("mojibake", problems[0])

    def test_a_tilde_followed_by_a_letter_is_valid_text(self) -> None:
        # An A-tilde alone is a legitimate Portuguese character when followed by a
        # letter; the guard only rejects it before punctuation/space.
        self.assertEqual(encoding.problems_for("Ãbaco".encode("utf-8")), [])

    def test_a_tilde_before_punctuation_is_mojibake(self) -> None:
        problems = encoding.problems_for("\u00c3\u00a9".encode("utf-8"))
        self.assertEqual(len(problems), 1)
        self.assertIn("mojibake", problems[0])

    def test_binary_is_skipped_by_the_worktree_scan(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "blob.bin").write_bytes(b"\x00\x01\x02")
            self.assertEqual(
                encoding.worktree_problems(root, ["blob.bin"], check_cr=True), {}
            )

    def test_worktree_scan_reports_bom_and_cr(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "new.md").write_bytes(b"\xef\xbb\xbfone\r\n")
            findings = encoding.worktree_problems(root, ["new.md"], check_cr=True)
            self.assertEqual(
                findings,
                {"new.md": ["UTF-8 BOM", "CR (CRLF line endings)"]},
            )


class RepositoryTest(unittest.TestCase):
    def test_this_repository_is_clean(self) -> None:
        root = Path(__file__).resolve().parents[2]
        findings = encoding.collect_problems(root)
        self.assertEqual(findings, {}, f"{len(findings)} finding(s)")


if __name__ == "__main__":
    unittest.main()
