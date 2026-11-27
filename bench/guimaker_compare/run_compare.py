#!/usr/bin/env python3
"""Phase 9D apples-to-apples harness for the GUIMaker comparison bench.

This is deliberately *external*: presents and startup come from PresentMon or
from a Windows ETW (wpr) capture, never from the apps' own counters. The apps
still print `frames` / `startup_ms` but those are a cross-check only. The
protocol that the code enforces is in PROTOCOL.md.

Windows-only, Python 3 standard library + ctypes. No pip installs.
"""

from __future__ import annotations

import argparse
import collections
import csv
import ctypes
import datetime as _dt
import json
import math
import os
import platform
import shlex
import shutil
import socket
import statistics
import subprocess
import sys
import tempfile
import threading
import time
import winreg
from ctypes import wintypes
from pathlib import Path

if os.name != "nt":
    print("run_compare.py is Windows-only (PrintWindow/ETW/PresentMon).", file=sys.stderr)
    raise SystemExit(2)

ROOT = Path(__file__).resolve().parent
REPO_ROOT = ROOT.parent.parent
WINDOW_LOGICAL_WIDTH = 1280.0
WINDOW_LOGICAL_HEIGHT = 800.0
TARGET_DIR = ROOT / "target"

SESSION_ID = _dt.datetime.now(_dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ")

LIB_ORDER = ["guimaker", "egui", "egui-wgpu", "iced", "slint", "imgui"]
FLOOR_LIBS = ["floor-wgpu-vulkan", "floor-wgpu-dx12", "floor-wgpu-gl", "floor-glow"]
ALL_LIBS = LIB_ORDER + FLOOR_LIBS

# One table per tier; the raw-API floor app is part of every tier.
LIB_TIERS = {
    "vulkan": ["guimaker", "egui-wgpu", "iced", "imgui", "floor-wgpu-vulkan"],
    "dx12": ["guimaker", "egui-wgpu", "iced", "imgui", "floor-wgpu-dx12"],
    "gl": ["guimaker", "egui", "slint", "floor-glow"],
}
TIER_ENV = {"vulkan": "vulkan", "dx12": "dx12", "gl": "gl"}

SCENARIOS = {
    # name: seconds, interaction, floors allowed, has a tail window
    "idle6": (6.0, None, True, False),
    "idle30": (30.0, None, True, True),
    "hover_body": (6.0, "hover_body", False, False),
    "hover_button": (6.0, "hover_button", False, False),
    "typing": (10.0, "typing", False, False),
    "resize": (8.0, "resize", False, False),
    "menu": (8.0, "menu", False, False),
}

# Bench strip geometry in *logical* pixels (the same in every app).
BENCH_INPUT = (200.0, -58.0)  # x absolute, y relative to client height
BENCH_BUTTON = (450.0, -58.0)
BENCH_MENU = (610.0, -58.0)

VERSIONS = {
    "guimaker": {
        "ferrous_engine": "0.1.0 (workspace)",
        "wgpu": "23.0.1",
        "winit": "0.30.13",
    },
    "egui": {"eframe": "0.29.1", "egui": "0.29.1", "glow": "0.13.1", "winit": "0.30.13"},
    "egui-wgpu": {
        "eframe": "0.29.1",
        "egui-wgpu": "0.29.1",
        "wgpu": "22.1.0",
        "winit": "0.30.13",
    },
    "iced": {"iced": "0.13.1", "iced_wgpu": "0.13.5", "wgpu": "22.1.0", "winit": "0.30.13"},
    "slint": {
        "slint": "1.7.2",
        "i-slint-renderer-femtovg": "1.7.2",
        "winit": "0.30.13",
    },
    "imgui": {
        "imgui": "0.12.0",
        "imgui-wgpu": "0.28.0",
        "imgui-winit-support": "0.13.0",
        "wgpu": "29.0.4",
        "winit": "0.30.13",
    },
    "floor-wgpu-vulkan": {"wgpu": "29.0.4", "winit": "0.30.13"},
    "floor-wgpu-dx12": {"wgpu": "29.0.4", "winit": "0.30.13"},
    "floor-wgpu-gl": {"wgpu": "29.0.4", "winit": "0.30.13"},
    "floor-glow": {"glow": "0.13.1", "glutin": "0.32.3", "glutin-winit": "0.5.0", "winit": "0.30.13"},
}

DRIVER_HINTS = (
    "nvwgf2umx",
    "nvwgf2um",
    "nvoglv64",
    "nvoglv32",
    "nvldumd",
    "igd",
    "ig9",
    "amdvlk",
    "atio6axx",
    "atidxx",
    "aticfx",
    "vulkan-1",
    "d3d12",
    "d3d11",
    "dxgi",
    "opengl32",
    "libegl",
    "libglesv2",
    "glu32",
)

SYSTEM_HINTS = ("ntdll", "kernelbase", "win32u", "ucrtbase", "msvcrt", "kernel32")

# ── Win32 plumbing ────────────────────────────────────────────────────────────

kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
user32 = ctypes.WinDLL("user32", use_last_error=True)
gdi32 = ctypes.WinDLL("gdi32", use_last_error=True)
psapi = ctypes.WinDLL("psapi", use_last_error=True)
pdh = ctypes.WinDLL("pdh", use_last_error=True)
shell32 = ctypes.WinDLL("shell32", use_last_error=True)
shcore = ctypes.WinDLL("shcore", use_last_error=True)
ntdll = ctypes.WinDLL("ntdll", use_last_error=True)


class FILETIME(ctypes.Structure):
    _fields_ = [("dwLowDateTime", wintypes.DWORD), ("dwHighDateTime", wintypes.DWORD)]


def _ft(value: FILETIME) -> int:
    return (int(value.dwHighDateTime) << 32) | int(value.dwLowDateTime)


class PROCESS_MEMORY_COUNTERS_EX(ctypes.Structure):
    _fields_ = [
        ("cb", wintypes.DWORD),
        ("PageFaultCount", wintypes.DWORD),
        ("PeakWorkingSetSize", ctypes.c_size_t),
        ("WorkingSetSize", ctypes.c_size_t),
        ("QuotaPeakPagedPoolUsage", ctypes.c_size_t),
        ("QuotaPagedPoolUsage", ctypes.c_size_t),
        ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t),
        ("QuotaNonPagedPoolUsage", ctypes.c_size_t),
        ("PagefileUsage", ctypes.c_size_t),
        ("PeakPagefileUsage", ctypes.c_size_t),
        ("PrivateUsage", ctypes.c_size_t),
    ]


class THREADENTRY32(ctypes.Structure):
    _fields_ = [
        ("dwSize", wintypes.DWORD),
        ("cntUsage", wintypes.DWORD),
        ("th32ThreadID", wintypes.DWORD),
        ("th32OwnerProcessID", wintypes.DWORD),
        ("tpBasePri", wintypes.LONG),
        ("tpDeltaPri", wintypes.LONG),
        ("dwFlags", wintypes.DWORD),
    ]


class MODULEINFO(ctypes.Structure):
    _fields_ = [
        ("lpBaseOfDll", ctypes.c_void_p),
        ("SizeOfImage", wintypes.DWORD),
        ("EntryPoint", ctypes.c_void_p),
    ]


class POINT(ctypes.Structure):
    _fields_ = [("x", wintypes.LONG), ("y", wintypes.LONG)]


class RECT(ctypes.Structure):
    _fields_ = [
        ("left", wintypes.LONG),
        ("top", wintypes.LONG),
        ("right", wintypes.LONG),
        ("bottom", wintypes.LONG),
    ]


class BITMAPINFOHEADER(ctypes.Structure):
    _fields_ = [
        ("biSize", wintypes.DWORD),
        ("biWidth", wintypes.LONG),
        ("biHeight", wintypes.LONG),
        ("biPlanes", wintypes.WORD),
        ("biBitCount", wintypes.WORD),
        ("biCompression", wintypes.DWORD),
        ("biSizeImage", wintypes.DWORD),
        ("biXPelsPerMeter", wintypes.LONG),
        ("biYPelsPerMeter", wintypes.LONG),
        ("biClrUsed", wintypes.DWORD),
        ("biClrImportant", wintypes.DWORD),
    ]


class BITMAPINFO(ctypes.Structure):
    _fields_ = [("bmiHeader", BITMAPINFOHEADER), ("bmiColors", wintypes.DWORD * 3)]


kernel32.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
kernel32.OpenProcess.restype = wintypes.HANDLE
kernel32.OpenThread.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
kernel32.OpenThread.restype = wintypes.HANDLE
kernel32.GetProcessTimes.argtypes = [
    wintypes.HANDLE,
    ctypes.POINTER(FILETIME),
    ctypes.POINTER(FILETIME),
    ctypes.POINTER(FILETIME),
    ctypes.POINTER(FILETIME),
]
kernel32.GetThreadTimes.argtypes = [
    wintypes.HANDLE,
    ctypes.POINTER(FILETIME),
    ctypes.POINTER(FILETIME),
    ctypes.POINTER(FILETIME),
    ctypes.POINTER(FILETIME),
]
kernel32.CreateToolhelp32Snapshot.argtypes = [wintypes.DWORD, wintypes.DWORD]
kernel32.CreateToolhelp32Snapshot.restype = wintypes.HANDLE
kernel32.Thread32First.argtypes = [wintypes.HANDLE, ctypes.POINTER(THREADENTRY32)]
kernel32.Thread32Next.argtypes = [wintypes.HANDLE, ctypes.POINTER(THREADENTRY32)]
kernel32.GetThreadDescription.argtypes = [
    wintypes.HANDLE,
    ctypes.POINTER(ctypes.c_wchar_p),
]
kernel32.GetThreadDescription.restype = ctypes.c_long
kernel32.LocalFree.argtypes = [ctypes.c_void_p]
kernel32.QueryPerformanceCounter.argtypes = [ctypes.POINTER(ctypes.c_int64)]
kernel32.QueryPerformanceFrequency.argtypes = [ctypes.POINTER(ctypes.c_int64)]
kernel32.GetCurrentThreadId.restype = wintypes.DWORD

user32.FindWindowW.argtypes = [wintypes.LPCWSTR, wintypes.LPCWSTR]
user32.FindWindowW.restype = wintypes.HWND
user32.IsWindowVisible.argtypes = [wintypes.HWND]
user32.IsWindowVisible.restype = wintypes.BOOL
user32.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
user32.GetWindowThreadProcessId.restype = wintypes.DWORD
user32.GetWindowRect.argtypes = [wintypes.HWND, ctypes.POINTER(RECT)]
user32.GetClientRect.argtypes = [wintypes.HWND, ctypes.POINTER(RECT)]
user32.ClientToScreen.argtypes = [wintypes.HWND, ctypes.POINTER(POINT)]
user32.SetWindowPos.argtypes = [
    wintypes.HWND,
    wintypes.HWND,
    ctypes.c_int,
    ctypes.c_int,
    ctypes.c_int,
    ctypes.c_int,
    wintypes.UINT,
]
user32.SetForegroundWindow.argtypes = [wintypes.HWND]
user32.SetFocus.argtypes = [wintypes.HWND]
user32.AttachThreadInput.argtypes = [wintypes.DWORD, wintypes.DWORD, wintypes.BOOL]
user32.GetForegroundWindow.restype = wintypes.HWND
user32.SetCursorPos.argtypes = [ctypes.c_int, ctypes.c_int]
user32.PrintWindow.argtypes = [wintypes.HWND, wintypes.HDC, wintypes.UINT]
user32.GetDC.argtypes = [wintypes.HWND]
user32.GetDC.restype = wintypes.HDC
user32.ReleaseDC.argtypes = [wintypes.HWND, wintypes.HDC]
user32.SetProcessDpiAwarenessContext.argtypes = [ctypes.c_void_p]
user32.SetProcessDPIAware.restype = wintypes.BOOL
user32.MonitorFromPoint.argtypes = [POINT, wintypes.DWORD]
user32.MonitorFromPoint.restype = wintypes.HANDLE

gdi32.CreateCompatibleDC.argtypes = [wintypes.HDC]
gdi32.CreateCompatibleDC.restype = wintypes.HDC
gdi32.CreateCompatibleBitmap.argtypes = [wintypes.HDC, ctypes.c_int, ctypes.c_int]
gdi32.CreateCompatibleBitmap.restype = wintypes.HBITMAP
gdi32.SelectObject.argtypes = [wintypes.HDC, wintypes.HGDIOBJ]
gdi32.SelectObject.restype = wintypes.HGDIOBJ
gdi32.DeleteObject.argtypes = [wintypes.HGDIOBJ]
gdi32.DeleteDC.argtypes = [wintypes.HDC]
gdi32.GetDIBits.argtypes = [
    wintypes.HDC,
    wintypes.HBITMAP,
    wintypes.UINT,
    wintypes.UINT,
    ctypes.c_void_p,
    ctypes.POINTER(BITMAPINFO),
    wintypes.UINT,
]

psapi.GetProcessMemoryInfo.argtypes = [
    wintypes.HANDLE,
    ctypes.POINTER(PROCESS_MEMORY_COUNTERS_EX),
    wintypes.DWORD,
]
psapi.EnumProcessModulesEx.argtypes = [
    wintypes.HANDLE,
    ctypes.POINTER(wintypes.HMODULE),
    wintypes.DWORD,
    ctypes.POINTER(wintypes.DWORD),
    wintypes.DWORD,
]
psapi.GetModuleInformation.argtypes = [
    wintypes.HANDLE,
    wintypes.HMODULE,
    ctypes.POINTER(MODULEINFO),
    wintypes.DWORD,
]
psapi.GetModuleBaseNameW.argtypes = [
    wintypes.HANDLE,
    wintypes.HMODULE,
    wintypes.LPWSTR,
    wintypes.DWORD,
]
psapi.GetModuleBaseNameW.restype = wintypes.DWORD

ntdll.NtQueryInformationThread.argtypes = [
    wintypes.HANDLE,
    ctypes.c_int,
    ctypes.c_void_p,
    wintypes.ULONG,
    ctypes.POINTER(wintypes.ULONG),
]

shcore.GetDpiForMonitor.argtypes = [
    wintypes.HANDLE,
    ctypes.c_int,
    ctypes.POINTER(wintypes.UINT),
    ctypes.POINTER(wintypes.UINT),
]

PROCESS_QUERY_INFORMATION = 0x0400
PROCESS_QUERY_LIMITED_INFORMATION = 0x1000
PROCESS_VM_READ = 0x0010
THREAD_QUERY_INFORMATION = 0x0040
THREAD_QUERY_LIMITED_INFORMATION = 0x0800
TH32CS_SNAPTHREAD = 0x00000004
INVALID_HANDLE_VALUE = ctypes.c_void_p(-1).value


def last_error() -> str:
    return ctypes.FormatError(ctypes.get_last_error()).strip()


def open_process(pid: int):
    handle = kernel32.OpenProcess(
        PROCESS_QUERY_INFORMATION | PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ,
        False,
        pid,
    )
    return handle or None


def process_cpu_seconds(handle) -> float:
    creation, exit_time, kernel, user = FILETIME(), FILETIME(), FILETIME(), FILETIME()
    if not kernel32.GetProcessTimes(
        handle, ctypes.byref(creation), ctypes.byref(exit_time), ctypes.byref(kernel), ctypes.byref(user)
    ):
        return 0.0
    return (_ft(kernel) + _ft(user)) / 10_000_000.0


def process_rss_bytes(handle) -> int:
    counters = PROCESS_MEMORY_COUNTERS_EX()
    counters.cb = ctypes.sizeof(counters)
    if not psapi.GetProcessMemoryInfo(
        handle, ctypes.byref(counters), ctypes.sizeof(counters)
    ):
        return 0
    return int(counters.WorkingSetSize)


def thread_cpu_seconds(tid: int) -> float:
    handle = kernel32.OpenThread(THREAD_QUERY_INFORMATION, False, tid)
    if not handle:
        return 0.0
    creation, exit_time, kernel, user = FILETIME(), FILETIME(), FILETIME(), FILETIME()
    ok = kernel32.GetThreadTimes(
        handle, ctypes.byref(creation), ctypes.byref(exit_time), ctypes.byref(kernel), ctypes.byref(user)
    )
    kernel32.CloseHandle(handle)
    if not ok:
        return 0.0
    return (_ft(kernel) + _ft(user)) / 10_000_000.0


def enumerate_threads(pid: int) -> list[int]:
    snapshot = kernel32.CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0)
    if not snapshot or snapshot == INVALID_HANDLE_VALUE:
        return []
    entry = THREADENTRY32()
    entry.dwSize = ctypes.sizeof(entry)
    tids: list[int] = []
    try:
        ok = kernel32.Thread32First(snapshot, ctypes.byref(entry))
        while ok:
            if entry.th32OwnerProcessID == pid:
                tids.append(int(entry.th32ThreadID))
            ok = kernel32.Thread32Next(snapshot, ctypes.byref(entry))
    finally:
        kernel32.CloseHandle(snapshot)
    return tids


def thread_snapshot(pid: int) -> dict[int, float]:
    return {tid: thread_cpu_seconds(tid) for tid in enumerate_threads(pid)}


def thread_name(tid: int) -> str:
    handle = kernel32.OpenThread(THREAD_QUERY_LIMITED_INFORMATION, False, tid)
    if not handle:
        return ""
    try:
        value = ctypes.c_wchar_p()
        if kernel32.GetThreadDescription(handle, ctypes.byref(value)) >= 0 and value.value:
            return value.value
    except (OSError, AttributeError):
        return ""
    finally:
        kernel32.CloseHandle(handle)
    return ""


def module_map(pid: int) -> list[tuple[int, int, str]]:
    """(base, end, base name) for every module in the process."""
    handle = open_process(pid)
    if not handle:
        return []
    modules: list[tuple[int, int, str]] = []
    try:
        needed = wintypes.DWORD()
        if not psapi.EnumProcessModulesEx(handle, None, 0, ctypes.byref(needed), 3):
            return []
        count = max(1, needed.value // ctypes.sizeof(wintypes.HMODULE))
        array = (wintypes.HMODULE * count)()
        if not psapi.EnumProcessModulesEx(
            handle, array, ctypes.sizeof(array), ctypes.byref(needed), 3
        ):
            return []
        seen = needed.value // ctypes.sizeof(wintypes.HMODULE)
        for index in range(min(seen, count)):
            info = MODULEINFO()
            if not psapi.GetModuleInformation(
                handle, array[index], ctypes.byref(info), ctypes.sizeof(info)
            ):
                continue
            buffer = ctypes.create_unicode_buffer(260)
            length = psapi.GetModuleBaseNameW(handle, array[index], buffer, 260)
            name = buffer.value if length else ""
            base = int(info.lpBaseOfDll or 0)
            modules.append((base, base + int(info.SizeOfImage), name))
    finally:
        kernel32.CloseHandle(handle)
    return modules


def thread_start_address(tid: int):
    # `NtQueryInformationThread(ThreadQuerySetWin32StartAddress)` needs
    # THREAD_QUERY_INFORMATION; QUERY_LIMITED is not enough.
    handle = kernel32.OpenThread(THREAD_QUERY_INFORMATION, False, tid)
    if not handle:
        return None
    try:
        address = ctypes.c_void_p()
        status = ntdll.NtQueryInformationThread(
            handle, 9, ctypes.byref(address), ctypes.sizeof(address), None
        )
        if status == 0:
            return int(address.value or 0)
    finally:
        kernel32.CloseHandle(handle)
    return None


def classify_module(name: str) -> str:
    lower = name.lower()
    if any(hint in lower for hint in DRIVER_HINTS):
        return "driver"
    if any(hint in lower for hint in SYSTEM_HINTS):
        return "system"
    return ""


def describe_thread(tid: int, modules: list[tuple[int, int, str]]) -> dict:
    name = thread_name(tid)
    module = ""
    kind = ""
    address = thread_start_address(tid)
    if address:
        for base, end, candidate in modules:
            if base <= address < end:
                module = candidate
                kind = classify_module(candidate)
                break
    if name:
        display = f"tid {tid} {name}"
        if module and module.lower() not in name.lower():
            display += f" ({module})"
    elif module:
        display = f"tid {tid} {module}"
        if kind:
            display += f" ({kind})"
    else:
        display = f"tid {tid} unknown"
    return {"tid": tid, "name": name, "module": module, "kind": kind, "display": display}


def qpc_frequency() -> float:
    value = ctypes.c_int64()
    if not kernel32.QueryPerformanceFrequency(ctypes.byref(value)) or value.value <= 0:
        return 10_000_000.0
    return float(value.value)


def qpc_seconds() -> float:
    value = ctypes.c_int64()
    if not kernel32.QueryPerformanceCounter(ctypes.byref(value)):
        return 0.0
    return value.value / qpc_frequency()


def enable_dpi_awareness() -> None:
    try:
        if not user32.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4)):
            user32.SetProcessDPIAware()
    except (OSError, AttributeError):
        try:
            user32.SetProcessDPIAware()
        except (OSError, AttributeError):
            pass


def monitor_scale() -> float:
    try:
        monitor = user32.MonitorFromPoint(POINT(0, 0), 2)  # nearest
        x = wintypes.UINT()
        y = wintypes.UINT()
        if monitor and shcore.GetDpiForMonitor(monitor, 0, ctypes.byref(x), ctypes.byref(y)) == 0:
            return max(1.0, x.value / 96.0)
    except (OSError, AttributeError):
        pass
    return 1.0


def find_window(pid: int, title: str, timeout: float = 10.0):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        hwnd = user32.FindWindowW(None, title)
        if hwnd:
            owner = wintypes.DWORD()
            user32.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
            if owner.value == pid:
                return hwnd
        time.sleep(0.02)
    return None


def wait_ready(pid: int, title: str, timeout: float = 15.0):
    """Wait for the real window to be visible (no file polling).

    The engine creates its window hidden and shows it on the first present;
    winit-based apps show at creation. `IsWindowVisible` covers both without
    reading the app's output files.
    """
    deadline = time.monotonic() + timeout
    hwnd = None
    while time.monotonic() < deadline:
        if hwnd is None:
            hwnd = find_window(pid, title, timeout=0.2)
        if hwnd and user32.IsWindowVisible(hwnd):
            return hwnd
        time.sleep(0.02)
    return hwnd


def window_thread_id(hwnd) -> int | None:
    if not hwnd:
        return None
    pid = wintypes.DWORD()
    tid = user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
    return int(tid) if tid else None


def client_geometry(hwnd):
    rect = RECT()
    if not user32.GetClientRect(hwnd, ctypes.byref(rect)):
        return None
    origin = POINT(0, 0)
    if not user32.ClientToScreen(hwnd, ctypes.byref(origin)):
        return None
    width = int(rect.right - rect.left)
    height = int(rect.bottom - rect.top)
    if width <= 0 or height <= 0:
        return None
    scale = width / WINDOW_LOGICAL_WIDTH
    if scale <= 0:
        return None
    return {
        "x": int(origin.x),
        "y": int(origin.y),
        "scale": scale,
        "width": width,
        "height": height,
        "logical_height": height / scale,
    }


def raise_window(hwnd) -> None:
    if not hwnd:
        return
    try:
        user32.SetWindowPos(hwnd, -1, 0, 0, 0, 0, 0x0001 | 0x0002)
        foreground = user32.GetForegroundWindow()
        foreground_tid = user32.GetWindowThreadProcessId(foreground, None)
        current_tid = kernel32.GetCurrentThreadId()
        user32.AttachThreadInput(current_tid, foreground_tid, True)
        user32.SetForegroundWindow(hwnd)
        user32.SetFocus(hwnd)
        user32.AttachThreadInput(current_tid, foreground_tid, False)
    except OSError:
        pass


def click_at(x: float, y: float) -> None:
    user32.SetCursorPos(int(round(x)), int(round(y)))
    time.sleep(0.03)
    user32.mouse_event(0x0002, 0, 0, 0, 0)
    time.sleep(0.03)
    user32.mouse_event(0x0004, 0, 0, 0, 0)


_SEND_INPUT: dict = {}


def _send_input_api() -> dict:
    if _SEND_INPUT:
        return _SEND_INPUT

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

    user32.SendInput.argtypes = [wintypes.UINT, ctypes.POINTER(INPUT), ctypes.c_int]
    user32.SendInput.restype = wintypes.UINT
    _SEND_INPUT.update({"INPUT": INPUT})
    return _SEND_INPUT


def send_unicode(ch: str) -> bool:
    try:
        INPUT = _send_input_api()["INPUT"]
        inputs = (INPUT * 2)()
        for index, flags in ((0, 0x0004), (1, 0x0004 | 0x0002)):
            inputs[index].type = 1
            inputs[index].ki.wVk = 0
            inputs[index].ki.wScan = ord(ch)
            inputs[index].ki.dwFlags = flags
            inputs[index].ki.time = 0
            inputs[index].ki.dwExtraInfo = 0
        sent = user32.SendInput(2, inputs, ctypes.sizeof(INPUT))
        return int(sent) == 2
    except (OSError, ValueError):
        return False


# ── PrintWindow capture and pixel diff ───────────────────────────────────────


def capture_window(hwnd) -> dict | None:
    if not hwnd:
        return None
    rect = RECT()
    if not user32.GetWindowRect(hwnd, ctypes.byref(rect)):
        return None
    width = int(rect.right - rect.left)
    height = int(rect.bottom - rect.top)
    if width <= 0 or height <= 0:
        return None
    window_dc = user32.GetDC(hwnd)
    if not window_dc:
        return None
    mem_dc = gdi32.CreateCompatibleDC(window_dc)
    bitmap = gdi32.CreateCompatibleBitmap(window_dc, width, height)
    previous = gdi32.SelectObject(mem_dc, bitmap)
    try:
        printed = bool(user32.PrintWindow(hwnd, mem_dc, 2))  # PW_RENDERFULLCONTENT
        info = BITMAPINFO()
        info.bmiHeader.biSize = ctypes.sizeof(BITMAPINFOHEADER)
        info.bmiHeader.biWidth = width
        info.bmiHeader.biHeight = -height
        info.bmiHeader.biPlanes = 1
        info.bmiHeader.biBitCount = 32
        info.bmiHeader.biCompression = 0
        buffer = ctypes.create_string_buffer(width * height * 4)
        got = gdi32.GetDIBits(
            mem_dc, bitmap, 0, height, buffer, ctypes.byref(info), 0
        )
        if not printed or not got:
            return None
        data = buffer.raw
        # A failed PrintWindow on DirectComposition returns an all-black image.
        pixels = width * height
        step = max(1, pixels // 2000)
        sampled = 0
        nonblack = 0
        for pixel in range(0, pixels, step):
            offset = pixel * 4
            if data[offset] + data[offset + 1] + data[offset + 2] > 8:
                nonblack += 1
            sampled += 1
        return {
            "width": width,
            "height": height,
            "bgra": data,
            "nonblack_ratio": nonblack / sampled if sampled else 0.0,
        }
    finally:
        gdi32.SelectObject(mem_dc, previous)
        gdi32.DeleteObject(bitmap)
        gdi32.DeleteDC(mem_dc)
        user32.ReleaseDC(hwnd, window_dc)


def save_bmp(path: Path, capture: dict) -> None:
    width, height, data = capture["width"], capture["height"], capture["bgra"]
    header = BITMAPINFOHEADER()
    header.biSize = ctypes.sizeof(BITMAPINFOHEADER)
    header.biWidth = width
    header.biHeight = -height
    header.biPlanes = 1
    header.biBitCount = 32
    header.biCompression = 0
    header.biSizeImage = len(data)
    file_header = (
        b"BM"
        + (14 + ctypes.sizeof(header) + len(data)).to_bytes(4, "little")
        + b"\x00\x00\x00\x00"
        + (14 + ctypes.sizeof(header)).to_bytes(4, "little")
    )
    header_bytes = ctypes.string_at(ctypes.byref(header), ctypes.sizeof(header))
    path.write_bytes(file_header + header_bytes + data)


def diff_captures(reference: dict | None, capture: dict | None) -> dict:
    if not reference or not capture:
        return {"comparable": False, "reason": "missing capture"}
    if reference["width"] != capture["width"] or reference["height"] != capture["height"]:
        width = min(reference["width"], capture["width"])
        height = min(reference["height"], capture["height"])
        note = "size-mismatch (intersection only)"
    else:
        width, height = reference["width"], reference["height"]
        note = ""
    if width <= 0 or height <= 0:
        return {"comparable": False, "reason": "empty overlap"}
    a = reference["bgra"]
    b = capture["bgra"]
    total_pixels = width * height
    step = max(1, total_pixels // 200_000)
    sampled = 0
    total_abs = 0
    changed = 0
    for pixel in range(0, total_pixels, step):
        offset = pixel * 4
        if offset + 3 >= len(a) or offset + 3 >= len(b):
            break
        delta = (
            abs(a[offset] - b[offset])
            + abs(a[offset + 1] - b[offset + 1])
            + abs(a[offset + 2] - b[offset + 2])
        )
        total_abs += delta
        if delta > 24:
            changed += 1
        sampled += 1
    if not sampled:
        return {"comparable": False, "reason": "empty overlap"}
    return {
        "comparable": not bool(note),
        "note": note,
        "width": width,
        "height": height,
        "sampled_pixels": sampled,
        "mean_abs_channel_diff": round(total_abs / (sampled * 3), 3),
        "changed_pixel_ratio": round(changed / sampled, 4),
    }


# ── GPU memory (one source for every app) ────────────────────────────────────


class PDH_UNION(ctypes.Union):
    _fields_ = [
        ("longValue", wintypes.LONG),
        ("doubleValue", ctypes.c_double),
        ("largeValue", ctypes.c_longlong),
        ("AnsiStringValue", ctypes.c_char_p),
        ("WideStringValue", ctypes.c_wchar_p),
    ]


class PDH_FMT_COUNTERVALUE(ctypes.Structure):
    _fields_ = [("CStatus", wintypes.DWORD), ("u", PDH_UNION)]


class PDH_FMT_COUNTERVALUE_ITEM_W(ctypes.Structure):
    _fields_ = [("szName", ctypes.c_wchar_p), ("FmtValue", PDH_FMT_COUNTERVALUE)]


pdh.PdhOpenQueryW.argtypes = [wintypes.LPCWSTR, ctypes.c_size_t, ctypes.POINTER(wintypes.HANDLE)]
pdh.PdhAddEnglishCounterW.argtypes = [
    wintypes.HANDLE,
    wintypes.LPCWSTR,
    ctypes.c_size_t,
    ctypes.POINTER(wintypes.HANDLE),
]
pdh.PdhCollectQueryData.argtypes = [wintypes.HANDLE]
pdh.PdhGetFormattedCounterArrayW.argtypes = [
    wintypes.HANDLE,
    wintypes.DWORD,
    ctypes.POINTER(wintypes.DWORD),
    ctypes.POINTER(wintypes.DWORD),
    ctypes.POINTER(PDH_FMT_COUNTERVALUE_ITEM_W),
]
pdh.PdhCloseQuery.argtypes = [wintypes.HANDLE]

PDH_FMT_DOUBLE = 0x00000200


def gpu_memory_bytes(pid: int) -> int:
    query = wintypes.HANDLE()
    if pdh.PdhOpenQueryW(None, 0, ctypes.byref(query)) != 0:
        return 0
    totals: dict[str, int] = {}
    try:
        for metric in ("Dedicated Usage", "Local Usage"):
            path = f"\\GPU Process Memory(pid_{pid}_*)\\{metric}"
            counter = wintypes.HANDLE()
            if pdh.PdhAddEnglishCounterW(query, path, 0, ctypes.byref(counter)) != 0:
                continue
            if pdh.PdhCollectQueryData(query) != 0:
                continue
            size = wintypes.DWORD(0)
            count = wintypes.DWORD(0)
            pdh.PdhGetFormattedCounterArrayW(
                counter, PDH_FMT_DOUBLE, ctypes.byref(size), ctypes.byref(count), None
            )
            if size.value <= 0 or count.value <= 0:
                continue
            buffer = ctypes.create_string_buffer(size.value)
            items = ctypes.cast(buffer, ctypes.POINTER(PDH_FMT_COUNTERVALUE_ITEM_W))
            if (
                pdh.PdhGetFormattedCounterArrayW(
                    counter, PDH_FMT_DOUBLE, ctypes.byref(size), ctypes.byref(count), items
                )
                != 0
            ):
                continue
            total = 0
            for index in range(count.value):
                total += int(items[index].FmtValue.u.doubleValue)
            totals[metric] = total
    finally:
        pdh.PdhCloseQuery(query)
    # Dedicated is the comparable number; Local includes shared/system memory.
    return totals.get("Dedicated Usage") or totals.get("Local Usage") or 0


# ── External present sources: PresentMon, else wpr/ETW ──────────────────────

WPRP = """<?xml version="1.0" encoding="utf-8"?>
<WindowsPerformanceRecorder Version="1.0">
  <Profiles>
    <SystemCollector Id="SystemCollector" Name="WPR System Collector">
      <BufferSize Value="1024"/>
      <Buffers Value="128"/>
    </SystemCollector>
    <EventCollector Id="EventCollector" Name="WPR Event Collector">
      <BufferSize Value="1024"/>
      <Buffers Value="256"/>
    </EventCollector>
    <SystemProvider Id="SystemProvider"/>
    <EventProvider Id="DxgKrnl" Name="Microsoft-Windows-DxgKrnl" Level="5">
      <Keywords><Keyword Value="0xffffffffffffffff"/></Keywords>
    </EventProvider>
    <EventProvider Id="DXGI" Name="Microsoft-Windows-DXGI" Level="5"/>
    <EventProvider Id="DwmCore" Name="Microsoft-Windows-Dwm-Core" Level="5"/>
    <EventProvider Id="KernelThread" Guid="{3d6fa8d1-fe05-11d0-9dda-00c04fd7ba7c}" Level="5"/>
    <Profile Id="GuimakerCompare.Verbose.File" Name="GuimakerCompare"
             DetailLevel="Verbose" LoggingMode="File">
      <Collectors>
        <SystemCollectorId Value="SystemCollector">
          <SystemProviderId Value="SystemProvider"/>
        </SystemCollectorId>
        <EventCollectorId Value="EventCollector">
          <EventProviders>
            <EventProviderId Value="DxgKrnl"/>
            <EventProviderId Value="DXGI"/>
            <EventProviderId Value="DwmCore"/>
            <EventProviderId Value="KernelThread"/>
          </EventProviders>
        </EventCollectorId>
      </Collectors>
    </Profile>
  </Profiles>
</WindowsPerformanceRecorder>
"""

# If the classic Thread provider is rejected on a given Windows build, retry
# with presents only. Wakeups are then marked unavailable instead of losing the
# whole external present source.
WPRP_MINIMAL = """<?xml version="1.0" encoding="utf-8"?>
<WindowsPerformanceRecorder Version="1.0">
  <Profiles>
    <EventCollector Id="EventCollector" Name="WPR Event Collector">
      <BufferSize Value="1024"/>
      <Buffers Value="256"/>
    </EventCollector>
    <EventProvider Id="DxgKrnl" Name="Microsoft-Windows-DxgKrnl" Level="5">
      <Keywords><Keyword Value="0xffffffffffffffff"/></Keywords>
    </EventProvider>
    <EventProvider Id="DXGI" Name="Microsoft-Windows-DXGI" Level="5"/>
    <Profile Id="GuimakerCompareMin.Verbose.File" Name="GuimakerCompareMin"
             DetailLevel="Verbose" LoggingMode="File">
      <Collectors>
        <EventCollectorId Value="EventCollector">
          <EventProviders>
            <EventProviderId Value="DxgKrnl"/>
            <EventProviderId Value="DXGI"/>
          </EventProviders>
        </EventCollectorId>
      </Collectors>
    </Profile>
  </Profiles>
</WindowsPerformanceRecorder>
"""


def find_presentmon(explicit: str | None) -> str | None:
    candidates = []
    if explicit:
        candidates.append(explicit)
    env = os.environ.get("PRESENTMON_PATH")
    if env:
        candidates.append(env)
    for name in ("PresentMon.exe", "PresentMon", "presentmon.exe"):
        found = shutil.which(name)
        if found:
            candidates.append(found)
    for candidate in candidates:
        path = Path(candidate)
        if path.is_file():
            return str(path)
    return None


def is_admin() -> bool:
    try:
        return bool(shell32.IsUserAnAdmin())
    except OSError:
        return False


def _to_float(value: str) -> float | None:
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def _normalise_qpc(value: float) -> float:
    # PresentMon 1.x reports large QPC tick counts; 2.x reports seconds.
    if value > 1e9:
        freq = qpc_frequency()
        if freq > 0:
            return value / freq
    return value


def parse_presentmon_csv(
    path: Path,
    qpc_at_process_start: float,
    pid: int | None = None,
    capture_start_wall: float = 0.0,
    process_start_wall: float = 0.0,
) -> dict | None:
    try:
        with path.open("r", encoding="utf-8", errors="replace", newline="") as handle:
            rows = list(csv.DictReader(handle))
    except OSError:
        return None
    if pid is not None:
        filtered = []
        for row in rows:
            value = _to_float(row.get("ProcessID", ""))
            if value is None or int(value) == pid:
                filtered.append(row)
        rows = filtered
    if not rows:
        return None
    time_keys = (
        "CPUStartQPCTime",
        "CPUStartTime",
        "PresentStartTime",
        "TimeInSeconds",
        "Time",
    )
    startup_ms = None
    for row in rows:
        for key in time_keys:
            raw = (row.get(key) or "").strip()
            if not raw:
                continue
            wall = _parse_wall(raw)
            if wall is not None and process_start_wall > 0:
                startup_ms = (wall - process_start_wall) * 1000.0
                break
            value = _to_float(raw)
            if value is None:
                continue
            value = _normalise_qpc(value)
            if qpc_at_process_start > 0 and value >= qpc_at_process_start - 1.0:
                # Absolute QPC seconds from the same clock as `qpc_seconds()`.
                startup_ms = (value - qpc_at_process_start) * 1000.0
            elif capture_start_wall > 0 and process_start_wall > 0:
                # Relative seconds since the PresentMon capture started.
                startup_ms = (
                    (capture_start_wall - process_start_wall) + value
                ) * 1000.0
            break
        if startup_ms is not None:
            break
    if startup_ms is not None and (startup_ms < -50.0 or startup_ms > 120_000.0):
        startup_ms = None
    return {"frames": len(rows), "startup_ms": startup_ms, "source": "presentmon"}


class PresentMonSession:
    def __init__(self, exe: str, csv_path: Path):
        self.exe = exe
        self.csv_path = csv_path
        self.proc = None
        self.reason = ""
        self.started_wall = 0.0

    def start(self, process_name: str) -> bool:
        attempts = [
            [
                "--process_name",
                process_name,
                "--output_file",
                str(self.csv_path),
                "--no_console_stats",
                "--terminate_on_proc_exit",
                "--stop_existing_session",
            ],
            [
                "-process_name",
                process_name,
                "-output_file",
                str(self.csv_path),
                "-no_console_stats",
                "-terminate_on_proc_exit",
            ],
        ]
        for args in attempts:
            try:
                proc = subprocess.Popen(
                    [self.exe, *args],
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                )
            except OSError as error:
                self.reason = str(error)
                return False
            time.sleep(0.5)
            if proc.poll() is None:
                self.proc = proc
                self.started_wall = time.time()
                return True
            self.reason = f"PresentMon exited ({proc.returncode})"
        return False

    def stop(
        self,
        qpc_at_process_start: float,
        pid: int | None = None,
        process_start_wall: float = 0.0,
        timeout: float = 8.0,
    ) -> dict | None:
        if self.proc is not None:
            try:
                self.proc.wait(timeout=timeout)
            except subprocess.TimeoutExpired:
                self.proc.terminate()
                try:
                    self.proc.wait(timeout=3.0)
                except subprocess.TimeoutExpired:
                    self.proc.kill()
        return parse_presentmon_csv(
            self.csv_path,
            qpc_at_process_start,
            pid,
            self.started_wall,
            process_start_wall,
        )


def _parse_wall(value: str) -> float | None:
    text = value.strip()
    if not text:
        return None
    for fmt in (
        "%m/%d/%Y %H:%M:%S.%f",
        "%m/%d/%Y %H:%M:%S",
        "%m/%d/%Y %I:%M:%S.%f %p",
        "%m/%d/%Y %I:%M:%S %p",
        "%Y-%m-%d %H:%M:%S.%f",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S.%f",
    ):
        try:
            return _dt.datetime.strptime(text, fmt).timestamp()
        except ValueError:
            continue
    return None


class WprSession:
    def __init__(self):
        self.exe = shutil.which("wpr") or shutil.which("wpr.exe")
        self.tracerpt = shutil.which("tracerpt") or shutil.which("tracerpt.exe")
        self.proc = None
        self.reason = ""
        self.directory: Path | None = None
        self.etl: Path | None = None
        self.csv: Path | None = None
        self.started = 0.0

    def available(self) -> bool:
        if not self.exe or not self.tracerpt:
            self.reason = "wpr/tracerpt not found"
            return False
        if not is_admin():
            self.reason = "wpr needs administrator rights"
            return False
        return True

    def start(self) -> bool:
        if not self.available():
            return False
        self.directory = Path(tempfile.mkdtemp(prefix="gmcmp-wpr-"))
        self.etl = self.directory / "capture.etl"
        self.csv = self.directory / "capture.csv"
        for index, text in enumerate((WPRP, WPRP_MINIMAL)):
            profile = self.directory / f"compare-{index}.wprp"
            profile.write_text(text, encoding="utf-8")
            result = subprocess.run(
                [self.exe, "-start", str(profile), "-filemode"],
                capture_output=True,
                text=True,
                timeout=120,
            )
            if result.returncode == 0:
                self.started = time.time()
                return True
            self.reason = (result.stderr or result.stdout).strip()
            subprocess.run(
                [self.exe, "-cancel"], capture_output=True, text=True, timeout=60
            )
        self.cleanup()
        return False

    def stop(self, pid: int, process_start_wall: float) -> dict | None:
        if not self.etl or not self.csv:
            return None
        subprocess.run(
            [self.exe, "-stop", str(self.etl)],
            capture_output=True,
            text=True,
            timeout=240,
        )
        if not self.etl.is_file():
            self.reason = "wpr did not write the ETL"
            return None
        subprocess.run(
            [self.tracerpt, str(self.etl), "-o", str(self.csv), "-of", "CSV", "-y"],
            capture_output=True,
            text=True,
            timeout=600,
        )
        return parse_tracerpt_csv(self.csv, pid, process_start_wall)

    def cleanup(self) -> None:
        if self.directory is not None:
            shutil.rmtree(self.directory, ignore_errors=True)


def parse_tracerpt_csv(path: Path, pid: int, process_start_wall: float) -> dict | None:
    """Best-effort parse of a tracerpt CSV: presents + context switches."""
    try:
        handle = path.open("r", encoding="utf-8", errors="replace", newline="")
    except OSError:
        return None
    try:
        reader = csv.reader(handle)
        header = None
        for row in reader:
            lowered = [cell.strip().lower() for cell in row]
            if any("process" in cell for cell in lowered) and any(
                "thread" in cell or cell in ("pid", "tid") for cell in lowered
            ):
                header = [cell.strip() for cell in row]
                break
        if header is None:
            return None

        index = {name.lower(): position for position, name in enumerate(header)}

        def column(*names):
            for name in names:
                position = index.get(name)
                if position is not None:
                    return position
            return None

        pid_index = column("process id", "pid")
        tid_index = column("thread id", "new thread id", "tid")
        provider_index = column("provider name", "provider", "providerid", "provider id")
        event_index = column("event name", "event", "task name")
        time_index = column(
            "time of day", "clock-time", "timestamp", "time stamp", "time"
        )
        if pid_index is None:
            return None

        boot_wall = time.time() - time.perf_counter()
        presents_dxgi = 0
        presents_dwm = 0
        first_dxgi_wall = None
        first_dwm_wall = None
        switches = collections.Counter()

        for row in reader:
            if len(row) <= pid_index:
                continue
            try:
                row_pid = int(float(row[pid_index]))
            except (TypeError, ValueError):
                continue
            if row_pid != pid:
                continue
            provider = row[provider_index] if provider_index is not None and provider_index < len(row) else ""
            event = row[event_index] if event_index is not None and event_index < len(row) else ""
            label = f"{provider} {event}".lower()

            wall = None
            if time_index is not None and time_index < len(row):
                raw = row[time_index].strip()
                wall = _parse_wall(raw)
                if wall is None:
                    number = _to_float(raw)
                    if number is not None:
                        # tracerpt commonly emits 100 ns ticks since boot.
                        wall = boot_wall + number / 10_000_000.0

            # wgpu/D3D apps expose DxgKrnl/DXGI presents; OpenGL apps only show
            # up as DWM present history. Prefer DxgKrnl/DXGI and never add both.
            if "present" in label:
                if "dwm" in provider.lower():
                    presents_dwm += 1
                    if first_dwm_wall is None:
                        first_dwm_wall = wall
                else:
                    presents_dxgi += 1
                    if first_dxgi_wall is None:
                        first_dxgi_wall = wall
            if "cswitch" in label or "context switch" in label or (
                "thread" in provider.lower() and "switch" in label
            ):
                if tid_index is not None and tid_index < len(row):
                    try:
                        switches[int(float(row[tid_index]))] += 1
                    except (TypeError, ValueError):
                        pass

        presents = presents_dxgi or presents_dwm
        first_present_wall = first_dxgi_wall if presents_dxgi else first_dwm_wall
        startup_ms = None
        if first_present_wall is not None and process_start_wall > 0:
            startup_ms = (first_present_wall - process_start_wall) * 1000.0
            if startup_ms < -50.0 or startup_ms > 120_000.0:
                startup_ms = None
        duration = max(0.001, time.time() - process_start_wall)
        return {
            "frames": presents or None,
            "startup_ms": startup_ms,
            "source": "wpr",
            "switches": switches,
            "duration": duration,
        }
    finally:
        handle.close()


# ── App output (cross-check only) ────────────────────────────────────────────


def parse_self_output(stdout: str, stderr: str) -> dict:
    frames = None
    startup = None
    text_chars = None
    memory = None
    backend = None
    present_mode = None
    fonts = None
    for line in stdout.splitlines():
        line = line.strip()
        if line.startswith("frames "):
            frames = int(_to_float(line.split(" ", 1)[1]) or 0)
        elif line.startswith("bench_text_chars "):
            text_chars = int(_to_float(line.split(" ", 1)[1]) or 0)
        elif line.startswith("startup_ms "):
            startup = _to_float(line.split(" ", 1)[1])
        elif line.startswith("backend "):
            backend = line.split(" ", 1)[1].strip()
        elif line.startswith("present_mode "):
            present_mode = line.split(" ", 1)[1].strip()
        elif line.startswith("fonts "):
            fonts = line.split(" ", 1)[1].strip()
    for line in stderr.splitlines():
        line = line.strip()
        if line.startswith("memory-report "):
            try:
                memory = json.loads(line.split(" ", 1)[1])
            except ValueError:
                pass
        elif line.startswith("startup-trace "):
            try:
                payload = json.loads(line.split(" ", 1)[1])
            except ValueError:
                continue
            for mark in payload.get("trace", {}).get("marks", []):
                if mark.get("name") == "engine: first frame presented":
                    startup = _to_float(mark.get("at_ms"))
    return {
        "frames": frames,
        "startup_ms": startup,
        "text_chars": text_chars,
        "memory": memory,
        "backend": backend,
        "present_mode": present_mode,
        "fonts": fonts,
    }


# ── Interactions ─────────────────────────────────────────────────────────────


def hover_sweep(hwnd, duration: float) -> int:
    rect = RECT()
    if not user32.GetWindowRect(hwnd, ctypes.byref(rect)):
        return 0
    raise_window(hwnd)
    moves = 0
    deadline = time.monotonic() + duration
    while time.monotonic() < deadline:
        for y in (rect.top + 120, rect.top + 240, rect.top + 360):
            x = rect.left + 80
            while x < rect.right - 80:
                user32.SetCursorPos(int(x), int(y))
                moves += 1
                time.sleep(1.0 / 120.0)
                x += 24
                if time.monotonic() >= deadline:
                    break
    return moves


def button_hover_worker(pid: int, title: str, duration: float) -> None:
    hwnd = wait_ready(pid, title)
    geometry = client_geometry(hwnd) if hwnd else None
    if not geometry:
        return
    time.sleep(0.4)
    raise_window(hwnd)
    x = int(round(geometry["x"] + geometry["scale"] * BENCH_BUTTON[0]))
    y = int(round(geometry["y"] + geometry["scale"] * (geometry["logical_height"] + BENCH_BUTTON[1])))
    deadline = time.monotonic() + duration
    while time.monotonic() < deadline:
        user32.SetCursorPos(x, y)
        time.sleep(0.25)


def typing_worker(pid: int, title: str, duration: float, holder: list) -> None:
    hwnd = wait_ready(pid, title)
    geometry = client_geometry(hwnd) if hwnd else None
    if not geometry:
        return
    time.sleep(0.6)
    raise_window(hwnd)
    x = int(round(geometry["x"] + geometry["scale"] * BENCH_INPUT[0]))
    y = int(round(geometry["y"] + geometry["scale"] * (geometry["logical_height"] + BENCH_INPUT[1])))
    # A real click focuses the app's real editor before any text arrives.
    click_at(x, y)
    time.sleep(0.3)
    alphabet = "abcdefghij"
    interval = 1.0 / 10.0
    deadline = time.monotonic() + duration
    typed = 0
    index = 0
    while time.monotonic() < deadline:
        if send_unicode(alphabet[index % len(alphabet)]):
            typed += 1
        holder[0] = typed
        index += 1
        time.sleep(interval)


def menu_worker(pid: int, title: str, duration: float) -> None:
    hwnd = wait_ready(pid, title)
    geometry = client_geometry(hwnd) if hwnd else None
    if not geometry:
        return
    time.sleep(0.6)
    raise_window(hwnd)
    x = int(round(geometry["x"] + geometry["scale"] * BENCH_MENU[0]))
    y = int(round(geometry["y"] + geometry["scale"] * (geometry["logical_height"] + BENCH_MENU[1])))
    half = max(1.0, duration / 2.0)
    click_at(x, y)  # open
    time.sleep(min(3.0, half))
    click_at(x, y)  # close
    time.sleep(min(3.0, half))


def resize_worker(pid: int, title: str, duration: float) -> None:
    hwnd = wait_ready(pid, title)
    if not hwnd:
        return
    time.sleep(0.6)
    raise_window(hwnd)
    rect = RECT()
    if not user32.GetWindowRect(hwnd, ctypes.byref(rect)):
        return
    scale = monitor_scale()
    sizes = [(1000, 700), (1280, 800)]
    deadline = time.monotonic() + duration
    index = 0
    while time.monotonic() < deadline:
        logical_w, logical_h = sizes[index % len(sizes)]
        user32.SetWindowPos(
            hwnd,
            -1,
            rect.left,
            rect.top,
            int(logical_w * scale),
            int(logical_h * scale),
            0x0004,  # SWP_NOZORDER
        )
        index += 1
        time.sleep(0.8)


# ── One measured run ─────────────────────────────────────────────────────────


def unique_path(path: Path) -> Path:
    """Never overwrite a previous measurement file."""
    if not path.exists():
        return path
    stamp = time.strftime("%Y%m%d-%H%M%S")
    return path.with_name(f"{path.stem}-{stamp}{path.suffix}")


def binary_name(lib: str) -> str:
    """The executable/window name: `guimaker-compare-<lib>` or the floor name."""
    return lib if lib in FLOOR_LIBS else f"guimaker-compare-{lib}"


def _mean(values: list[float]) -> float:
    return statistics.fmean(values) if values else 0.0


def run_once(
    lib: str,
    scenario: str,
    tier: str,
    run: int,
    fonts: str,
    present_source: str,
    presentmon_path: str | None,
    save_captures: bool,
    out_dir: Path,
    wake_etw: bool,
    reference_capture: dict | None,
) -> tuple[dict, dict | None]:
    binary = binary_name(lib)
    exe = TARGET_DIR / "release" / f"{binary}.exe"
    if not exe.is_file():
        return {"library": lib, "run": run, "scenario": scenario, "tier": tier, "error": f"missing {exe}"}, reference_capture

    seconds, action, _floors, has_tail = SCENARIOS[scenario]
    title = binary
    env = dict(os.environ)
    env["FERROUS_TRACE_STARTUP"] = "1"
    env["FERROUS_TRACE_STARTUP_JSON"] = "1"
    env["FERROUS_PRESENT_MODE"] = "fifo"
    env["ICED_PRESENT_MODE"] = "fifo"
    env["WGPU_BACKEND"] = TIER_ENV.get(tier, "vulkan")
    env["BENCH_DPI_SCALE"] = f"{monitor_scale():.4f}"

    pm = None
    wpr = None
    if present_source in ("auto", "presentmon") and presentmon_path:
        pm = PresentMonSession(
            presentmon_path,
            unique_path(out_dir / f"{lib}-{scenario}-{run}.presentmon.csv"),
        )
        if not pm.start(exe.name):
            pm = None
    # wpr is required when PresentMon is not the source, and it is also the
    # context-switch (wakeup) source whenever the caller did not disable it.
    need_wpr = pm is None or wake_etw
    if need_wpr:
        candidate = WprSession()
        if candidate.available() and candidate.start():
            wpr = candidate

    out_file = tempfile.NamedTemporaryFile(
        "w", suffix=".out", prefix=f"{lib}-", delete=False, encoding="utf-8"
    )
    err_file = tempfile.NamedTemporaryFile(
        "w", suffix=".err", prefix=f"{lib}-", delete=False, encoding="utf-8"
    )

    started = time.perf_counter()
    proc = subprocess.Popen(
        [str(exe), "--seconds", str(seconds), "--fonts", fonts],
        stdout=out_file,
        stderr=err_file,
        env=env,
        cwd=str(ROOT),
    )
    wall_start = time.time()
    qpc0 = qpc_seconds()

    handle = open_process(proc.pid)
    moves_holder = [0]
    typed_holder = [0]
    worker = None
    if action == "hover_body":
        def sweep() -> None:
            hwnd = wait_ready(proc.pid, title)
            time.sleep(0.3)
            moves_holder[0] = hover_sweep(hwnd or 0, seconds)

        worker = threading.Thread(target=sweep, daemon=True)
    elif action == "hover_button":
        worker = threading.Thread(
            target=button_hover_worker, args=(proc.pid, title, seconds), daemon=True
        )
    elif action == "typing":
        worker = threading.Thread(
            target=typing_worker, args=(proc.pid, title, seconds - 0.5, typed_holder), daemon=True
        )
    elif action == "menu":
        worker = threading.Thread(
            target=menu_worker, args=(proc.pid, title, seconds), daemon=True
        )
    elif action == "resize":
        worker = threading.Thread(
            target=resize_worker, args=(proc.pid, title, seconds), daemon=True
        )
    if worker is not None:
        worker.start()

    end = started + seconds
    warm = None
    tail = None
    finish = None
    cpu_samples: list[float] = []
    rss_samples: list[int] = []
    wall_warm = wall_end = 0.0
    cpu_warm = cpu_end = 0.0
    capture = None
    hwnd_at_end = None
    ui_tid = None
    live_threads: dict[int, dict] = {}
    live_modules: list[tuple[int, int, str]] = []
    gpu_bytes = 0
    last_sample = None
    last_sample_at = 0.0
    capture_at = end - 1.2
    tail_at = end - 5.2
    finish_at = end - 0.9
    cpus = os.cpu_count() or 1

    while proc.poll() is None:
        now = time.perf_counter()
        if warm is None and now >= started + 1.0:
            warm = thread_snapshot(proc.pid)
            wall_warm = now
            if handle:
                cpu_warm = process_cpu_seconds(handle)
                last_sample = cpu_warm
                last_sample_at = now
        if has_tail and tail is None and now >= tail_at:
            tail = thread_snapshot(proc.pid)
        if finish is None and now >= finish_at:
            finish = thread_snapshot(proc.pid)
            wall_end = now
            if handle:
                cpu_end = process_cpu_seconds(handle)
            hwnd_at_end = find_window(proc.pid, title, timeout=0.4)
            ui_tid = window_thread_id(hwnd_at_end)
            # Thread names and owning modules only exist while the threads do.
            live_modules = module_map(proc.pid)
            live_threads = {
                tid: describe_thread(tid, live_modules)
                for tid in enumerate_threads(proc.pid)
            }
            gpu_bytes = gpu_memory_bytes(proc.pid)
        if capture is None and now >= capture_at:
            hwnd = find_window(proc.pid, title, timeout=0.4)
            if hwnd:
                capture = capture_window(hwnd)
        if handle and last_sample is not None and now < end - 0.5:
            current = process_cpu_seconds(handle)
            elapsed = now - last_sample_at
            if elapsed > 0:
                cpu_samples.append(max(0.0, (current - last_sample) / elapsed / cpus * 100.0))
            last_sample = current
            last_sample_at = now
            rss_samples.append(process_rss_bytes(handle))
        time.sleep(0.1)

    try:
        proc.wait(timeout=seconds + 30.0)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait()

    if worker is not None:
        worker.join(timeout=3.0)
    if pm is not None:
        presentmon = pm.stop(qpc0, proc.pid, wall_start)
    else:
        presentmon = None
    if wpr is not None:
        etw = wpr.stop(proc.pid, wall_start)
    else:
        etw = None
    if wpr is not None:
        wpr.cleanup()
    if handle:
        kernel32.CloseHandle(handle)

    out_file.close()
    err_file.close()
    out_text = Path(out_file.name).read_text(encoding="utf-8", errors="replace")
    err_text = Path(err_file.name).read_text(encoding="utf-8", errors="replace")
    Path(out_file.name).unlink(missing_ok=True)
    Path(err_file.name).unlink(missing_ok=True)
    self_report = parse_self_output(out_text, err_text)

    external_frames = None
    external_startup = None
    frames_source = "none"
    present_source_used = "none"
    if presentmon:
        external_frames = presentmon.get("frames")
        external_startup = presentmon.get("startup_ms")
        frames_source = present_source_used = "presentmon"
    elif etw and etw.get("frames") is not None:
        external_frames = etw.get("frames")
        external_startup = etw.get("startup_ms")
        frames_source = present_source_used = "wpr"
    elif etw:
        external_startup = etw.get("startup_ms")
        present_source_used = "wpr"

    startup_source = "self"
    startup = self_report["startup_ms"]
    if external_startup is not None:
        startup = external_startup
        startup_source = present_source_used

    duration = max(0.001, (wall_end or time.perf_counter()) - wall_warm) if warm else seconds
    end_snapshot = finish or {}

    def describe(tid: int) -> dict:
        cached = live_threads.get(tid)
        if cached is not None:
            return dict(cached)
        return describe_thread(tid, live_modules)

    deltas: list[tuple[int, float]] = []
    for tid, value in end_snapshot.items():
        deltas.append((tid, max(0.0, value - warm.get(tid, 0.0))))
    deltas.sort(key=lambda item: item[1], reverse=True)

    switches = (etw or {}).get("switches") or {}
    # The ETW session spans the app's whole lifetime; `seconds` is the clean
    # denominator (the harness window is shorter by design).
    switch_duration = max(0.1, seconds)
    threads = []
    for tid, cpu_seconds in deltas[:8]:
        info = describe(tid)
        info["cpu_seconds"] = round(cpu_seconds, 6)
        info["cpu_pct"] = round(cpu_seconds / duration * 100.0 / cpus, 3)
        info["wakeups_per_s"] = (
            round(switches.get(tid, 0) / switch_duration, 2) if switches else None
        )
        threads.append(info)

    tail_top = None
    if has_tail and tail and end_snapshot:
        tail_deltas = []
        for tid, value in end_snapshot.items():
            tail_deltas.append((tid, max(0.0, value - tail.get(tid, 0.0))))
        tail_deltas.sort(key=lambda item: item[1], reverse=True)
        if tail_deltas:
            tail_top = describe(tail_deltas[0][0])["display"]

    diff = diff_captures(reference_capture, capture)
    if capture is not None and save_captures:
        save_bmp(unique_path(out_dir / f"{lib}-{scenario}-{run}.bmp"), capture)

    tail_samples = cpu_samples[-60:]
    top_threads_cpu = [[tid, round(value, 6)] for tid, value in deltas[:5]]
    row = {
        "library": lib,
        "run": run,
        "scenario": scenario,
        "tier": tier,
        "session_id": SESSION_ID,
        "measured_at": _dt.datetime.now(_dt.timezone.utc).isoformat(
            timespec="seconds"
        ),
        "floor": lib in FLOOR_LIBS,
        "mode": scenario,
        "startup_ms": round(startup, 3) if startup is not None else None,
        "startup_ms_self": round(self_report["startup_ms"], 3)
        if self_report["startup_ms"] is not None
        else None,
        "startup_source": startup_source,
        "idle_cpu_pct": round(_mean(cpu_samples), 3),
        "tail_cpu_pct": round(_mean(tail_samples), 3),
        "hover_cpu_pct": round(_mean(cpu_samples), 3)
        if action in ("hover_body", "hover_button")
        else None,
        "button_cpu_pct": round(_mean(cpu_samples), 3) if action == "hover_button" else None,
        "typing_cpu_pct": round(_mean(cpu_samples), 3) if action == "typing" else None,
        "typing_chars": typed_holder[0] if action == "typing" else 0,
        "bench_text_chars": self_report["text_chars"],
        "frames": external_frames,
        "frames_external": external_frames,
        "frames_self": self_report["frames"],
        "frames_source": frames_source,
        # Legacy alias used by the Phase 9B/9C rows.
        "idle_frames": external_frames
        if external_frames is not None
        else self_report["frames"],
        "top_threads_cpu": top_threads_cpu,
        "threads": threads,
        # Every thread alive at the finish snapshot is named, not just the top.
        "thread_names": {str(tid): info["display"] for tid, info in live_threads.items()},
        "tail_top_thread": tail_top,
        "wakeups_per_s": round(sum(switches.values()) / switch_duration, 2)
        if switches
        else None,
        "wakeups_source": "etw-wpr" if switches else "unavailable",
        "gpu_bytes": gpu_bytes,
        "gpu_source": "pdh-gpu-process-memory",
        "rss_bytes": max(rss_samples) if rss_samples else 0,
        "wall_seconds": round(time.perf_counter() - started, 3),
        "hover_moves": moves_holder[0],
        "ui_thread_id": ui_tid,
        "ui_thread_cpu_seconds": next(
            (value for tid, value in deltas if tid == ui_tid), None
        ),
        "pixel_diff": diff,
        "capture_ok": capture is not None
        and capture.get("nonblack_ratio", 0.0) > 0.0005,
        "capture_nonblack_ratio": round(capture.get("nonblack_ratio", 0.0), 4)
        if capture
        else None,
        "backend": tier,
        "present_mode": "fifo",
        "present_source": present_source_used,
        "fonts": fonts,
        "versions": VERSIONS.get(lib, {}),
        "memory": self_report["memory"],
        "features": {},
        "reason": "",
        "winner": None,
        "exit_code": proc.returncode,
    }
    if pm is None and etw is None:
        row["source_warning"] = "no external present source (PresentMon or wpr)"
    elif external_startup is None:
        row["source_warning"] = "external startup unavailable; startup_ms is self-reported"
    return row, (capture if lib == "guimaker" else reference_capture)


# ── Summary ──────────────────────────────────────────────────────────────────


def _stat(values: list[float], digits: int = 3) -> dict | None:
    values = [value for value in values if value is not None]
    if not values:
        return None
    return {
        "median": round(statistics.median(values), digits),
        "min": round(min(values), digits),
        "max": round(max(values), digits),
    }


def _registry_value(path: str, name: str) -> str | None:
    try:
        with winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE, path) as key:
            value, _ = winreg.QueryValueEx(key, name)
            return str(value)
    except OSError:
        return None


def _gpu_adapters() -> list[dict]:
    base = r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}"
    adapters: list[dict] = []
    try:
        with winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE, base) as key:
            index = 0
            while True:
                try:
                    subkey = winreg.EnumKey(key, index)
                except OSError:
                    break
                index += 1
                if not subkey.isdigit():
                    continue
                path = f"{base}\\{subkey}"
                name = _registry_value(path, "DriverDesc")
                if not name:
                    continue
                adapters.append(
                    {
                        "name": name,
                        "driver_version": _registry_value(path, "DriverVersion") or "",
                    }
                )
    except OSError:
        pass
    return adapters


def environment_info() -> dict:
    cpu = _registry_value(
        r"HARDWARE\DESCRIPTION\System\CentralProcessor\0", "ProcessorNameString"
    ) or platform.processor()
    os_name = _registry_value(
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion", "ProductName"
    )
    display = _registry_value(
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion", "DisplayVersion"
    )
    build = _registry_value(
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion", "CurrentBuild"
    )
    os_description = (
        " ".join(part for part in (os_name, display) if part).strip()
        or platform.platform()
    )
    if build:
        os_description += f" (build {build})"
    return {
        "session_id": SESSION_ID,
        "measurement_date": _dt.datetime.now(_dt.timezone.utc).isoformat(
            timespec="seconds"
        ),
        "host": socket.gethostname(),
        "os": os_description,
        "platform": platform.platform(),
        "cpu": cpu,
        "gpus": _gpu_adapters(),
        "dpi_scale": round(monitor_scale(), 4),
        "python": sys.version.split()[0],
        "command": "python " + " ".join(shlex.quote(arg) for arg in sys.argv),
        "protocol": "bench/guimaker_compare/PROTOCOL.md",
    }


def _fmt(value, digits: int = 2) -> str:
    if value is None:
        return "n/a"
    if isinstance(value, int):
        return f"{value:,}"
    return f"{value:.{digits}f}"


def _median(entries: list[dict], key: str):
    values = [entry.get(key) for entry in entries if entry.get(key) is not None]
    return statistics.median(values) if values else None


def _pixel_ratio(entries: list[dict]):
    ratios = []
    for entry in entries:
        value = entry.get("pixel_diff") or {}
        if value.get("comparable"):
            ratios.append(value.get("changed_pixel_ratio", 0.0))
    return statistics.median(ratios) if ratios else None


def annotate_reasons(rows: list[dict]) -> dict:
    """Add a measured `reason` to every row and return it by group.

    The reason is derived from the medians of the group (scenario/tier/app) and
    the GUIMaker baseline in the same tier. No hand-typed claim enters the JSON.
    """
    groups: dict[tuple[str, str, str], list[dict]] = collections.defaultdict(list)
    for row in rows:
        if "error" in row:
            continue
        groups[(row["scenario"], row["tier"], row["library"])].append(row)

    metrics = {}
    for key, entries in groups.items():
        metrics[key] = {
            "startup_ms": _median(entries, "startup_ms"),
            "idle_cpu_pct": _median(entries, "idle_cpu_pct"),
            "frames_external": _median(entries, "frames_external"),
            "rss_bytes": _median(entries, "rss_bytes"),
            "gpu_bytes": _median(entries, "gpu_bytes"),
            "pixel_ratio": _pixel_ratio(entries),
        }

    by_scenario_tier: dict[tuple[str, str], list[tuple[tuple, str]]] = (
        collections.defaultdict(list)
    )
    for (scenario, tier, lib) in groups:
        if lib in FLOOR_LIBS:
            continue
        values = metrics[(scenario, tier, lib)]
        score = (
            values["idle_cpu_pct"] if values["idle_cpu_pct"] is not None else math.inf,
            values["frames_external"]
            if values["frames_external"] is not None
            else math.inf,
            values["rss_bytes"] if values["rss_bytes"] is not None else math.inf,
        )
        by_scenario_tier[(scenario, tier)].append((score, lib))
    winners = {
        key: min(entries)[1] for key, entries in by_scenario_tier.items() if entries
    }

    reasons: dict[tuple[str, str, str], str] = {}
    for key, entries in groups.items():
        scenario, tier, lib = key
        values = metrics[key]
        winner = winners.get((scenario, tier))
        baseline_key = (scenario, tier, "guimaker")
        baseline = metrics.get(baseline_key)
        is_baseline = lib == "guimaker"

        diff = values["pixel_ratio"]
        if lib in FLOOR_LIBS:
            rss_mb = (
                "n/a"
                if values["rss_bytes"] is None
                else f"{values['rss_bytes'] / (1024 * 1024):.1f}"
            )
            reason = (
                "raw-API floor (clear-only, no UI or text); "
                f"idle {_fmt(values['idle_cpu_pct'])}%, "
                f"{_fmt(values['frames_external'])} presents, "
                f"RSS {rss_mb} MB"
            )
            if diff is not None:
                reason += f", pixel diff {diff * 100.0:.2f}% vs GUIMaker"
            reason += "."
        else:
            parts = []
            if lib == winner:
                parts.append("winner (lowest idle CPU, then fewest presents)")
            elif is_baseline:
                parts.append(f"not the winner ({winner} wins)")
            else:
                parts.append("measured against GUIMaker")
            if not is_baseline and baseline is None:
                parts.append("GUIMaker not measured in this tier")
            if values["idle_cpu_pct"] is not None:
                if is_baseline or baseline is None or baseline["idle_cpu_pct"] is None:
                    parts.append(f"idle CPU {_fmt(values['idle_cpu_pct'])}%")
                else:
                    delta = values["idle_cpu_pct"] - baseline["idle_cpu_pct"]
                    parts.append(
                        f"idle CPU {_fmt(values['idle_cpu_pct'])}% "
                        f"({delta:+.2f} pp vs GUIMaker)"
                    )
            if values["frames_external"] is not None:
                if is_baseline or baseline is None or baseline["frames_external"] is None:
                    parts.append(f"{_fmt(values['frames_external'])} presents")
                else:
                    delta = values["frames_external"] - baseline["frames_external"]
                    parts.append(
                        f"{_fmt(values['frames_external'])} presents "
                        f"({delta:+.0f} vs GUIMaker)"
                    )
            if values["rss_bytes"] is not None:
                if is_baseline or baseline is None or baseline["rss_bytes"] is None:
                    parts.append(f"RSS {values['rss_bytes'] / (1024 * 1024):.1f} MB")
                else:
                    delta = (values["rss_bytes"] - baseline["rss_bytes"]) / (1024 * 1024)
                    parts.append(
                        f"RSS {values['rss_bytes'] / (1024 * 1024):.1f} MB "
                        f"({delta:+.1f} MB vs GUIMaker)"
                    )
            if values["startup_ms"] is not None:
                if is_baseline or baseline is None or baseline["startup_ms"] is None:
                    parts.append(f"startup {values['startup_ms']:.0f} ms")
                else:
                    delta = values["startup_ms"] - baseline["startup_ms"]
                    parts.append(
                        f"startup {values['startup_ms']:.0f} ms ({delta:+.0f} ms)"
                    )
            if values["gpu_bytes"] is not None:
                if is_baseline or baseline is None or baseline["gpu_bytes"] is None:
                    parts.append(f"GPU {values['gpu_bytes'] / (1024 * 1024):.1f} MB")
                else:
                    delta = (values["gpu_bytes"] - baseline["gpu_bytes"]) / (1024 * 1024)
                    parts.append(
                        f"GPU {values['gpu_bytes'] / (1024 * 1024):.1f} MB "
                        f"({delta:+.1f} MB)"
                    )
            if diff is not None:
                parts.append(f"pixel diff {diff * 100.0:.2f}% vs GUIMaker")
            if is_baseline and winner and winner != lib:
                winner_values = metrics.get((scenario, tier, winner), {})
                parts.append(
                    f"{winner} wins with idle {_fmt(winner_values.get('idle_cpu_pct'))}% "
                    f"and {_fmt(winner_values.get('frames_external'))} presents"
                )
            reason = "; ".join(parts) + "."

        reasons[key] = reason
        for entry in entries:
            entry["reason"] = reason
            entry["winner"] = winner
    return {
        "session_id": SESSION_ID,
        "winners": {f"{scenario}/{tier}": lib for (scenario, tier), lib in winners.items()},
        "reasons": {
            f"{scenario}/{tier}/{lib}": text
            for (scenario, tier, lib), text in reasons.items()
        },
    }


def summarise(rows: list[dict], tiers: list[str], interactive_tier: str) -> dict:
    groups: dict[tuple[str, str, str], list[dict]] = collections.defaultdict(list)
    for row in rows:
        if "error" in row:
            continue
        groups[(row["scenario"], row["tier"], row["library"])].append(row)

    summary: dict = {"tiers": {}, "interactive": {}, "best_backend": {}}
    keys = (
        "startup_ms",
        "idle_cpu_pct",
        "tail_cpu_pct",
        "frames_external",
        "rss_bytes",
        "gpu_bytes",
        "wakeups_per_s",
        "pixel_diff",
    )
    for (scenario, tier, lib), entries in groups.items():
        stats = {}
        for key in keys:
            if key == "pixel_diff":
                ratios = []
                for entry in entries:
                    value = entry.get("pixel_diff") or {}
                    if value.get("comparable"):
                        ratios.append(value.get("changed_pixel_ratio", 0.0))
                stats[key] = _stat(ratios, 4)
            else:
                stats[key] = _stat([entry.get(key) for entry in entries])
        target = summary["tiers"] if scenario.startswith("idle") else summary["interactive"]
        target.setdefault(scenario, {}).setdefault(tier, {})[lib] = stats

    for lib in ALL_LIBS:
        best = None
        for tier in tiers:
            entries = groups.get(("idle6", tier, lib), [])
            if not entries:
                continue
            cpu = statistics.median([e["idle_cpu_pct"] for e in entries])
            frames = statistics.median(
                [e["frames_external"] for e in entries if e.get("frames_external") is not None]
                or [float("inf")]
            )
            score = (frames, cpu)
            if best is None or score < best[0]:
                best = (score, tier, cpu, frames)
        if best is not None:
            summary["best_backend"][lib] = {
                "tier": best[1],
                "idle_cpu_pct": round(best[2], 3),
                "frames_external": best[3],
            }
    summary["interactive_tier"] = interactive_tier
    return summary


def print_summary(summary: dict, tiers: list[str], interactive_tier: str) -> None:
    def line(lib: str, stats: dict) -> str:
        def fmt(key, digits=3):
            value = stats.get(key)
            if not value:
                return "n/a"
            return f"{value['median']:.{digits}f} [{value['min']:.{digits}f}..{value['max']:.{digits}f}]"

        return (
            f"  {lib:<18} startup {fmt('startup_ms', 1)} ms  "
            f"idle {fmt('idle_cpu_pct', 2)} %  "
            f"frames {fmt('frames_external', 1)}  "
            f"rss {fmt('rss_bytes', 0)} B  gpu {fmt('gpu_bytes', 0)} B"
        )

    for scenario, tier_table in summary["tiers"].items():
        for tier in tiers:
            entries = tier_table.get(tier)
            if not entries:
                continue
            print(f"\n{scenario} / tier {tier}")
            for lib in LIB_TIERS.get(tier, []):
                if lib in entries:
                    print(line(lib, entries[lib]))

    for scenario, tier_table in summary["interactive"].items():
        entries = tier_table.get(interactive_tier)
        if not entries:
            continue
        print(f"\n{scenario} / tier {interactive_tier}")
        for lib, stats in entries.items():
            print(line(lib, stats))

    if summary.get("best_backend"):
        print("\nbest backend per library on this OS (idle 6 s)")
        for lib, best in summary["best_backend"].items():
            print(
                f"  {lib:<18} tier {best['tier']:<7} "
                f"idle {best['idle_cpu_pct']:.2f} %  frames {best['frames_external']}"
            )


# ── Host prerequisites and CLI ───────────────────────────────────────────────


def host_free_banner() -> None:
    print("=" * 78)
    print("STOP. Free the host before measuring:")
    print("  close browsers, chat, video and every other GPU-heavy app;")
    print("  leave only the terminal that will run this harness;")
    print("  C: must stay above 5 GB free; nothing in results/ is deleted.")
    print("Then re-run with --confirmed-host-free.")
    print("=" * 78)


def disk_guard() -> bool:
    try:
        free = shutil.disk_usage("C:\\").free
    except OSError:
        return True
    if free < 5 * 1024**3:
        print(
            f"STOP: C: has {free / 1024**3:.2f} GiB free, below the 5 GiB floor.",
            file=sys.stderr,
        )
        return False
    return True


def verify_versions() -> None:
    lock = ROOT / "Cargo.lock"
    if not lock.is_file():
        return
    text = lock.read_text(encoding="utf-8", errors="replace")
    for lib, table in VERSIONS.items():
        for crate, version in table.items():
            if crate.startswith("ferrous") or "(workspace)" in version:
                continue
            needle = f'name = "{crate}"'
            if needle not in text:
                print(f"warning: {crate} for {lib} is not in Cargo.lock", file=sys.stderr)
                continue
            if f'version = "{version}"' not in text:
                print(
                    f"warning: {crate} {version} ({lib}) not found in Cargo.lock",
                    file=sys.stderr,
                )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Phase 9D comparison harness")
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument(
        "--scenario",
        action="append",
        choices=sorted(SCENARIOS),
        help="scenario(s); defaults to idle6 plus any legacy flag",
    )
    parser.add_argument("--tier", action="append", choices=sorted(LIB_TIERS))
    parser.add_argument("--interactive-tier", choices=sorted(LIB_TIERS), default="vulkan")
    parser.add_argument("--only", nargs="*", default=None)
    parser.add_argument("--fonts", choices=["roboto", "default"], default="roboto")
    parser.add_argument("--out", type=Path, default=ROOT / "results")
    parser.add_argument("--target-dir", type=Path, default=ROOT / "target")
    parser.add_argument("--build", action="store_true")
    parser.add_argument("--presentmon", default=None)
    parser.add_argument(
        "--present-source", choices=["auto", "presentmon", "wpr"], default="auto"
    )
    parser.add_argument("--no-wake-etw", action="store_true")
    parser.add_argument("--save-captures", action="store_true")
    parser.add_argument("--allow-self-report", action="store_true")
    parser.add_argument("--confirmed-host-free", action="store_true")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument(
        "--write-readme",
        action="store_true",
        help="rewrite the marked performance sections of both READMEs after measuring",
    )
    parser.add_argument(
        "--render-only",
        action="store_true",
        help="refresh both READMEs from existing results and exit (no measurement)",
    )
    # Legacy flags map onto scenarios.
    parser.add_argument("--hover", action="store_true")
    parser.add_argument("--typing", action="store_true")
    parser.add_argument("--button-hover", action="store_true")
    parser.add_argument("--seconds", type=float, default=None, help=argparse.SUPPRESS)
    return parser.parse_args()


def main() -> int:
    global TARGET_DIR
    args = parse_args()
    TARGET_DIR = args.target_dir
    enable_dpi_awareness()

    if args.render_only:
        import render_readme

        return render_readme.render_and_write(results_dir=args.out)

    scenarios = list(args.scenario or [])
    if args.hover and "hover_body" not in scenarios:
        scenarios.append("hover_body")
    if args.typing and "typing" not in scenarios:
        scenarios.append("typing")
    if args.button_hover and "hover_button" not in scenarios:
        scenarios.append("hover_button")
    if not scenarios:
        scenarios = ["idle6"]

    tiers = list(args.tier or ["vulkan", "dx12", "gl"])
    only = args.only or None
    presentmon = find_presentmon(args.presentmon)
    if args.present_source == "presentmon" and not presentmon:
        print("--present-source presentmon requested but PresentMon was not found.", file=sys.stderr)
        return 2

    host_free_banner()
    if not args.dry_run and not args.confirmed_host_free:
        return 2
    if args.dry_run:
        print("dry run: no process will be started.")
        print(f"  PresentMon: {presentmon or 'not found (ask before installing; nothing is installed)'}")
        print(f"  wpr fallback: {'available' if WprSession().available() else 'unavailable'}")
        print(f"  monitor scale: {monitor_scale():.3f}")
        print(f"  scenarios: {', '.join(scenarios)}")
        print(f"  tiers: {', '.join(tiers)}")
        for scenario in scenarios:
            allowed = LIB_TIERS if SCENARIOS[scenario][2] else {args.interactive_tier: LIB_TIERS[args.interactive_tier]}
            for tier, libs in allowed.items():
                if SCENARIOS[scenario][2] and tier not in tiers:
                    continue
                libs = [lib for lib in libs if not only or lib in only]
                if not SCENARIOS[scenario][2]:
                    libs = [lib for lib in libs if lib not in FLOOR_LIBS]
                print(f"  {scenario} / {tier}: {', '.join(libs)} x {args.runs} runs")
        return 0
    if not disk_guard():
        return 3

    verify_versions()
    if args.build:
        subprocess.run(
            ["cargo", "build", "--release", "--workspace"],
            cwd=ROOT,
            check=True,
        )

    if not presentmon and not WprSession().available():
        message = (
            "No external present source: install PresentMon yourself (ask the user "
            "first) or run elevated so wpr can capture ETW."
        )
        if not args.allow_self_report:
            print(message, file=sys.stderr)
            return 4
        print("warning: falling back to self-reported frames", file=sys.stderr)

    if not presentmon:
        print(
            "PresentMon was not found; nothing was installed. Using the built-in "
            "wpr ETW capture (needs administrator). Ask the user before installing "
            "PresentMon and re-run with --presentmon PATH to prefer it.",
            file=sys.stderr,
        )

    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / "versions.json").write_text(
        json.dumps(VERSIONS, indent=2, sort_keys=True), encoding="utf-8"
    )

    base_out = args.out if args.fonts == "roboto" else args.out / "fonts-default"
    rows: list[dict] = []
    written: list[tuple[Path, dict]] = []
    # One GUIMaker reference capture per (scenario, tier), reused across runs so
    # an even run (reversed order) still diffs against it.
    references: dict[tuple[str, str], dict] = {}
    for scenario in scenarios:
        seconds, _action, floors_allowed, _has_tail = SCENARIOS[scenario]
        effective_tiers = tiers if floors_allowed else [args.interactive_tier]
        for tier in effective_tiers:
            libs = list(LIB_TIERS[tier])
            if not floors_allowed:
                libs = [lib for lib in libs if lib not in FLOOR_LIBS]
            if only:
                libs = [lib for lib in libs if lib in only]
            for run in range(1, args.runs + 1):
                order = libs if run % 2 == 1 else list(reversed(libs))
                run_dir = (
                    base_out / f"tier-{tier}" / scenario
                    if floors_allowed
                    else base_out / scenario
                )
                run_dir.mkdir(parents=True, exist_ok=True)
                for lib in order:
                    row, reference = run_once(
                        lib,
                        scenario,
                        tier,
                        run,
                        args.fonts,
                        args.present_source if presentmon else "wpr",
                        presentmon,
                        args.save_captures,
                        run_dir,
                        not args.no_wake_etw,
                        references.get((scenario, tier)),
                    )
                    if reference is not None:
                        references[(scenario, tier)] = reference
                    rows.append(row)
                    path = unique_path(run_dir / f"{lib}-run{run}.json")
                    written.append((path, row))
                    path.write_text(json.dumps(row, indent=2), encoding="utf-8")
                    print(
                        f"{scenario}/{tier} {lib} run {run}: "
                        f"startup {row.get('startup_ms')} ms, "
                        f"idle {row.get('idle_cpu_pct')} %, "
                        f"frames {row.get('frames_external')} "
                        f"({row.get('frames_source')}), "
                        f"rss {row.get('rss_bytes')} B"
                    )
                    if row.get("frames_external") is None and not args.allow_self_report:
                        print(
                            "ERROR: no external presented-frame count; the run is not a "
                            "measurement (see PROTOCOL.md section 1).",
                            file=sys.stderr,
                        )
                        return 5
                    if not row.get("capture_ok"):
                        print(
                            f"warning: PrintWindow capture failed for {lib} "
                            f"({scenario}/{tier}); pixel_diff is not comparable",
                            file=sys.stderr,
                        )

    reasons = annotate_reasons(rows)
    # Persist the annotated rows so the README generator can enforce `reason`.
    for path, row in written:
        path.write_text(json.dumps(row, indent=2), encoding="utf-8")

    environment = environment_info()
    (base_out / "environment.json").write_text(
        json.dumps(environment, indent=2, sort_keys=True), encoding="utf-8"
    )
    summary = summarise(rows, tiers, args.interactive_tier)
    summary["environment"] = environment
    summary["reasons"] = reasons
    (base_out / "summary.json").write_text(
        json.dumps(summary, indent=2, sort_keys=True), encoding="utf-8"
    )
    print_summary(summary, tiers, args.interactive_tier)

    if args.write_readme:
        import render_readme

        render_readme.render_and_write(results_dir=args.out)
    else:
        print(
            "hint: re-run with --write-readme (or run `python run_compare.py "
            "--render-only`) to refresh the marked Performance sections.",
            file=sys.stderr,
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
