#!/usr/bin/env python3
"""Check FerrousEngine's GUIMaker dependency boundaries.

GUIMaker is an optional design system layered on top of FerrousEngine. The
dependency graph must stay acyclic and optional:

- No engine crate may depend on a GUIMaker crate, not even as a dev- or
  build-dependency. A project that uses Ferrous without GUIMaker compiles
  none of it.
- The GUIMaker core (``guimaker``) may use only the GPU-free engine
  foundation listed in CORE_ALLOWED_ENGINE.
- The Ferrous drawing backend (``guimaker_ferrous``) may use that
  foundation plus the drawing crates in DRAWING_ALLOWED_ENGINE; only this
  layer may touch the GPU stack.
- ``guimaker_gallery`` and ``guimaker_designer`` are applications: they may
  use both sets *and* the application shell ``ferrous_engine`` (for the
  ``FerrousApp`` host adapter, winit input and the runner), but never the
  other way round.  This is deliberately the only extra edge applications
  get; ``guimaker`` stays headless and ``guimaker_ferrous`` stays a pure
  drawing backend so a product can embed either without the shell.
- The core may not depend back on the drawing GUIMaker crates (no cycle),
  and the core may not depend on a GPU or windowing crate directly.
- No GUIMaker crate may be built on egui, iced, Slint or a web view: the
  design system is ours.

Every workspace package is listed explicitly so a new crate cannot silently
escape the rules. The check reads ``cargo metadata --no-deps`` once and exits
0 when every edge is legal, 1 otherwise. It tolerates an empty GUIMaker layer
(Phase 0) while still checking the engine -> GUIMaker direction, so the rule
is already live before the crates are added.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

GUIMAKER_PREFIX = "guimaker"
GUIMAKER_CORE = "guimaker"
GUIMAKER_FERROUS = "guimaker_ferrous"
GUIMAKER_GALLERY = "guimaker_gallery"
GUIMAKER_DESIGNER = "guimaker_designer"

# Every non-GUIMaker workspace package must appear here. The values document
# which side of the boundary the crate lives on; the check itself only uses
# the keys, and treats each one as forbidden from depending on GUIMaker.
GUIMAKER_ENGINE_PACKAGES = {
    # Headless foundation: no wgpu in the mandatory dependency closure.
    "ferrous_core": "headless math/scene/input primitives",
    "ferrous_ecs": "headless entity-component-system",
    "ferrous_ecs_macros": "derive macros for ferrous_ecs",
    "ferrous_events": "event types; closure has no wgpu",
    "ferrous_reflection": "reflection metadata",
    "ferrous_state": "state handles",
    "ferrous_asset_types": "asset type descriptors",
    # Drawing / rendering: wgpu in the mandatory closure. ferrous_svg is
    # GPU-free but is still a drawing-domain crate.
    "ferrous_2d": "2D geometry and batching",
    "ferrous_font": "font parsing, atlas and layout",
    "ferrous_gpu": "wgpu device wrapper",
    "ferrous_render_graph": "render graph",
    "ferrous_renderer": "main renderer",
    "ferrous_svg": "SVG tessellation",
    "ferrous_ui_render": "UI batch and renderer",
    # Higher-level / application crates: the core and the drawing backend may
    # not use them.  Applications additionally get only `ferrous_engine`; see
    # APPLICATION_ALLOWED_ENGINE.  The check below still forbids every engine
    # crate (including ferrous_engine) from depending on GUIMaker.
    "ferrous_assets": "asset I/O; GPU and text are optional features",
    "ferrous_editor": "editor application",
    "ferrous_engine": "application shell",
    "ferrous_gui": "legacy UI facade",
    "ferrous_layout": "UI layout engine",
    "ferrous_ui_core": "UI widget toolkit",
    "ferrous_ui_macros": "UI derive macros",
    "ferrous_voxels": "3D voxel renderer",
    "ferrous_web": "wasm web application",
}

# Measured with `cargo metadata --no-deps`: none of these pulls wgpu through
# a mandatory normal dependency, so the design core can use them without
# dragging the GPU stack into a headless consumer.
CORE_ALLOWED_ENGINE = frozenset(
    {
        # GPU and windowing are optional features, off by default.
        "ferrous_core",
        # Derive macros are optional; no GPU in the default closure.
        "ferrous_ecs",
        "ferrous_ecs_macros",
        # winit is in the closure, but no wgpu.
        "ferrous_events",
        # Dependency-free metadata crates.
        "ferrous_reflection",
        "ferrous_state",
        "ferrous_asset_types",
    }
)

# The drawing backend is the only GUIMaker crate that may touch the GPU stack.
DRAWING_ALLOWED_ENGINE = frozenset(
    {
        "ferrous_2d",
        "ferrous_font",
        "ferrous_gpu",
        "ferrous_render_graph",
        "ferrous_renderer",
        # GPU-free, but explicitly a drawing-domain crate.
        "ferrous_svg",
        "ferrous_ui_render",
    }
)

# Applications (gallery, designer) additionally get the application shell
# ``ferrous_engine``: its ``FerrousApp`` + ``DrawContext`` is the host contract
# the gallery adapter implements, and the runner/winit types are how a real
# window is driven.  The core (``guimaker``) and the drawing backend
# (``guimaker_ferrous``) must NOT get this edge: they stay embeddable without
# the shell.  ``ferrous_engine`` itself may never depend on a GUIMaker crate
# (checked below), so the edge cannot create a cycle.
APPLICATION_ALLOWED_ENGINE = (
    CORE_ALLOWED_ENGINE | DRAWING_ALLOWED_ENGINE | {"ferrous_engine"}
)

GUIMAKER_ALLOWED_ENGINE = {
    GUIMAKER_CORE: CORE_ALLOWED_ENGINE,
    GUIMAKER_FERROUS: CORE_ALLOWED_ENGINE | DRAWING_ALLOWED_ENGINE,
    GUIMAKER_GALLERY: APPLICATION_ALLOWED_ENGINE,
    GUIMAKER_DESIGNER: APPLICATION_ALLOWED_ENGINE,
}

# Third-party UI stacks: a GUIMaker crate that depended on one of these would
# stop being our own design system.
GUIMAKER_BANNED_EXTERNAL = ("egui", "eframe", "iced", "slint", "naga")

# The core is headless by contract: no GPU, no window, no swapchain.
CORE_BANNED_EXTERNAL = ("wgpu", "winit", "raw-window-handle")


def _is_guimaker(name: str) -> bool:
    return name.startswith(GUIMAKER_PREFIX)


def _dependency_kind(dependency: dict) -> str:
    return dependency.get("kind") or "normal"


def _dependency_targets(dependency: dict) -> set[str]:
    # `name` is the package name; `rename` is the local alias in Cargo.toml.
    # Both are checked so a renamed edge cannot hide.
    return {
        name
        for name in (dependency.get("name"), dependency.get("rename"))
        if name
    }


def check(metadata: dict) -> list[str]:
    """Return one message per boundary violation in a cargo metadata document."""
    packages = metadata.get("packages", [])
    names = {package["name"] for package in packages}
    problems: list[str] = []

    for name in sorted(names):
        if _is_guimaker(name):
            if name not in GUIMAKER_ALLOWED_ENGINE:
                problems.append(
                    f"{name} has no boundary rule; "
                    "add it to GUIMAKER_ALLOWED_ENGINE"
                )
        elif name not in GUIMAKER_ENGINE_PACKAGES:
            problems.append(
                f"{name} has no boundary rule; "
                "add it to GUIMAKER_ENGINE_PACKAGES"
            )

    for package in sorted(packages, key=lambda item: item["name"]):
        name = package["name"]
        allowed = GUIMAKER_ALLOWED_ENGINE.get(name)
        dependencies = sorted(
            package.get("dependencies", []),
            key=lambda item: (item["name"], item.get("kind") or "normal"),
        )

        for dependency in dependencies:
            kind = _dependency_kind(dependency)
            targets = _dependency_targets(dependency)
            guimaker_targets = {
                target for target in targets if _is_guimaker(target)
            }

            if not _is_guimaker(name):
                for target in sorted(guimaker_targets):
                    problems.append(
                        f"{name} must not depend on {target} "
                        f"({kind} dependency)"
                    )
                continue

            if allowed is None:
                continue

            if name == GUIMAKER_CORE:
                for target in sorted(guimaker_targets - {name}):
                    problems.append(
                        f"{name} must not depend on {target} "
                        "(no cycle back into the drawing layer)"
                    )

            for target in sorted(targets - guimaker_targets):
                if target.startswith("ferrous_") and target not in allowed:
                    problems.append(
                        f"{name} must not depend on {target} "
                        f"({kind} dependency)"
                    )
                elif target.startswith(GUIMAKER_BANNED_EXTERNAL):
                    problems.append(
                        f"{name} must not depend on {target} "
                        "(GUIMaker does not build on a third-party UI stack)"
                    )
                elif (
                    name == GUIMAKER_CORE
                    and target.startswith(CORE_BANNED_EXTERNAL)
                ):
                    problems.append(
                        f"{name} must not depend on {target} "
                        "(the core is headless: no GPU, no windowing)"
                    )

    return problems


def _summary(metadata: dict) -> list[str]:
    """Return one compact line per GUIMaker crate, plus a fallback line."""
    lines: list[str] = []
    packages = sorted(
        metadata.get("packages", []), key=lambda item: item["name"]
    )

    for package in packages:
        name = package["name"]
        if name not in GUIMAKER_ALLOWED_ENGINE:
            continue
        engine_deps = {
            target
            for dependency in package.get("dependencies", [])
            for target in _dependency_targets(dependency)
            if target.startswith("ferrous_")
        }
        if name == GUIMAKER_CORE:
            mode = "core-only"
        elif name == GUIMAKER_FERROUS:
            mode = "core+drawing, gpu"
        else:
            mode = "core+drawing+engine"
        count = len(engine_deps)
        noun = "engine dep" if count == 1 else "engine deps"
        lines.append(f"{name}: {count} {noun} ({mode})")

    if not lines:
        lines.append(
            "no GUIMaker crates in the workspace yet; "
            "engine crates checked for reverse dependencies"
        )
    return lines


def main(argv: list[str] | None = None) -> int:
    del argv
    root = Path(__file__).resolve().parents[1]
    try:
        raw = subprocess.check_output(
            ["cargo", "metadata", "--format-version", "1", "--no-deps"],
            cwd=root,
            text=True,
            encoding="utf-8",
        )
        metadata: dict = json.loads(raw)
    except (OSError, subprocess.CalledProcessError, json.JSONDecodeError) as error:
        print(f"could not read cargo metadata: {error}", file=sys.stderr)
        return 1

    problems = check(metadata)
    if problems:
        print("Dependency boundary violations:")
        for problem in problems:
            print(f"  - {problem}")
        return 1

    print("Dependency boundaries OK")
    for line in _summary(metadata):
        print(line)
    return 0


if __name__ == "__main__":
    sys.exit(main())
