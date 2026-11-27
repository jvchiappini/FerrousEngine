//! Window appearance at startup: painted, never white, and only when ready.
//!
//! winit registers its window class with `hbrBackground: 0`, so Windows erases a
//! new window with the system window colour — the white flash. The engine avoids
//! it in two ways (prompt 17 S3/S3b):
//!
//! - the window is created hidden and, by default, shown only when the first
//!   frame is presented ([`show_ready`]), so the first visible content is the
//!   application;
//! - when the first frame is late, [`show_timeout`] shows the window painted
//!   with the app background colour, the same colour the renderer clears to.

use ferrous_core::Color;
use winit::window::Window;

/// Shows `window`: its content is the frame that was just presented.
///
/// After the show it takes the keyboard. A window created hidden is normally
/// activated by `ShowWindow`, but Windows applies the process's `STARTUPINFO`
/// show command to the *first* `ShowWindow`, so an app launched by a harness
/// (or any launcher that asked for a hidden window) appears without focus and
/// never receives typing. An app the user launched must take focus, so ask for
/// it explicitly (prompt 15 Phase 9D finding 4). Popups are separate: they are
/// created `WS_EX_NOACTIVATE` and never come through here.
pub(crate) fn show_ready(window: &Window) {
    #[cfg(target_os = "windows")]
    if let Ok(handle) = window.window_handle() {
        if let RawWindowHandle::Win32(win32) = handle.as_raw() {
            disable_open_transition(win32.hwnd.get() as HWND);
        }
    }
    window.set_visible(true);
    window.focus_window();
}

/// Shows `window` painted with `background` because the first frame is late.
///
/// On Windows the window class background brush is set to `background` and the
/// window is shown and erased immediately, so the user sees the app colour
/// instead of a white rectangle. On other platforms the window is shown and the
/// renderer's clear frame fills it.
pub(crate) fn show_timeout(window: &Window, background: Color) {
    #[cfg(target_os = "windows")]
    {
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let Ok(handle) = window.window_handle() else {
            window.set_visible(true);
            return;
        };
        let RawWindowHandle::Win32(win32) = handle.as_raw() else {
            window.set_visible(true);
            return;
        };
        let hwnd = win32.hwnd.get() as *mut core::ffi::c_void;
        disable_open_transition(hwnd as HWND);
        let brush = set_class_brush(hwnd, background);
        // Fill the client area while the window is still hidden: the first
        // composition on `ShowWindow` is already the app colour, not an
        // unpainted (white) surface. The class brush only paints on the next
        // erase, which can be a vsync late.
        if let Some(brush) = brush {
            // SAFETY: `hwnd` is the live window; the DC and the client rect
            // describe its client area and are released here.
            unsafe {
                let mut rect: RECT = std::mem::zeroed();
                let dc = GetDC(hwnd as HWND);
                if !dc.is_null() {
                    if GetClientRect(hwnd as HWND, &mut rect) != 0 {
                        FillRect(dc, &rect, brush as *mut core::ffi::c_void);
                    }
                    ReleaseDC(hwnd as HWND, dc);
                }
            }
        }
        window.set_visible(true);
        // Erase now too: covers a resize between the fill and the show.
        // SAFETY: the window handle is valid; the optional rectangle and
        // region are null, meaning "the whole client area".
        unsafe {
            RedrawWindow(
                hwnd,
                std::ptr::null(),
                std::ptr::null_mut(),
                RDW_INVALIDATE | RDW_ERASE | RDW_UPDATENOW,
            );
        }
    }
    #[cfg(not(target_os = "windows"))]
    window.set_visible(true);
    window.focus_window();
}

/// Sets the window class background brush to `background`, once per colour.
///
/// Windows owns a class brush until the class is unregistered, so a fresh brush
/// per window creation would leak one handle per window. The engine keeps the
/// first brush per colour for the life of the process (an app uses one or two
/// colours) and reuses it.
#[cfg(target_os = "windows")]
fn set_class_brush(hwnd: *mut core::ffi::c_void, background: Color) -> Option<isize> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    static BRUSHES: OnceLock<Mutex<HashMap<u32, isize>>> = OnceLock::new();

    // The brush repeats the renderer's clear colour: Color channels are
    // linear, COLORREF wants sRGB bytes.
    let channel = |value: f32| (linear_to_srgb(value.clamp(0.0, 1.0)) * 255.0).round() as u32;
    let colorref =
        channel(background.r) | (channel(background.g) << 8) | (channel(background.b) << 16);

    let brushes = BRUSHES.get_or_init(|| Mutex::new(HashMap::new()));
    let Ok(mut brushes) = brushes.lock() else {
        return None;
    };
    let brush = *brushes.entry(colorref).or_insert_with(|| {
        // SAFETY: CreateSolidBrush allocates a GDI object; a null return is
        // handled by the caller (the window keeps its default background).
        unsafe { CreateSolidBrush(colorref) as isize }
    });
    if brush == 0 {
        return None;
    }
    // SAFETY: the brush is owned by this process for its lifetime;
    // GCLP_HBRBACKGROUND expects an HBRUSH.
    unsafe {
        SetClassLongPtrW(hwnd, GCLP_HBRBACKGROUND, brush);
    }
    Some(brush)
}

/// Turns off DWM's window-open fade for `hwnd`.
///
/// Windows 11 fades and scales a new window over ~100 ms. An application that
/// waited for its first frame to show the window does not want that: the fade
/// blends the frame with whatever was on screen, and a screenshot of the first
/// visible moment would not be the frame. With transitions disabled the first
/// composed pixel is the frame itself.
#[cfg(target_os = "windows")]
fn disable_open_transition(hwnd: HWND) {
    use windows_sys::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_TRANSITIONS_FORCEDISABLED,
    };
    let disabled: i32 = 1;
    // SAFETY: DWM reads `disabled` (a BOOL) during the call.
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_TRANSITIONS_FORCEDISABLED as u32,
            (&disabled as *const i32).cast(),
            std::mem::size_of::<i32>() as u32,
        );
    }
}

/// The sRGB encoding of a linear channel, as GDI expects for COLORREF.
#[cfg(target_os = "windows")]
fn linear_to_srgb(value: f32) -> f32 {
    if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

#[cfg(target_os = "windows")]
use windows_sys::Win32::Foundation::{HWND, RECT};
#[cfg(target_os = "windows")]
use windows_sys::Win32::Graphics::Gdi::{
    CreateSolidBrush, FillRect, GetDC, RedrawWindow, ReleaseDC, RDW_ERASE, RDW_INVALIDATE,
    RDW_UPDATENOW,
};
#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetClientRect, SetClassLongPtrW, GCLP_HBRBACKGROUND,
};
#[cfg(target_os = "windows")]
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
