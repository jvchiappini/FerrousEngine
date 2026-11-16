//! Window appearance at startup: the app background colour, never white, and
//! the CPU-painted launch snapshot (prompt 17 S3b).
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

/// A child window that paints the launch snapshot with GDI.
///
/// It is created over the whole client area of the (still hidden) window, then
/// the window is shown: the first thing the user sees is the last rendered
/// screen, tens of milliseconds after the process starts instead of after the
/// GPU driver's ~430 ms. The child is destroyed when the first GPU frame is
/// presented; by then the swapchain and the snapshot show the same pixels.
///
/// Why a child window and not a `WM_PAINT` subclass: winit owns the parent's
/// window procedure (and its `GWLP_USERDATA`), while our own window class keeps
/// a private state pointer and leaves winit's message handling untouched. The
/// child is `WS_EX_TRANSPARENT` and answers `HTTRANSPARENT`, so clicks, keys,
/// the wheel and the close button keep reaching the parent (the runner queues
/// them until the handoff) instead of dying in an invisible overlay.
pub(crate) struct SnapshotSurface {
    #[cfg(target_os = "windows")]
    hwnd: windows_sys::Win32::Foundation::HWND,
}

impl SnapshotSurface {
    /// Paints `image` over `window`'s client area and shows nothing itself:
    /// the caller shows the window.
    ///
    /// `None` on platforms without a painter (the runner then shows the window
    /// painted with the background colour, the S3 path).
    #[cfg(target_os = "windows")]
    pub(crate) fn show(window: &Window, image: crate::snapshot::SnapshotImage) -> Option<Self> {
        Some(Self {
            hwnd: win32::create(window, image)?,
        })
    }

    /// Non-Windows: no CPU painter yet.
    #[cfg(not(target_os = "windows"))]
    pub(crate) fn show(_window: &Window, _image: crate::snapshot::SnapshotImage) -> Option<Self> {
        None
    }

    /// Paints the snapshot now (call it after the window is visible).
    pub(crate) fn repaint(&self) {
        #[cfg(target_os = "windows")]
        win32::paint_now(self.hwnd);
    }
}

impl Drop for SnapshotSurface {
    fn drop(&mut self) {
        #[cfg(target_os = "windows")]
        win32::destroy(self.hwnd);
    }
}

#[cfg(target_os = "windows")]
mod win32 {
    //! The Win32 side of [`super::SnapshotSurface`].
    use core::ffi::c_void;
    use std::mem::size_of;

    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::{
        BeginPaint, EndPaint, RedrawWindow, SetDIBitsToDevice, BITMAPINFO, BITMAPINFOHEADER,
        BI_RGB, DIB_RGB_COLORS, PAINTSTRUCT, RDW_ERASE, RDW_INVALIDATE, RDW_UPDATENOW,
    };
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetWindowLongPtrW,
        RegisterClassExW, SetWindowLongPtrW, GWLP_USERDATA, HTTRANSPARENT, MA_NOACTIVATE,
        WM_ERASEBKGND, WM_MOUSEACTIVATE, WM_NCDESTROY, WM_NCHITTEST, WM_PAINT, WNDCLASSEXW,
        WS_CHILD, WS_CLIPSIBLINGS, WS_EX_NOACTIVATE, WS_EX_TRANSPARENT, WS_VISIBLE,
    };
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    use crate::snapshot::SnapshotImage;

    const CLASS_NAME: &str = "FerrousLaunchSnapshot";

    /// What a child window paints; owned by the child (`GWLP_USERDATA`).
    struct Paint {
        image: SnapshotImage,
    }

    /// A wide, null-terminated copy of `text`.
    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match message {
            WM_PAINT => {
                // SAFETY: `hwnd` is our child; `PAINTSTRUCT` is a plain C struct
                // and BeginPaint/EndPaint bracket the only painting.
                unsafe {
                    let mut paint: PAINTSTRUCT = std::mem::zeroed();
                    let hdc = BeginPaint(hwnd, &mut paint);
                    let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Paint;
                    if !state.is_null() {
                        let state = &*state;
                        let width = state.image.width();
                        let height = state.image.height();
                        let mut info: BITMAPINFO = std::mem::zeroed();
                        info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
                        info.bmiHeader.biWidth = width as i32;
                        // Negative height: top-down, the order the capture made.
                        info.bmiHeader.biHeight = -(height as i32);
                        info.bmiHeader.biPlanes = 1;
                        info.bmiHeader.biBitCount = 32;
                        info.bmiHeader.biCompression = BI_RGB;
                        SetDIBitsToDevice(
                            hdc,
                            0,
                            0,
                            width,
                            height,
                            0,
                            0,
                            0,
                            height,
                            state.image.dib().as_ptr().cast::<c_void>(),
                            &info,
                            DIB_RGB_COLORS,
                        );
                    }
                    EndPaint(hwnd, &paint);
                }
                0
            }
            // Nothing to erase: `WM_PAINT` overwrites every pixel.
            WM_ERASEBKGND => 1,
            // Let mouse input reach the parent (winit's window) so the runner
            // can queue it until the handoff.
            WM_NCHITTEST => HTTRANSPARENT as LRESULT,
            // A click must not activate the child instead of the app window.
            WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
            WM_NCDESTROY => {
                // SAFETY: the state was boxed in `create`; the window that owned
                // it is going away, so nobody else reads it. `Paint::drop`
                // releases the GDI handles.
                unsafe {
                    let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Paint;
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                    if !state.is_null() {
                        drop(Box::from_raw(state));
                    }
                }
                // SAFETY: default handling for the destruction of our child.
                unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
            }
            _ => {
                // SAFETY: pass everything else to the default procedure.
                unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
            }
        }
    }

    /// Registers the (single, process-wide) child class.
    fn register_class() -> bool {
        static REGISTERED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *REGISTERED.get_or_init(|| {
            let class = wide(CLASS_NAME);
            let mut description: WNDCLASSEXW = unsafe { std::mem::zeroed() };
            description.cbSize = size_of::<WNDCLASSEXW>() as u32;
            description.lpfnWndProc = Some(window_proc);
            // SAFETY: no module handle argument means "the main module".
            description.hInstance = unsafe { GetModuleHandleW(std::ptr::null()) };
            description.lpszClassName = class.as_ptr();
            // SAFETY: the description and the class name live for the call.
            unsafe { RegisterClassExW(&description) != 0 }
        })
    }

    /// Creates the child that paints `image` over `window`'s client area.
    pub(super) fn create(window: &winit::window::Window, image: SnapshotImage) -> Option<HWND> {
        let handle = window.window_handle().ok()?;
        let RawWindowHandle::Win32(win32) = handle.as_raw() else {
            return None;
        };
        if !register_class() {
            return None;
        }
        let parent = win32.hwnd.get() as HWND;
        // SAFETY: `parent` is the live winit window; `RECT` is a plain struct.
        let mut rect: RECT = unsafe { std::mem::zeroed() };
        // SAFETY: see above.
        if unsafe { GetClientRect(parent, &mut rect) } == 0 {
            return None;
        }
        let width = (rect.right - rect.left).max(1);
        let height = (rect.bottom - rect.top).max(1);
        let state = Box::into_raw(Box::new(Paint { image }));
        let class = wide(CLASS_NAME);
        let title = wide("");
        // SAFETY: no module handle argument means "the main module".
        let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
        // SAFETY: the class is registered, the parent is live and `state` stays
        // owned by the child until WM_NCDESTROY frees it.
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_TRANSPARENT | WS_EX_NOACTIVATE,
                class.as_ptr(),
                title.as_ptr(),
                WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS,
                0,
                0,
                width,
                height,
                parent,
                std::ptr::null_mut(),
                instance,
                state.cast::<c_void>(),
            )
        };
        if hwnd.is_null() {
            // SAFETY: the window was never created, so the state was not stored.
            unsafe { drop(Box::from_raw(state)) };
            return None;
        }
        Some(hwnd)
    }

    /// Paints the child now, synchronously.
    pub(super) fn paint_now(hwnd: HWND) {
        // SAFETY: `hwnd` is our live child; null rectangles mean the whole area.
        unsafe {
            RedrawWindow(
                hwnd,
                std::ptr::null(),
                std::ptr::null_mut(),
                RDW_INVALIDATE | RDW_ERASE | RDW_UPDATENOW,
            );
        }
    }

    /// Destroys the child (its `WM_NCDESTROY` frees the state).
    pub(super) fn destroy(hwnd: HWND) {
        if !hwnd.is_null() {
            // SAFETY: `hwnd` is our live child and is not used afterwards.
            unsafe {
                DestroyWindow(hwnd);
            }
        }
    }
}

#[cfg(target_os = "windows")]
use windows_sys::Win32::Graphics::Gdi::{
    CreateSolidBrush, RedrawWindow, RDW_ERASE, RDW_INVALIDATE, RDW_UPDATENOW,
};
#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::WindowsAndMessaging::{SetClassLongPtrW, GCLP_HBRBACKGROUND};
