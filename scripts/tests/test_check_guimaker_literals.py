"""Tests for ``scripts/check_guimaker_literals.py``.

Run with ``python -m unittest discover -s scripts/tests -p
"test_check_guimaker_literals.py"``. Every fixture is a real temporary
checkout, so the guard exercises its path resolution and file scanning.
"""

from __future__ import annotations

import contextlib
import io
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import check_guimaker_literals


def _write(checkout: Path, relative: str, body: str) -> Path:
    path = checkout / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(body, encoding="utf-8")
    return path


def _run(files: dict[str, str]) -> list[str]:
    with tempfile.TemporaryDirectory() as temporary:
        checkout = Path(temporary)
        for relative, body in files.items():
            _write(checkout, relative, body)
        return check_guimaker_literals.collect_violations(checkout)


class LiteralGuardTest(unittest.TestCase):
    def test_clean_file_passes(self) -> None:
        violations = _run(
            {
                "crates/guimaker/src/view.rs": (
                    "pub fn build(theme: &Theme) -> View {\n"
                    "    let gap = theme.spacing(Spacing::S4);\n"
                    "    let accent = theme.role(ColorRole::Accent);\n"
                    "    View::new().gap(gap).pad(Spacing::S4)\n"
                    "}\n"
                )
            }
        )
        self.assertEqual(violations, [])

    def test_literal_color_constructor_fails(self) -> None:
        violations = _run(
            {
                "crates/guimaker/src/view.rs": (
                    "pub fn build() -> Color {\n"
                    "    Color::hex(0x112233)\n"
                    "}\n"
                )
            }
        )
        self.assertEqual(len(violations), 1)
        self.assertIn("crates/guimaker/src/view.rs:2:", violations[0])
        self.assertIn("literal Color::hex(", violations[0])

    def test_raw_pad_number_fails(self) -> None:
        violations = _run(
            {"crates/guimaker/src/view.rs": "fn build() { view.pad(12.0); }\n"}
        )
        self.assertEqual(len(violations), 1)
        self.assertIn("raw pixel value passed to pad(", violations[0])

    def test_raw_radius_number_fails(self) -> None:
        violations = _run(
            {"crates/guimaker/src/view.rs": "fn build() { view.radius(6.0); }\n"}
        )
        self.assertEqual(len(violations), 1)
        self.assertIn("raw pixel value passed to radius(", violations[0])

    def test_cfg_test_module_is_exempt(self) -> None:
        violations = _run(
            {
                "crates/guimaker/src/view.rs": (
                    "pub fn build() -> View { View::new() }\n"
                    "\n"
                    "#[cfg(test)]\n"
                    "mod tests {\n"
                    "    use super::*;\n"
                    "\n"
                    "    #[test]\n"
                    "    fn literals_are_fine_in_tests() {\n"
                    "        let color = Color::hex(0x112233);\n"
                    "        let style = Style::new().pad(12.0).radius(6.0);\n"
                    "    }\n"
                    "}\n"
                )
            }
        )
        self.assertEqual(violations, [])

    def test_cfg_test_single_item_is_exempt(self) -> None:
        violations = _run(
            {
                "crates/guimaker/src/view.rs": (
                    "#[cfg(test)]\n"
                    "fn helper() -> Color { Color::hex(0x112233) }\n"
                    "\n"
                    "pub fn build() -> View { View::new() }\n"
                )
            }
        )
        self.assertEqual(violations, [])

    def test_theme_file_is_exempt(self) -> None:
        violations = _run(
            {
                "crates/guimaker/src/theme/palette.rs": (
                    "pub fn accent() -> Color {\n"
                    "    Color::hex(0x3366FF)\n"
                    "}\n"
                )
            }
        )
        self.assertEqual(violations, [])

    def test_tests_directory_is_exempt(self) -> None:
        violations = _run(
            {
                "crates/guimaker/src/tests/helpers.rs": (
                    "fn style() -> Style { Style::new().pad(12.0) }\n"
                )
            }
        )
        self.assertEqual(violations, [])

    def test_token_pad_passes(self) -> None:
        violations = _run(
            {
                "crates/guimaker/src/view.rs": (
                    "fn build() { view.pad(Spacing::S4).gap(gap); }\n"
                )
            }
        )
        self.assertEqual(violations, [])

    def test_px_width_escape_hatch_passes(self) -> None:
        violations = _run(
            {
                "crates/guimaker/src/view.rs": (
                    "fn build() { view.width(Size::Px(13.0)).height(Px::new(7.0)); }\n"
                )
            }
        )
        self.assertEqual(violations, [])

    def test_raw_spacing_px_constructor_fails(self) -> None:
        violations = _run(
            {"crates/guimaker/src/view.rs": "fn build() { let s = Spacing::Px(12.0); }\n"}
        )
        self.assertEqual(len(violations), 1)
        self.assertIn("raw-pixel constructor Spacing::Px(", violations[0])

    def test_raw_radius_from_px_constructor_fails(self) -> None:
        violations = _run(
            {
                "crates/guimaker/src/view.rs": (
                    "fn build() { let r = Radius::from_px(6.0); }\n"
                )
            }
        )
        self.assertEqual(len(violations), 1)
        self.assertIn("raw-pixel constructor Radius::from_px(", violations[0])

    def test_main_prints_success_summary(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            checkout = Path(temporary)
            _write(
                checkout,
                "crates/guimaker/src/view.rs",
                "fn build(theme: &Theme) { view.pad(Spacing::S4); }\n",
            )
            output = io.StringIO()
            with contextlib.redirect_stdout(output):
                status = check_guimaker_literals.main(["--root", temporary])
        self.assertEqual(status, 0)
        self.assertEqual(
            output.getvalue().strip(),
            "guimaker literal guard: 1 files scanned, 0 violations",
        )

    def test_main_exits_one_and_prints_count(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            checkout = Path(temporary)
            _write(
                checkout,
                "crates/guimaker/src/view.rs",
                "fn build() { view.pad(12.0); }\n",
            )
            output = io.StringIO()
            with contextlib.redirect_stdout(output):
                status = check_guimaker_literals.main(["--root", temporary])
        self.assertEqual(status, 1)
        self.assertTrue(
            output.getvalue().rstrip().endswith(
                "guimaker literal guard: 1 violation(s)"
            )
        )


if __name__ == "__main__":
    unittest.main()
