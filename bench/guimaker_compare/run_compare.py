#!/usr/bin/env python3
"""Run the Phase 9B comparison apps and store raw JSON.

The protocol is fixed by README.md: release builds, the same vsync mode, window
size and DPI, five runs per app, median and range. This script only orchestrates;
it does not build unless `--build` is passed.

STOP and ask the user to free the host before running this. The baseline perf
sweep and these comparison runs are the measurement step.
"""

from __future__ import annotations

import argparse
import json
import statistics
import subprocess
import sys
import threading
import time
from pathlib import Path

try:
    import psutil
except ImportError:  # pragma: no cover - the measurement host has it
    raise SystemExit("psutil is required")

ROOT = Path(__file__).resolve().parent
LIBS = ["guimaker", "egui", "egui-wgpu", "iced", "slint", "imgui", "webview2"]
EXE = {
    lib: ROOT / "target" / "release" / f"guimaker-compare-{lib}.exe" for lib in LIBS
}
TARGET_DIR = ROOT / "target"
TITLE = {lib: f"guimaker-compare-{lib}" for lib in LIBS}


def window_rect(title: str):
    """The window rect for `title`, or None (Windows only)."""
    try:
        import ctypes
        from ctypes import wintypes

        user32 = ctypes.windll.user32
        hwnd = user32.FindWindowW(None, title)
        if not hwnd:
            return None
        rect = wintypes.RECT()
        if not user32.GetWindowRect(hwnd, ctypes.byref(rect)):
            return None
        return rect.left, rect.top, rect.right, rect.bottom
    except Exception:
        return None


def hover_sweep(title: str, duration: float) -> int:
    """Moves the pointer across the window for `duration` seconds."""
    try:
        import ctypes

        user32 = ctypes.windll.user32
        rect = None
        wait_until = time.monotonic() + 3.0
        while time.monotonic() < wait_until:
            rect = window_rect(title)
            if rect is not None:
                break
            time.sleep(0.05)
        if rect is None:
            return 0
        left, top, right, bottom = rect
        moves = 0
        deadline = time.monotonic() + duration
        while time.monotonic() < deadline:
            # Sweep the body, where the cards and fields are.
            for y in (top + 120, top + 240, top + 360):
                x = left + 80
                while x < right - 80:
                    user32.SetCursorPos(int(x), int(y))
                    moves += 1
                    time.sleep(1.0 / 120.0)
                    x += 24
                    if time.monotonic() >= deadline:
                        break
        return moves
    except Exception:
        return 0


def parse_startup(stderr: str) -> tuple[float | None, str | None]:
    """The `engine: first frame presented` mark from the startup JSON line."""
    for line in stderr.splitlines():
        line = line.strip()
        if not line.startswith("startup-trace "):
            continue
        try:
            payload = json.loads(line.split(" ", 1)[1])
        except (ValueError, IndexError):
            continue
        for mark in payload.get("trace", {}).get("marks", []):
            if mark.get("name") == "engine: first frame presented":
                return float(mark.get("at_ms")), payload.get("stage")
    return None, None



def thread_cpu_seconds(tid: int) -> float:
    """Cumulative user+kernel seconds for a native thread (Windows)."""
    try:
        import ctypes
        from ctypes import wintypes

        class FILETIME(ctypes.Structure):
            _fields_ = [
                ("dwLowDateTime", wintypes.DWORD),
                ("dwHighDateTime", wintypes.DWORD),
            ]

        kernel32 = ctypes.windll.kernel32
        handle = kernel32.OpenThread(0x0040, False, tid)  # THREAD_QUERY_INFORMATION
        if not handle:
            return 0.0
        creation, exit_time, kernel, user = FILETIME(), FILETIME(), FILETIME(), FILETIME()
        ok = kernel32.GetThreadTimes(
            handle,
            ctypes.byref(creation),
            ctypes.byref(exit_time),
            ctypes.byref(kernel),
            ctypes.byref(user),
        )
        kernel32.CloseHandle(handle)
        if not ok:
            return 0.0

        def ticks(value):
            return (int(value.dwHighDateTime) << 32) | int(value.dwLowDateTime)

        return (ticks(kernel) + ticks(user)) / 10_000_000.0
    except Exception:
        return 0.0


def window_thread_id(title: str):
    try:
        import ctypes

        user32 = ctypes.windll.user32
        hwnd = user32.FindWindowW(None, title)
        if not hwnd:
            return None
        pid = ctypes.c_ulong()
        tid = user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
        return int(tid) if tid else None
    except Exception:
        return None


def thread_snapshot(ps) -> dict[int, float]:
    try:
        return {thread.id: thread_cpu_seconds(thread.id) for thread in ps.threads()}
    except psutil.Error:
        return {}

def park_cursor() -> None:
    try:
        import ctypes
        ctypes.windll.user32.SetCursorPos(0, 0)
    except Exception:
        pass


def run_once(lib: str, seconds: float, hover: bool) -> dict:
    global TARGET_DIR
    exe = TARGET_DIR / "release" / f"guimaker-compare-{lib}.exe"
    if not exe.is_file():
        return {"library": lib, "error": f"missing {exe}"}
    env = dict(**__import__("os").environ)
    env.setdefault("FERROUS_TRACE_STARTUP", "1")
    env.setdefault("FERROUS_TRACE_STARTUP_JSON", "1")
    if not hover:
        park_cursor()
    started = time.monotonic()
    # Redirect to files, not pipes: the engine's closing report is large and a
    # full pipe buffer would block the child before it exits.
    import tempfile
    out_file = tempfile.NamedTemporaryFile(
        "w", suffix=".out", prefix=f"{lib}-", delete=False, encoding="utf-8"
    )
    err_file = tempfile.NamedTemporaryFile(
        "w", suffix=".err", prefix=f"{lib}-", delete=False, encoding="utf-8"
    )
    proc = subprocess.Popen(
        [str(exe), "--seconds", str(seconds)],
        stdout=out_file,
        stderr=err_file,
        text=True,
        env=env,
    )
    ps = psutil.Process(proc.pid)
    ps.cpu_percent(None)
    samples: list[float] = []
    rss: list[int] = []
    moves = 0
    hover_thread = None
    if hover:
        hover_thread = threading.Thread(
            target=hover_sweep, args=(TITLE[lib], seconds), daemon=True
        )
        hover_thread.start()
    first_frame_wall = None
    warmup_end = started + 1.0
    snap_start: dict[int, float] = {}
    snap_end: dict[int, float] = {}
    ui_tid = None
    while proc.poll() is None:
        now = time.monotonic()
        if first_frame_wall is None:
            try:
                out_so_far = Path(out_file.name).read_text(encoding="utf-8", errors="replace")
                err_so_far = Path(err_file.name).read_text(encoding="utf-8", errors="replace")
                if "startup-trace " in err_so_far or "startup_ms " in out_so_far:
                    first_frame_wall = (now - started) * 1000.0
            except OSError:
                pass
        if now >= warmup_end:
            try:
                samples.append(ps.cpu_percent(None))
                rss.append(ps.memory_info().rss)
            except psutil.Error:
                break
        if not snap_start and now >= warmup_end:
            snap_start = thread_snapshot(ps)
        if not snap_end and now >= started + max(1.0, seconds - 1.0):
            snap_end = thread_snapshot(ps)
            ui_tid = window_thread_id(TITLE[lib])
        time.sleep(0.05)
    if hover_thread is not None:
        hover_thread.join(timeout=1.0)
    try:
        proc.wait(timeout=seconds + 20)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait()
    out_file.close()
    err_file.close()
    out = Path(out_file.name).read_text(encoding="utf-8", errors="replace")
    err = Path(err_file.name).read_text(encoding="utf-8", errors="replace")
    Path(out_file.name).unlink(missing_ok=True)
    Path(err_file.name).unlink(missing_ok=True)
    startup, stage = parse_startup(err)
    memory = None
    for line in err.splitlines():
        if line.startswith("memory-report "):
            try:
                memory = json.loads(line.split(" ", 1)[1])
            except ValueError:
                pass
    frames = None
    for line in out.splitlines():
        line = line.strip()
        if line.startswith("frames "):
            frames = int(line.split(" ", 1)[1])
        elif line.startswith("startup_ms ") and startup is None:
            try:
                startup = float(line.split(" ", 1)[1])
                stage = "app"
            except ValueError:
                pass
    cpu = statistics.fmean(samples[10:]) if len(samples) > 10 else (
        statistics.fmean(samples) if samples else 0.0
    )
    tail = samples[-200:] if len(samples) > 200 else samples
    tail_cpu = statistics.fmean(tail) if tail else 0.0
    if first_frame_wall is not None:
        startup = first_frame_wall
        stage = "harness-wall"
    deltas = []
    for tid, value in snap_end.items():
        before = snap_start.get(tid, 0.0)
        deltas.append((tid, round(value - before, 6)))
    deltas.sort(key=lambda item: item[1], reverse=True)
    threads = deltas[:5]
    ui_thread_cpu = dict(deltas).get(ui_tid) if ui_tid is not None else None
    return {
        "library": lib,
        "run": 0,
        "startup_ms": startup,
        "startup_stage": stage,
        "idle_cpu_pct": round(cpu, 3),
        "tail_cpu_pct": round(tail_cpu, 3),
        "top_threads_cpu": threads,
        "ui_thread_id": ui_tid,
        "ui_thread_cpu_seconds": ui_thread_cpu,
        "idle_frames": frames,
        "rss_bytes": max(rss) if rss else 0,
        "wall_seconds": round(time.monotonic() - started, 3),
        "hover_moves": moves,
        "memory": memory,
        "features": {},
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument("--seconds", type=float, default=6.0)
    parser.add_argument("--out", type=Path, default=ROOT / "results")
    parser.add_argument("--only", nargs="*", default=LIBS)
    parser.add_argument("--build", action="store_true")
    parser.add_argument("--hover", action="store_true")
    parser.add_argument("--target-dir", type=Path, default=ROOT / "target")
    args = parser.parse_args()
    global TARGET_DIR
    TARGET_DIR = args.target_dir

    if args.build:
        subprocess.run(
            ["cargo", "build", "--release", "--workspace"],
            cwd=ROOT,
            check=True,
        )

    args.out.mkdir(parents=True, exist_ok=True)
    summary: dict[str, list[dict]] = {}
    for lib in args.only:
        rows = []
        for run in range(1, args.runs + 1):
            row = run_once(lib, args.seconds, args.hover)
            row["run"] = run
            rows.append(row)
            path = args.out / f"{lib}-run{run}.json"
            with path.open("w", encoding="utf-8", newline="\\n") as handle:
                handle.write(json.dumps(row, indent=2))
            print(f"{lib} run {run}: {row}")
        summary[lib] = rows

    print("\nsummary (median [min..max])")
    for lib, rows in summary.items():
        ok = [r for r in rows if "error" not in r]
        if not ok:
            print(f"  {lib}: no runs ({rows[0].get('error')})")
            continue

        def stat(key: str):
            values = [r[key] for r in ok if r.get(key) is not None]
            if not values:
                return "n/a"
            return f"{statistics.median(values):.3f} [{min(values):.3f}..{max(values):.3f}]"

        print(
            f"  {lib}: startup {stat('startup_ms')} ms, "
            f"idle {stat('idle_cpu_pct')} %, rss {stat('rss_bytes')} B, "
            f"frames {stat('idle_frames')}"
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
