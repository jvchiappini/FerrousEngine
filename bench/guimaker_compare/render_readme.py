#!/usr/bin/env python3
"""Render the Phase 9D performance section into both READMEs.

Reads the JSON written by `run_compare.py` (no hand-typed numbers), verifies
that every row has a measured `reason`, and rewrites only the text between:

    <!-- PHASE9D-PERFORMANCE:START -->
    <!-- PHASE9D-PERFORMANCE:END -->

Standard library only. Windows is not required to render existing results.
"""

from __future__ import annotations

import argparse
import json
import statistics
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent
REPO = ROOT.parent.parent
RESULTS = ROOT / "results"

MARK_START = "<!-- PHASE9D-PERFORMANCE:START -->"
MARK_END = "<!-- PHASE9D-PERFORMANCE:END -->"

MB = 1024 * 1024

TIER_APPS = {
    "vulkan": ["guimaker", "egui-wgpu", "iced", "imgui", "floor-wgpu-vulkan"],
    "dx12": ["guimaker", "egui-wgpu", "iced", "imgui", "floor-wgpu-dx12"],
    "gl": ["guimaker", "egui", "slint", "floor-glow"],
}
TIER_LABEL = {
    "vulkan": "Vulkan / wgpu",
    "dx12": "DX12 / wgpu",
    "gl": "OpenGL",
}
FLOORS = ["floor-wgpu-vulkan", "floor-wgpu-dx12", "floor-wgpu-gl", "floor-glow"]
FLOOR_TIER = {
    "floor-wgpu-vulkan": "vulkan",
    "floor-wgpu-dx12": "dx12",
    "floor-wgpu-gl": "gl",
    "floor-glow": "gl",
}
LIB_ORDER = ["guimaker", "egui", "egui-wgpu", "iced", "slint", "imgui"]
SCENARIO_ORDER = ["hover_body", "hover_button", "typing", "resize", "menu"]
SCENARIO_LABEL = {
    "hover_body": "Body hover sweep (6 s)",
    "hover_button": "Button hover (6 s)",
    "typing": "Typing at 10 chars/s after a real click (10 s)",
    "resize": "Resize (8 s)",
    "menu": "Open + close a menu (8 s)",
}
REPRODUCE = (
    "python bench/guimaker_compare/run_compare.py --confirmed-host-free --runs 5 "
    "--scenario idle6 --scenario idle30 --scenario hover_body --scenario hover_button "
    "--scenario typing --scenario resize --scenario menu --tier vulkan --tier dx12 "
    "--tier gl --interactive-tier vulkan --write-readme"
)


# ── data loading ─────────────────────────────────────────────────────────────


def load_rows(results_dir: Path) -> tuple[list[dict], str | None]:
    candidates: list[dict] = []
    for path in results_dir.rglob("*-run*.json"):
        if "fonts-default" in path.parts:
            continue  # the equal-font table is the primary one
        try:
            row = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, ValueError):
            continue
        if not isinstance(row, dict):
            continue
        if not all(key in row for key in ("scenario", "tier", "library", "run")):
            continue
        if "floor" not in row:
            # Phase 9B/9C rows predate this protocol; never mix them in.
            continue
        candidates.append(row)
    session_ids = sorted(
        {row["session_id"] for row in candidates if row.get("session_id")}
    )
    if not session_ids:
        return [], None
    session_id = session_ids[-1]
    rows = [row for row in candidates if row.get("session_id") == session_id]
    return rows, session_id


def load_environment(results_dir: Path, session_id: str | None) -> dict:
    for name in ("environment.json", "fonts-default/environment.json"):
        path = results_dir / name
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, ValueError):
            continue
        if isinstance(data, dict):
            return data
    try:
        summary = json.loads((results_dir / "summary.json").read_text(encoding="utf-8"))
        if isinstance(summary.get("environment"), dict):
            return summary["environment"]
    except (OSError, ValueError):
        pass
    return {}


def load_versions(results_dir: Path, rows: list[dict]) -> dict[str, dict]:
    try:
        data = json.loads((results_dir / "versions.json").read_text(encoding="utf-8"))
        if isinstance(data, dict):
            return data
    except (OSError, ValueError):
        pass
    versions: dict[str, dict] = {}
    for row in rows:
        if isinstance(row.get("versions"), dict):
            versions.setdefault(row["library"], row["versions"])
    return versions


def group(rows: list[dict]) -> dict[tuple[str, str, str], list[dict]]:
    groups: dict[tuple[str, str, str], list[dict]] = {}
    for row in rows:
        key = (row["scenario"], row["tier"], row["library"])
        groups.setdefault(key, []).append(row)
    for entries in groups.values():
        entries.sort(key=lambda item: item.get("run", 0))
    return groups


# ── statistics / formatting ──────────────────────────────────────────────────


def triple(entries: list[dict], key: str):
    values = [entry.get(key) for entry in entries if entry.get(key) is not None]
    if not values:
        return None
    return statistics.median(values), min(values), max(values)


def pixel_triple(entries: list[dict]):
    values = []
    for entry in entries:
        diff = entry.get("pixel_diff") or {}
        if diff.get("comparable"):
            values.append(diff.get("changed_pixel_ratio", 0.0))
    if not values:
        return None
    return statistics.median(values), min(values), max(values)


def fmt(value, digits: int = 2, scale: float = 1.0) -> str:
    if value is None:
        return "n/a"
    return f"{value / scale:.{digits}f}"


def fmt_triple(value, digits: int = 2, scale: float = 1.0, integer: bool = False) -> str:
    if not value:
        return "n/a"
    median, low, high = value
    if integer:
        return f"{int(round(median)):,} [{int(round(low)):,}..{int(round(high)):,}]"
    return (
        f"{median / scale:.{digits}f} "
        f"[{low / scale:.{digits}f}..{high / scale:.{digits}f}]"
    )


def cell(text: str) -> str:
    return str(text).replace("|", "\\|").replace("\n", " ").strip()


def apps_for(
    groups: dict[tuple[str, str, str], list[dict]], scenario: str, tier: str
) -> list[str]:
    known = list(TIER_APPS[tier])
    extras = sorted(
        {
            lib
            for (group_scenario, group_tier, lib) in groups
            if group_scenario == scenario
            and group_tier == tier
            and lib not in known
        }
    )
    return known + extras


def reason_of(entries: list[dict], key: tuple[str, str, str]) -> str:
    for entry in entries:
        reason = entry.get("reason")
        if reason:
            return reason
    raise SystemExit(
        "missing measured reason for "
        f"{key[0]}/{key[1]}/{key[2]}: run_compare.py must annotate every row"
    )


def winners(groups: dict[tuple[str, str, str], list[dict]]) -> dict[tuple[str, str], str]:
    by_tier: dict[tuple[str, str], list[tuple[tuple, str]]] = {}
    for (scenario, tier, lib), entries in groups.items():
        if lib in FLOORS:
            continue
        cpu = triple(entries, "idle_cpu_pct")
        frames = triple(entries, "frames_external")
        rss = triple(entries, "rss_bytes")
        score = (
            cpu[0] if cpu else float("inf"),
            frames[0] if frames else float("inf"),
            rss[0] if rss else float("inf"),
        )
        by_tier.setdefault((scenario, tier), []).append((score, lib))
    return {key: min(items)[1] for key, items in by_tier.items() if items}


# ── rendering ────────────────────────────────────────────────────────────────


def render_setup(environment: dict, session_id: str | None) -> list[str]:
    gpus = environment.get("gpus") or []
    if gpus:
        gpu_text = "<br>".join(
            cell(f"{gpu.get('name', 'GPU')} - driver {gpu.get('driver_version') or 'unknown'}")
            for gpu in gpus
        )
    else:
        gpu_text = "unknown"
    lines = [
        "### Measurement setup",
        "",
        "| Field | Value |",
        "|---|---|",
        f"| Measured | {cell(environment.get('measurement_date') or 'unknown')} |",
        f"| Session | `{cell(session_id or environment.get('session_id') or 'unknown')}` |",
        f"| Host | {cell(environment.get('host') or 'unknown')} |",
        f"| OS | {cell(environment.get('os') or 'unknown')} |",
        f"| CPU | {cell(environment.get('cpu') or 'unknown')} |",
        f"| GPU / driver | {gpu_text} |",
        f"| Display scale | {fmt(environment.get('dpi_scale'), 2)}x |",
        f"| Python | {cell(environment.get('python') or 'unknown')} |",
        "| Protocol | [PROTOCOL.md]({protocol}) |",
        "| Reproduce | `" + REPRODUCE + "` |",
        "",
        "All numbers below are **medians over five alternating runs**, with the "
        "range in brackets. Presents and startup come from the OS (PresentMon or "
        "`wpr` ETW), not from the apps. Every row, including rows where GUIMaker "
        "does not win, carries a measured `reason`.",
        "",
    ]
    return lines


def render_versions(versions: dict[str, dict], apps: list[str]) -> list[str]:
    lines = ["### Library versions", "", "| App | Versions |", "|---|---|"]
    for app in apps:
        table = versions.get(app) or {}
        text = ", ".join(f"{name} {version}" for name, version in table.items())
        lines.append(f"| `{app}` | {cell(text or 'not recorded')} |")
    lines.append("")
    return lines


def render_tier(
    groups: dict[tuple[str, str, str], list[dict]],
    win: dict[tuple[str, str], str],
    tier: str,
    scenario: str,
    heading: str,
) -> list[str]:
    lines = [f"#### {heading}", ""]
    if scenario == "idle6":
        lines += [
            "| App | Startup ms | Idle CPU % | Presents | RSS MB | GPU MB | Pixel diff % | Reason |",
            "|---|---|---|---|---|---|---|---|",
        ]
    else:
        lines += [
            "| App | Tail CPU % | Tail top thread | Presents | Reason |",
            "|---|---|---|---|---|",
        ]
    any_row = False
    for app in apps_for(groups, scenario, tier):
        entries = groups.get((scenario, tier, app))
        label = f"**{app}** (floor)" if app in FLOORS else app
        if not entries:
            if scenario == "idle6":
                lines.append(
                    f"| {label} | not measured | not measured | not measured | "
                    "not measured | not measured | not measured | "
                    "not measured in this session (no JSON). |"
                )
            else:
                lines.append(
                    f"| {label} | not measured | not measured | not measured | "
                    "not measured in this session (no JSON). |"
                )
            continue
        any_row = True
        reason = reason_of(entries, (scenario, tier, app))
        if scenario == "idle6":
            lines.append(
                "| {app} | {startup} | {cpu} | {frames} | {rss} | {gpu} | {pixels} | {reason} |".format(
                    app=label,
                    startup=fmt_triple(triple(entries, "startup_ms"), 0),
                    cpu=fmt_triple(triple(entries, "idle_cpu_pct"), 2),
                    frames=fmt_triple(triple(entries, "frames_external"), 0, integer=True),
                    rss=fmt_triple(triple(entries, "rss_bytes"), 1, MB),
                    gpu=fmt_triple(triple(entries, "gpu_bytes"), 1, MB),
                    pixels=fmt_triple(pixel_triple(entries), 2, 0.01),
                    reason=cell(reason),
                )
            )
        else:
            tail_thread = next(
                (
                    entry.get("tail_top_thread")
                    for entry in entries
                    if entry.get("tail_top_thread")
                ),
                "not named",
            )
            lines.append(
                "| {app} | {cpu} | {thread} | {frames} | {reason} |".format(
                    app=label,
                    cpu=fmt_triple(triple(entries, "tail_cpu_pct"), 2),
                    thread=cell(tail_thread),
                    frames=fmt_triple(triple(entries, "frames_external"), 0, integer=True),
                    reason=cell(reason),
                )
            )
    if not any_row:
        lines.append("| _No measurements in this session._ | | | | | | | |")
    lines.append("")
    return lines


def render_floors(groups: dict[tuple[str, str, str], list[dict]]) -> list[str]:
    lines = [
        "### Raw-API floor (clear only, no UI/text)",
        "",
        "| Floor | Tier | Startup ms | Idle CPU % | Presents | RSS MB | GPU MB | Pixel diff % | Reason |",
        "|---|---|---|---|---|---|---|---|---|",
    ]
    for floor in FLOORS:
        tier = FLOOR_TIER[floor]
        entries = groups.get(("idle6", tier, floor))
        if not entries:
            lines.append(
                f"| `{floor}` | {tier} | not measured | not measured | not measured | "
                "not measured | not measured | not measured | not measured in this session. |"
            )
            continue
        lines.append(
            "| `{floor}` | {tier} | {startup} | {cpu} | {frames} | {rss} | {gpu} | {pixels} | {reason} |".format(
                floor=floor,
                tier=tier,
                startup=fmt_triple(triple(entries, "startup_ms"), 0),
                cpu=fmt_triple(triple(entries, "idle_cpu_pct"), 2),
                frames=fmt_triple(triple(entries, "frames_external"), 0, integer=True),
                rss=fmt_triple(triple(entries, "rss_bytes"), 1, MB),
                gpu=fmt_triple(triple(entries, "gpu_bytes"), 1, MB),
                pixels=fmt_triple(pixel_triple(entries), 2, 0.01),
                reason=cell(reason_of(entries, ("idle6", tier, floor))),
            )
        )
    lines.append("")
    return lines


def render_best_backend(groups: dict[tuple[str, str, str], list[dict]]) -> list[str]:
    lines = [
        "### Best backend per library on this OS",
        "",
        "Chosen from the measured idle-6 s groups by fewest external presents, "
        "then lowest idle CPU, then lowest RSS.",
        "",
        "| App | Best tier | Idle CPU % | Presents | Startup ms | RSS MB | Reason |",
        "|---|---|---|---|---|---|---|",
    ]
    extras = sorted(
        {lib for (_, _, lib) in groups if lib not in LIB_ORDER and lib not in FLOORS}
    )
    apps = LIB_ORDER + FLOORS + extras
    for app in apps:
        options = []
        for tier in TIER_APPS:
            entries = groups.get(("idle6", tier, app))
            if not entries:
                continue
            cpu = triple(entries, "idle_cpu_pct")
            frames = triple(entries, "frames_external")
            rss = triple(entries, "rss_bytes")
            score = (
                frames[0] if frames else float("inf"),
                cpu[0] if cpu else float("inf"),
                rss[0] if rss else float("inf"),
            )
            options.append((score, tier, entries))
        if not options:
            lines.append(f"| `{app}` | not measured | not measured | not measured | not measured | not measured | not measured. |")
            continue
        _, tier, entries = min(options, key=lambda item: item[0])
        measured_tiers = ", ".join(tier_name for _, tier_name, _ in options)
        if app in FLOORS:
            reason = f"floor measured only on {measured_tiers}."
        else:
            reason = (
                f"best of the measured tiers ({measured_tiers}) on presents, then "
                "idle CPU; chosen from the JSON."
            )
        lines.append(
            "| `{app}` | {tier} | {cpu} | {frames} | {startup} | {rss} | {reason} |".format(
                app=app,
                tier=tier,
                cpu=fmt_triple(triple(entries, "idle_cpu_pct"), 2),
                frames=fmt_triple(triple(entries, "frames_external"), 0, integer=True),
                startup=fmt_triple(triple(entries, "startup_ms"), 0),
                rss=fmt_triple(triple(entries, "rss_bytes"), 1, MB),
                reason=cell(reason),
            )
        )
    lines.append("")
    return lines


def render_interactive(
    groups: dict[tuple[str, str, str], list[dict]], rows: list[dict]
) -> list[str]:
    measured = {
        row["tier"] for row in rows if row.get("scenario") in SCENARIO_ORDER
    }
    tier = sorted(measured)[0] if measured else "vulkan"
    lines = [
        f"### Interactive scenarios (tier {tier})",
        "",
        "Every interactive row is listed; no scenario is cherry-picked.",
        "",
    ]
    for scenario in SCENARIO_ORDER:
        present = any(
            (scenario, tier, app) in groups
            for app in apps_for(groups, scenario, tier)
            if app not in FLOORS
        )
        if not present:
            continue
        lines += [f"#### {SCENARIO_LABEL[scenario]}", ""]
        if scenario == "typing":
            lines += [
                "| App | CPU % | Presents | RSS MB | Typing chars | Pixel diff % | Reason |",
                "|---|---|---|---|---|---|---|",
            ]
        else:
            lines += [
                "| App | CPU % | Presents | RSS MB | Pixel diff % | Reason |",
                "|---|---|---|---|---|---|",
            ]
        for app in apps_for(groups, scenario, tier):
            if app in FLOORS:
                continue
            entries = groups.get((scenario, tier, app))
            if not entries:
                if scenario == "typing":
                    lines.append(
                        f"| {app} | not measured | not measured | not measured | "
                        "not measured | not measured | not measured in this session. |"
                    )
                else:
                    lines.append(
                        f"| {app} | not measured | not measured | not measured | "
                        "not measured | not measured in this session. |"
                    )
                continue
            reason = cell(reason_of(entries, (scenario, tier, app)))
            if scenario == "typing":
                lines.append(
                    "| {app} | {cpu} | {frames} | {rss} | {chars} | {pixels} | {reason} |".format(
                        app=app,
                        cpu=fmt_triple(triple(entries, "idle_cpu_pct"), 2),
                        frames=fmt_triple(triple(entries, "frames_external"), 0, integer=True),
                        rss=fmt_triple(triple(entries, "rss_bytes"), 1, MB),
                        chars=fmt_triple(triple(entries, "typing_chars"), 0, integer=True),
                        pixels=fmt_triple(pixel_triple(entries), 2, 0.01),
                        reason=reason,
                    )
                )
            else:
                lines.append(
                    "| {app} | {cpu} | {frames} | {rss} | {pixels} | {reason} |".format(
                        app=app,
                        cpu=fmt_triple(triple(entries, "idle_cpu_pct"), 2),
                        frames=fmt_triple(triple(entries, "frames_external"), 0, integer=True),
                        rss=fmt_triple(triple(entries, "rss_bytes"), 1, MB),
                        pixels=fmt_triple(pixel_triple(entries), 2, 0.01),
                        reason=reason,
                    )
                )
        lines.append("")
    return lines


def render_block(
    rows: list[dict],
    session_id: str | None,
    environment: dict,
    versions: dict[str, dict],
    protocol_link: str,
) -> str:
    if not rows:
        return (
            "## Performance (Phase 9D)\n\n"
            "Not measured yet; run `" + REPRODUCE + "` "
            "(see [PROTOCOL.md](" + protocol_link + ")) to fill this section.\n"
        )

    groups = group(rows)
    win = winners(groups)
    extras = sorted(
        {lib for (_, _, lib) in groups if lib not in LIB_ORDER and lib not in FLOORS}
    )
    apps = LIB_ORDER + FLOORS + extras
    lines: list[str] = ["## Performance (Phase 9D)", ""]
    setup = render_setup(environment, session_id)
    setup = [line.replace("{protocol}", protocol_link) for line in setup]
    lines += setup
    lines += render_versions(versions, apps)

    lines += ["### Per-tier results", ""]
    for tier in ("vulkan", "dx12", "gl"):
        lines += render_tier(
            groups, win, tier, "idle6", f"{TIER_LABEL[tier]} - idle 6 s"
        )
        if any(
            ("idle30", tier, app) in groups
            for app in apps_for(groups, "idle30", tier)
        ):
            lines += render_tier(
                groups,
                win,
                tier,
                "idle30",
                f"{TIER_LABEL[tier]} - idle 30 s tail (top thread named)",
            )

    lines += render_floors(groups)
    lines += render_best_backend(groups)
    lines += render_interactive(groups, rows)

    missing = [
        app
        for app in apps
        if not any(key[2] == app and key[0] == "idle6" for key in groups)
    ]
    if missing:
        lines += [
            "### Not measured in this session",
            "",
            ", ".join(f"`{app}`" for app in missing)
            + " - no JSON for those apps. Nothing is omitted silently.",
            "",
        ]
    return "\n".join(lines).rstrip() + "\n"


# ── README editing ───────────────────────────────────────────────────────────


def replace_section(path: Path, block: str, check: bool = False) -> bool:
    with path.open("r", encoding="utf-8", newline="") as handle:
        text = handle.read()
    if MARK_START not in text or MARK_END not in text:
        raise SystemExit(f"{path}: missing {MARK_START} / {MARK_END} markers")
    newline = "\r\n" if "\r\n" in text else "\n"
    start = text.index(MARK_START) + len(MARK_START)
    end = text.index(MARK_END, start)
    rendered = block.strip().replace("\n", newline)
    updated = (
        text[:start] + newline + newline + rendered + newline + newline + text[end:]
    )
    changed = updated != text
    if changed and not check:
        # newline="" keeps the rest of the file byte-for-byte; only the marked
        # block changes.
        with path.open("w", encoding="utf-8", newline="") as handle:
            handle.write(updated)
    return changed


def render_and_write(
    results_dir: Path | None = None, repo: Path | None = None, check: bool = False
) -> int:
    results_dir = Path(results_dir) if results_dir else RESULTS
    repo = Path(repo) if repo else REPO
    rows, session_id = load_rows(results_dir)
    environment = load_environment(results_dir, session_id)
    if not environment:
        environment = {}
    if not environment.get("measurement_date"):
        dates = [row.get("measured_at") for row in rows if row.get("measured_at")]
        if dates:
            environment["measurement_date"] = max(dates)
    versions = load_versions(results_dir, rows)

    targets = [
        (repo / "README.md", "bench/guimaker_compare/PROTOCOL.md"),
        (
            repo / "crates" / "guimaker" / "README.md",
            "../../bench/guimaker_compare/PROTOCOL.md",
        ),
    ]
    changed = False
    for path, link in targets:
        block = render_block(rows, session_id, environment, versions, link)
        file_changed = replace_section(path, block, check=check)
        changed = file_changed or changed
        if check:
            state = "would update" if file_changed else "unchanged"
        else:
            state = "updated" if file_changed else "unchanged"
        print(f"{path}: {state}")
    if not rows:
        print(
            f"No Phase 9D results in {results_dir}; wrote the not-measured placeholder.",
            file=sys.stderr,
        )
    if check and changed:
        return 1
    return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Render the Phase 9D README section")
    parser.add_argument("--results", type=Path, default=RESULTS)
    parser.add_argument("--repo", type=Path, default=REPO)
    parser.add_argument(
        "--check",
        action="store_true",
        help="exit 1 if either README would change; write nothing",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    return render_and_write(args.results, args.repo, check=args.check)


if __name__ == "__main__":
    sys.exit(main())
