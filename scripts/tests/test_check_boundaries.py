"""Tests for ``scripts/check_boundaries.py``.

Run with ``python -m unittest discover -s scripts/tests``. The fixtures are
synthetic ``cargo metadata`` documents, so the tests never shell out to cargo.
"""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import check_boundaries


def _dependency(
    name: str, kind: str | None = None, rename: str | None = None
) -> dict:
    return {"name": name, "kind": kind, "rename": rename}


def _package(name: str, *dependencies: dict) -> dict:
    return {"name": name, "dependencies": list(dependencies)}


def _metadata(*packages: dict) -> dict:
    return {"packages": list(packages)}


def _workspace(*packages: dict) -> dict:
    """Build metadata with every known engine package, replacing duplicates."""
    by_name = {
        name: _package(name) for name in check_boundaries.GUIMAKER_ENGINE_PACKAGES
    }
    for package in packages:
        by_name[package["name"]] = package
    return _metadata(*by_name.values())


class BoundaryTest(unittest.TestCase):
    def test_clean_workspace_passes(self) -> None:
        metadata = _workspace(
            _package(
                "guimaker",
                _dependency("ferrous_core"),
                _dependency("ferrous_events"),
            ),
            _package(
                "guimaker_ferrous",
                _dependency("guimaker"),
                _dependency("ferrous_renderer"),
            ),
        )
        self.assertEqual(check_boundaries.check(metadata), [])

    def test_workspace_without_guimaker_still_passes(self) -> None:
        self.assertEqual(check_boundaries.check(_workspace()), [])

    def test_engine_depending_on_guimaker_fails(self) -> None:
        metadata = _workspace(
            _package("ferrous_core", _dependency("guimaker")),
            _package("guimaker"),
        )
        self.assertIn(
            "ferrous_core must not depend on guimaker (normal dependency)",
            check_boundaries.check(metadata),
        )

    def test_engine_dev_and_build_dependencies_on_guimaker_fail(self) -> None:
        for kind in ("dev", "build"):
            with self.subTest(kind=kind):
                metadata = _workspace(
                    _package("ferrous_core", _dependency("guimaker", kind=kind)),
                    _package("guimaker"),
                )
                self.assertIn(
                    f"ferrous_core must not depend on guimaker "
                    f"({kind} dependency)",
                    check_boundaries.check(metadata),
                )

    def test_core_depending_on_renderer_fails(self) -> None:
        metadata = _workspace(
            _package("guimaker", _dependency("ferrous_renderer")),
        )
        self.assertIn(
            "guimaker must not depend on ferrous_renderer "
            "(normal dependency)",
            check_boundaries.check(metadata),
        )

    def test_core_may_not_use_non_core_engine_crate(self) -> None:
        metadata = _workspace(
            _package("guimaker", _dependency("ferrous_ui_core")),
        )
        self.assertIn(
            "guimaker must not depend on ferrous_ui_core "
            "(normal dependency)",
            check_boundaries.check(metadata),
        )

    def test_core_may_not_depend_on_ferrous_backend(self) -> None:
        metadata = _workspace(
            _package("guimaker", _dependency("guimaker_ferrous")),
            _package("guimaker_ferrous"),
        )
        self.assertIn(
            "guimaker must not depend on guimaker_ferrous "
            "(no cycle back into the drawing layer)",
            check_boundaries.check(metadata),
        )

    def test_ferrous_depending_on_drawing_passes(self) -> None:
        metadata = _workspace(
            _package(
                "guimaker_ferrous",
                _dependency("guimaker"),
                _dependency("ferrous_2d"),
                _dependency("ferrous_font"),
                _dependency("ferrous_renderer"),
            ),
            _package("guimaker"),
        )
        self.assertEqual(check_boundaries.check(metadata), [])

    def test_ferrous_may_use_the_headless_foundation(self) -> None:
        metadata = _workspace(
            _package(
                "guimaker_ferrous",
                _dependency("guimaker"),
                _dependency("ferrous_core"),
                _dependency("ferrous_events"),
                _dependency("ferrous_renderer"),
            ),
            _package("guimaker"),
        )
        self.assertEqual(check_boundaries.check(metadata), [])

    def test_core_may_not_take_the_gpu_stack_directly(self) -> None:
        for dependency in ("wgpu", "winit"):
            with self.subTest(dependency=dependency):
                metadata = _workspace(_package("guimaker", _dependency(dependency)))
                self.assertIn(
                    f"guimaker must not depend on {dependency} "
                    "(the core is headless: no GPU, no windowing)",
                    check_boundaries.check(metadata),
                )

    def test_guimaker_may_not_build_on_another_ui_stack(self) -> None:
        for dependency in ("egui", "iced", "slint"):
            for name in ("guimaker", "guimaker_ferrous"):
                with self.subTest(dependency=dependency, name=name):
                    metadata = _workspace(_package(name, _dependency(dependency)))
                    self.assertIn(
                        f"{name} must not depend on {dependency} "
                        "(GUIMaker does not build on a third-party UI stack)",
                        check_boundaries.check(metadata),
                    )

    def test_gallery_may_use_core_and_drawing(self) -> None:
        metadata = _workspace(
            _package(
                "guimaker_gallery",
                _dependency("guimaker"),
                _dependency("guimaker_ferrous"),
                _dependency("ferrous_core"),
                _dependency("ferrous_renderer"),
            ),
            _package("guimaker"),
            _package("guimaker_ferrous"),
        )
        self.assertEqual(check_boundaries.check(metadata), [])

    def test_renamed_dependency_on_guimaker_fails(self) -> None:
        metadata = _workspace(
            _package("ferrous_core", _dependency("guimaker", rename="gm")),
            _package("guimaker"),
        )
        self.assertIn(
            "ferrous_core must not depend on guimaker (normal dependency)",
            check_boundaries.check(metadata),
        )

    def test_guimaker_rename_alias_is_checked(self) -> None:
        metadata = _workspace(
            _package("ferrous_core", _dependency("gm", rename="guimaker")),
            _package("guimaker"),
        )
        self.assertIn(
            "ferrous_core must not depend on guimaker (normal dependency)",
            check_boundaries.check(metadata),
        )

    def test_summary_counts_engine_dependencies(self) -> None:
        metadata = _workspace(
            _package(
                "guimaker",
                _dependency("ferrous_core"),
                _dependency("ferrous_events"),
            ),
            _package("guimaker_ferrous", _dependency("ferrous_renderer")),
        )
        self.assertEqual(
            check_boundaries._summary(metadata),
            [
                "guimaker: 2 engine deps (core-only)",
                "guimaker_ferrous: 1 engine dep (core+drawing, gpu)",
            ],
        )

    def test_summary_without_guimaker_has_fallback_line(self) -> None:
        self.assertEqual(
            check_boundaries._summary(_workspace()),
            [
                "no GUIMaker crates in the workspace yet; "
                "engine crates checked for reverse dependencies"
            ],
        )

    def test_unlisted_workspace_package_fails(self) -> None:
        metadata = _workspace(_package("ferrous_future"))
        self.assertIn(
            "ferrous_future has no boundary rule; "
            "add it to GUIMAKER_ENGINE_PACKAGES",
            check_boundaries.check(metadata),
        )


if __name__ == "__main__":
    unittest.main()
