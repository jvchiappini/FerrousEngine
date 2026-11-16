//! Window appearance at startup: the app background colour, never white.
//!
//! winit registers its window class with `hbrBackground: 0`, so Windows erases a
//! new window with the system window colour — the white flash before the first
//! frame. The engine paints the window with the application background colour
//! before any GPU work and only then shows it.

use ferrous_core::Color;
use winit::window::Window;

/// Shows `window` painted with `background`, returning whether it is visible.
///
/// On Windows the window class background brush is set to `background` and the
/// window is shown and erased immediately, so the user sees the right colour at
/// window-creation time instead of waiting for the swapchain. On other
/// platforms the window stays hidden until the first cleared frame is
/// presented, which avoids a white flash at the cost of showing later; `false`
/// tells the runner to show it after that frame.
#[cfg(target_os = "windows")]
pub(crate) fn show_with_background(window: &Window, background: Color) -> bool {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    // Always show the window even when the brush cannot be set: a visible
    // white window is better than an invisible app.
    let shown = || true;
    let Ok(handle) = window.window_handle() else {
        window.set_visible(true);
        return shown();
    };
    let RawWindowHandle::Win32(win32) = handle.as_raw() else {
        window.set_visible(true);
        return shown();
    };

    // COLORREF is 0x00BBGGRR. The config colour is authored in sRGB bytes, the
    // same space the surface clear uses for the neutral backgrounds apps pick.
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u32;
    let colorref =
        channel(background.r) | (channel(background.g) << 8) | (channel(background.b) << 16);

    // SAFETY: `hwnd` is the live window winit created. The brush is handed to
    // the window class (which owns it until the class is unregistered); it is
    // deliberately not deleted while the window exists.
    let brush = unsafe { CreateSolidBrush(colorref) };
    if !brush.is_null() {
        // SAFETY: see above; GCLP_HBRBACKGROUND expects an HBRUSH.
        unsafe {
            SetClassLongPtrW(
                win32.hwnd.get() as *mut core::ffi::c_void,
                GCLP_HBRBACKGROUND,
                brush as isize,
            );
        }
    }
    window.set_visible(true);
    // Erase now instead of at the next paint: WM_ERASEBKGND fills the client
    // area with the class brush.
    // SAFETY: the window handle is valid; the optional rectangle and region
    // are left null to mean "the whole client area".
    unsafe {
        RedrawWindow(
            win32.hwnd.get() as *mut core::ffi::c_void,
            std::ptr::null(),
            std::ptr::null_mut(),
            RDW_INVALIDATE | RDW_ERASE | RDW_UPDATENOW,
        );
    }
    shown()
}

/// Non-Windows: the runner shows the window after the first cleared frame.
#[cfg(not(target_os = "windows"))]
pub(crate) fn show_with_background(_window: &Window, _background: Color) -> bool {
    false
}

#[cfg(target_os = "windows")]
use windows_sys::Win32::Graphics::Gdi::{
    CreateSolidBrush, RedrawWindow, RDW_ERASE, RDW_INVALIDATE, RDW_UPDATENOW,
};
#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::WindowsAndMessaging::{SetClassLongPtrW, GCLP_HBRBACKGROUND};
