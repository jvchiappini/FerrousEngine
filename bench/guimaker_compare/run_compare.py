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
        raise_window(title)
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


def raise_window(title: str) -> None:
    """Bring the bench window above the terminal and give it the keyboard."""
    try:
        import ctypes

        user32 = ctypes.windll.user32
        kernel32 = ctypes.windll.kernel32
        hwnd = user32.FindWindowW(None, title)
        if not hwnd:
            return
        # HWND_TOPMOST = -1; SWP_NOSIZE | SWP_NOMOVE.
        user32.SetWindowPos(hwnd, -1, 0, 0, 0, 0, 0x0001 | 0x0002)
        # SetForegroundWindow is restricted to the foreground process; attach
        # this thread to the current foreground thread to satisfy the lock.
        foreground = user32.GetForegroundWindow()
        foreground_tid = user32.GetWindowThreadProcessId(foreground, None)
        current_tid = kernel32.GetCurrentThreadId()
        user32.AttachThreadInput(current_tid, foreground_tid, True)
        user32.SetForegroundWindow(hwnd)
        user32.SetFocus(hwnd)
        user32.AttachThreadInput(current_tid, foreground_tid, False)
    except Exception:
        pass


def park_cursor() -> None:
    try:
        import ctypes
        ctypes.windll.user32.SetCursorPos(0, 0)
    except Exception:
        pass


# Bench strip geometry, in the same logical coordinates every app uses.
BENCH_INPUT = (200.0, 742.0)  # centre of x=40 w=320, y=height-76 h=36
BENCH_BUTTON = (450.0, 742.0)  # centre of x=380 w=140, y=height-76 h=36
WINDOW_LOGICAL_WIDTH = 1280.0

_SEND_INPUT_CACHE: dict = {}


def _send_input() -> dict:
    """Build and cache the Win32 INPUT/SendInput plumbing (Windows only)."""
    if _SEND_INPUT_CACHE:
        return _SEND_INPUT_CACHE
    import ctypes
    from ctypes import wintypes

    class KEYBDINPUT(ctypes.Structure):
        _fields_ = [
            ("wVk", wintypes.WORD),
            ("wScan", wintypes.WORD),
            ("dwFlags", wintypes.DWORD),
            ("time", wintypes.DWORD),
            ("dwExtraInfo", ctypes.c_void_p),
        ]

    class MOUSEINPUT(ctypes.Structure):
        _fields_ = [
            ("dx", wintypes.LONG),
            ("dy", wintypes.LONG),
            ("mouseData", wintypes.DWORD),
            ("dwFlags", wintypes.DWORD),
            ("time", wintypes.DWORD),
            ("dwExtraInfo", ctypes.c_void_p),
        ]

    class HARDWAREINPUT(ctypes.Structure):
        _fields_ = [
            ("uMsg", wintypes.DWORD),
            ("wParamL", wintypes.WORD),
            ("wParamH", wintypes.WORD),
        ]

    class INPUTUNION(ctypes.Union):
        _fields_ = [("ki", KEYBDINPUT), ("mi", MOUSEINPUT), ("hi", HARDWAREINPUT)]

    class INPUT(ctypes.Structure):
        _anonymous_ = ("u",)
        _fields_ = [("type", wintypes.DWORD), ("u", INPUTUNION)]

    user32 = ctypes.windll.user32
    user32.SendInput.argtypes = [wintypes.UINT, ctypes.POINTER(INPUT), ctypes.c_int]
    user32.SendInput.restype = wintypes.UINT
    _SEND_INPUT_CACHE.update({"ctypes": ctypes, "INPUT": INPUT, "user32": user32})
    return _SEND_INPUT_CACHE


def enable_dpi_awareness() -> None:
    try:
        import ctypes
        ctypes.windll.user32.SetProcessDPIAware()
    except Exception:
        pass


def client_geometry(title: str):
    """Client origin, physical/logical scale and logical height.

    ``scale`` is the client width over the 1280 logical width; the apps that
    force 1.5 give 1.5, the GUIMaker app follows the monitor (1.25 here). The
    logical height lets the strip's bottom position be computed instead of
    assuming an 800-high window.
    """
    try:
        import ctypes
        from ctypes import wintypes

        user32 = ctypes.windll.user32
        hwnd = user32.FindWindowW(None, title)
        if not hwnd:
            return None
        rect = wintypes.RECT()
        if not user32.GetClientRect(hwnd, ctypes.byref(rect)):
            return None
        origin = wintypes.POINT(0, 0)
        if not user32.ClientToScreen(hwnd, ctypes.byref(origin)):
            return None
        width = int(rect.right - rect.left)
        height = int(rect.bottom - rect.top)
        if width <= 0 or height <= 0:
            return None
        scale = width / WINDOW_LOGICAL_WIDTH
        if scale <= 0:
            return None
        return (int(origin.x), int(origin.y), scale, height / scale)
    except Exception:
        return None


def wait_for_ready(title: str, out_path: str, timeout: float = 7.0):
    """Wait for the window and the app's `bench_ready` line."""
    deadline = time.monotonic() + timeout
    geometry = None
    ready = False
    while time.monotonic() < deadline:
        if geometry is None:
            geometry = client_geometry(title)
        if not ready:
            try:
                text = Path(out_path).read_text(encoding="utf-8", errors="replace")
            except OSError:
                text = ""
            ready = "bench_ready" in text
        if geometry is not None and ready:
            return geometry
        time.sleep(0.05)
    return geometry


def click_at(x: float, y: float) -> None:
    try:
        import ctypes

        user32 = ctypes.windll.user32
        user32.SetCursorPos(int(round(x)), int(round(y)))
        time.sleep(0.03)
        user32.mouse_event(0x0002, 0, 0, 0, 0)  # MOUSEEVENTF_LEFTDOWN
        time.sleep(0.03)
        user32.mouse_event(0x0004, 0, 0, 0, 0)  # MOUSEEVENTF_LEFTUP
    except Exception:
        pass


def send_unicode(ch: str) -> bool:
    """Type one character into the focused control via SendInput."""
    try:
        api = _send_input()
        ctypes = api["ctypes"]
        INPUT = api["INPUT"]
        user32 = api["user32"]
        inputs = (INPUT * 2)()
        for index, flags in ((0, 0x0004), (1, 0x0004 | 0x0002)):
            inputs[index].type = 1  # INPUT_KEYBOARD
            inputs[index].ki.wVk = 0
            inputs[index].ki.wScan = ord(ch)
            inputs[index].ki.dwFlags = flags  # KEYEVENTF_UNICODE (+ KEYUP)
            inputs[index].ki.time = 0
            inputs[index].ki.dwExtraInfo = 0
        sent = user32.SendInput(2, inputs, ctypes.sizeof(INPUT))
        return int(sent) == 2
    except Exception:
        return False


def typing_worker(title: str, out_path: str, duration: float, chars_per_sec: float, holder: list) -> None:
    """Click the bench input and type `duration` seconds at a steady rate."""
    try:
        import ctypes

        user32 = ctypes.windll.user32
        geometry = wait_for_ready(title, out_path)
        if geometry is None:
            return
        origin_x, origin_y, scale, logical_height = geometry
        raise_window(title)
        strip_y = logical_height - 58.0
        click_at(origin_x + scale * BENCH_INPUT[0], origin_y + scale * strip_y)
        time.sleep(0.2)

        alphabet = "abcdefghij"
        interval = 1.0 / max(1.0, chars_per_sec)
        deadline = time.monotonic() + duration
        typed = 0
        index = 0
        while time.monotonic() < deadline:
            if send_unicode(alphabet[index % len(alphabet)]):
                typed += 1
            holder[0] = typed
            index += 1
            time.sleep(interval)
    except Exception:
        return


def button_hover_worker(title: str, out_path: str, duration: float) -> None:
    """Park the pointer on the bench button and keep it there."""
    try:
        import ctypes

        user32 = ctypes.windll.user32
        geometry = wait_for_ready(title, out_path)
        if geometry is None:
            return
        raise_window(title)
        origin_x, origin_y, scale, logical_height = geometry
        x = int(round(origin_x + scale * BENCH_BUTTON[0]))
        y = int(round(origin_y + scale * (logical_height - 58.0)))
        deadline = time.monotonic() + duration
        while time.monotonic() < deadline:
            user32.SetCursorPos(x, y)
            time.sleep(0.25)
    except Exception:
        return


def run_once(
    lib: str, seconds: float, hover: bool, typing: bool, button: bool
) -> dict:
    global TARGET_DIR
    exe = TARGET_DIR / "release" / f"guimaker-compare-{lib}.exe"
    if not exe.is_file():
        return {"library": lib, "error": f"missing {exe}"}
    env = dict(**__import__("os").environ)
    env.setdefault("FERROUS_TRACE_STARTUP", "1")
    env.setdefault("FERROUS_TRACE_STARTUP_JSON", "1")
    if not (hover or typing or button):
        park_cursor()
    if typing:
        mode = "typing"
    elif button:
        mode = "button-hover"
    elif hover:
        mode = "hover"
    else:
        mode = "idle"
    # Typing needs 5 s of input after the first frame; give it room to finish.
    effective_seconds = max(seconds, 9.0) if typing else seconds
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
        [str(exe), "--seconds", str(effective_seconds)],
        stdout=out_file,
        stderr=err_file,
        text=True,
        env=env,
    )
    ps = psutil.Process(proc.pid)
    ps.cpu_percent(None)
    samples: list[float] = []
    rss: list[int] = []
    moves_holder = [0]
    typed_holder = [0]
    worker = None
    if typing:
        worker = threading.Thread(
            target=typing_worker,
            args=(TITLE[lib], out_file.name, 5.0, 10.0, typed_holder),
            daemon=True,
        )
        worker.start()
    elif button:
        worker = threading.Thread(
            target=button_hover_worker,
            args=(TITLE[lib], out_file.name, effective_seconds),
            daemon=True,
        )
        worker.start()
    elif hover:
        worker = threading.Thread(
            target=lambda: moves_holder.__setitem__(
                0, hover_sweep(TITLE[lib], effective_seconds)
            ),
            daemon=True,
        )
        worker.start()
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
        if not snap_end and now >= started + max(1.0, effective_seconds - 1.0):
            snap_end = thread_snapshot(ps)
            ui_tid = window_thread_id(TITLE[lib])
        time.sleep(0.05)
    if worker is not None:
        worker.join(timeout=2.0)
    try:
        proc.wait(timeout=effective_seconds + 20)
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
    bench_text_chars = None
    for line in out.splitlines():
        line = line.strip()
        if line.startswith("frames "):
            frames = int(line.split(" ", 1)[1])
        elif line.startswith("bench_text_chars "):
            try:
                bench_text_chars = int(line.split(" ", 1)[1])
            except ValueError:
                pass
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
        "mode": mode,
        "startup_ms": startup,
        "startup_stage": stage,
        "idle_cpu_pct": round(cpu, 3),
        "tail_cpu_pct": round(tail_cpu, 3),
        "typing_cpu_pct": round(cpu, 3) if typing else None,
        "button_cpu_pct": round(cpu, 3) if button else None,
        "typing_chars": typed_holder[0] if typing else 0,
        "bench_text_chars": bench_text_chars,
        "top_threads_cpu": threads,
        "ui_thread_id": ui_tid,
        "ui_thread_cpu_seconds": ui_thread_cpu,
        "idle_frames": frames,
        "rss_bytes": max(rss) if rss else 0,
        "wall_seconds": round(time.monotonic() - started, 3),
        "hover_moves": moves_holder[0],
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
    parser.add_argument(
        "--typing",
        action="store_true",
        help="click the bench input and type 10 chars/s for 5 s",
    )
    parser.add_argument(
        "--button-hover",
        action="store_true",
        help="park the pointer on the bench button",
    )
    parser.add_argument("--target-dir", type=Path, default=ROOT / "target")
    args = parser.parse_args()
    global TARGET_DIR
    TARGET_DIR = args.target_dir

    enable_dpi_awareness()
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
            row = run_once(lib, args.seconds, args.hover, args.typing, args.button_hover)
            row["run"] = run
            rows.append(row)
            path = args.out / f"{lib}-run{run}.json"
            with path.open("w", encoding="utf-8", newline="\n") as handle:
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
