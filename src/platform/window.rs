//! Main-window helpers: lookup, minimise state, cloaking to the tray, and
//! dark/light theming of the title bar and native menus.

/// Return the `HWND` of a top-level window titled `title` that belongs to a
/// process running this same executable, or `0` if there is none.
///
/// Matching on the title alone is not enough: any program, or an Explorer
/// folder window named after the app, can show the same title.
#[must_use]
pub fn find_app_window(title: &str) -> isize {
    use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowExW, GetWindowThreadProcessId};

    let Some(own_exe) = std::env::current_exe().ok().map(|p| resolved(&p)) else {
        return 0;
    };
    let wide = super::wide::to_wide(title);
    let mut previous = std::ptr::null_mut();
    loop {
        // SAFETY: `wide` is a valid null-terminated wide string; `previous`
        // is null or a window returned by the previous iteration. Iterates
        // top-level windows with this exact title.
        let hwnd = unsafe {
            FindWindowExW(
                std::ptr::null_mut(),
                previous,
                std::ptr::null(),
                wide.as_ptr(),
            )
        };
        if hwnd.is_null() {
            return 0;
        }
        let mut pid = 0u32;
        // SAFETY: `hwnd` is a window handle and `pid` a valid out pointer.
        unsafe { GetWindowThreadProcessId(hwnd, &raw mut pid) };
        let same_exe = super::process::image_path(pid)
            .is_some_and(|path| resolved(&path).eq_ignore_ascii_case(&own_exe));
        if same_exe {
            return hwnd as isize;
        }
        previous = hwnd;
    }
}

/// `path` with junctions, symbolic links and 8.3 short names resolved, so two
/// spellings of the same executable compare equal. Falls back to the path as
/// given when it cannot be resolved.
fn resolved(path: &std::path::Path) -> std::ffi::OsString {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .into_os_string()
}

/// Check whether the application window is currently minimized (iconic).
///
/// Uses the Win32 `IsIconic` API for reliable minimized-state detection.
/// Unlike `egui::ViewportInfo::minimized` (which may return `None` when
/// the windowing back-end does not report it), this always returns a
/// definitive answer on Windows.
///
/// Returns `false` when `hwnd` is `0`.
#[must_use]
pub fn is_window_minimized(hwnd: isize) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::IsIconic;

    if hwnd == 0 {
        return false;
    }

    // SAFETY: IsIconic is a read-only state check on a valid owned window.
    unsafe { IsIconic(hwnd as *mut _) != 0 }
}

/// Immediately hide the window by calling `ShowWindow(SW_HIDE)` directly.
///
/// Synchronously clears the `WS_VISIBLE` flag so the window disappears on
/// the current frame.  Prefer [`cloak_window`] for the minimize-to-tray
/// flow; this function is used internally by `cloak_window`.
///
/// Does nothing when `hwnd` is `0`.
pub fn hide_window(hwnd: isize) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SW_HIDE, ShowWindow};

    if hwnd == 0 {
        return;
    }

    // SAFETY: `hwnd` is our own main window, valid for the lifetime of the
    // process.  `SW_HIDE` is a standard, non-destructive window-state change.
    unsafe { ShowWindow(hwnd as *mut _, SW_HIDE) };
}

/// Cloak the window: make it invisible to the user while keeping
/// `WS_VISIBLE` set so eframe's event loop stays in `ControlFlow::Wait`.
///
/// The approach avoids the eframe/winit CPU bug (`emilk/egui#7776`) where
/// `ControlFlow::Poll` is used for invisible windows.  Steps:
///
/// 1. `SW_HIDE` - instantly clear `WS_VISIBLE` so no animations play.
/// 2. Add `WS_EX_TOOLWINDOW` / remove `WS_EX_APPWINDOW` - hides the
///    window from the taskbar and Alt-Tab.
/// 3. `SW_SHOWMINNOACTIVE` - restores `WS_VISIBLE` in the iconic
///    (minimized) state without stealing focus.  On modern Windows,
///    a minimized tool window has no on-screen representation.
///
/// After this call: `IsWindowVisible` → `true`, `IsIconic` → `true`,
/// no taskbar button, no Alt-Tab entry, zero visual presence.
///
/// Does nothing when `hwnd` is `0`.
pub fn cloak_window(hwnd: isize) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetWindowLongPtrW, SW_SHOWMINNOACTIVE, SetWindowLongPtrW, ShowWindow,
        WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
    };

    if hwnd == 0 {
        return;
    }

    // SAFETY: All calls operate on our own main-window handle, which is
    // valid for the process lifetime.  The sequence is
    // hide → style change → show-minimized, each a standard Win32 call.
    unsafe {
        hide_window(hwnd);
        let ex = GetWindowLongPtrW(hwnd as *mut _, GWL_EXSTYLE);
        #[allow(clippy::cast_possible_wrap)]
        let new_ex = (ex | WS_EX_TOOLWINDOW as isize) & !(WS_EX_APPWINDOW as isize);
        SetWindowLongPtrW(hwnd as *mut _, GWL_EXSTYLE, new_ex);
        ShowWindow(hwnd as *mut _, SW_SHOWMINNOACTIVE);
    }
}

/// Reverse [`cloak_window`]: restore the window to its pre-minimize state
/// with a normal taskbar button and Alt-Tab entry.
///
/// Steps:
///
/// 1. Remove `WS_EX_TOOLWINDOW` / add `WS_EX_APPWINDOW` - taskbar and
///    Alt-Tab presence restored.
/// 2. [`bring_to_front`] - un-minimizes (keeping a maximized window
///    maximized) and brings the window to the front.
///
/// Does nothing when `hwnd` is `0`.
pub fn uncloak_window(hwnd: isize) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetWindowLongPtrW, SetWindowLongPtrW, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
    };

    if hwnd == 0 {
        return;
    }

    // SAFETY: Both calls operate on our own main-window handle, which is
    // valid for the process lifetime, and only change its extended style.
    unsafe {
        let ex = GetWindowLongPtrW(hwnd as *mut _, GWL_EXSTYLE);
        #[allow(clippy::cast_possible_wrap)]
        let new_ex = (ex & !(WS_EX_TOOLWINDOW as isize)) | WS_EX_APPWINDOW as isize;
        SetWindowLongPtrW(hwnd as *mut _, GWL_EXSTYLE, new_ex);
    }
    bring_to_front(hwnd);
}

/// Show `hwnd` and bring it to the front, waking its event loop.
///
/// A minimized window is restored (to maximized, if it was maximized before
/// minimizing). A window that is already showing keeps its size: `SW_RESTORE`
/// on a maximized window would shrink it back to its normal size.
///
/// Does nothing when `hwnd` is `0`.
pub fn bring_to_front(hwnd: isize) {
    use windows_sys::Win32::Graphics::Gdi::InvalidateRect;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        IsIconic, SW_RESTORE, SW_SHOW, SetForegroundWindow, ShowWindow,
    };

    if hwnd == 0 {
        return;
    }
    let hwnd = hwnd as windows_sys::Win32::Foundation::HWND;
    // SAFETY: `hwnd` is a top-level window handle; these calls only change
    // its show state, z-order and paint state. Invalidating the client area
    // queues a WM_PAINT, which wakes the owning event loop so it notices the
    // change without waiting for its next scheduled repaint.
    unsafe {
        let show = if IsIconic(hwnd) != 0 {
            SW_RESTORE
        } else {
            SW_SHOW
        };
        ShowWindow(hwnd, show);
        SetForegroundWindow(hwnd);
        InvalidateRect(hwnd, std::ptr::null(), 0);
    }
}

/// Set the window title bar to dark or light mode independently of the OS theme.
///
/// Uses [`DwmSetWindowAttribute`] with `DWMWA_USE_IMMERSIVE_DARK_MODE`
/// (value 20, stable since Windows 10 build 18985 / 19H2).  This ensures the
/// title bar matches the in-app theme rather than inheriting the OS-level
/// dark/light preference.
///
/// Does nothing when `hwnd` is `0` (no window found) or on failure.
///
/// [`DwmSetWindowAttribute`]: https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/nf-dwmapi-dwmsetwindowattribute
pub fn set_title_bar_dark_mode(hwnd: isize, dark: bool) {
    use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;

    /// `DWMWA_USE_IMMERSIVE_DARK_MODE` attribute constant.
    /// Stable since Windows 10 Build 18985 (19H2+).
    const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;

    if hwnd == 0 {
        return;
    }

    let value: i32 = i32::from(dark);

    // SAFETY: DwmSetWindowAttribute is a documented Win32 DWM call.
    // We pass a valid HWND, the attribute constant, a pointer to a
    // BOOL-valued i32, and its byte size (4).  The isize→HWND cast
    // is the reverse of find_app_window's HWND→isize return convention.
    let _ = unsafe {
        DwmSetWindowAttribute(
            hwnd as windows_sys::Win32::Foundation::HWND,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            std::ptr::from_ref(&value).cast(),
            std::mem::size_of::<i32>() as u32,
        )
    };
}

/// Paint the window's title bar in `rgb`, so it blends with the app's
/// background instead of using the system caption colour.
///
/// Uses `DWMWA_CAPTION_COLOR`, which Windows 11 (build 22000+) supports.
/// Earlier builds reject the attribute and keep their normal title bar, which
/// is the intended fallback. Does nothing when `hwnd` is `0`.
pub fn set_caption_color(hwnd: isize, rgb: [u8; 3]) {
    use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;

    /// `DWMWA_CAPTION_COLOR` attribute constant (Windows 11 build 22000+).
    const DWMWA_CAPTION_COLOR: u32 = 35;

    if hwnd == 0 {
        return;
    }

    // COLORREF layout: 0x00BBGGRR.
    let colorref = u32::from(rgb[0]) | (u32::from(rgb[1]) << 8) | (u32::from(rgb[2]) << 16);

    // SAFETY: Documented DWM call with a valid HWND, the attribute constant,
    // a pointer to a COLORREF-sized u32 and its byte size.
    let _ = unsafe {
        DwmSetWindowAttribute(
            hwnd as windows_sys::Win32::Foundation::HWND,
            DWMWA_CAPTION_COLOR,
            std::ptr::from_ref(&colorref).cast(),
            std::mem::size_of::<u32>() as u32,
        )
    };
}

/// Force the process's native Win32 menus to render in dark or light theme.
///
/// Uses the undocumented but stable `SetPreferredAppMode` (ordinal 135) and
/// `FlushMenuThemes` (ordinal 136) exports from `uxtheme.dll`.  These ordinals
/// have been stable since Windows 10 1903 and are relied on by production
/// apps such as Firefox, VS Code, and Notepad++.
///
/// Does nothing silently if the DLL cannot be loaded or the ordinals are
/// missing (e.g. on older Windows builds).
pub fn set_process_dark_mode(dark: bool) {
    /// `SetPreferredAppMode` argument: force dark context menus.
    const FORCE_DARK: i32 = 2;
    /// `SetPreferredAppMode` argument: force light context menus.
    const FORCE_LIGHT: i32 = 3;

    let Some(api) = menu_theme_api() else {
        return;
    };
    if let Some(set_mode) = api.set_preferred_app_mode {
        // SAFETY: Ordinal 135 is `fn(i32) -> i32` (stdcall), stable across
        // all Windows 10/11 builds since 1903.
        unsafe {
            set_mode(if dark { FORCE_DARK } else { FORCE_LIGHT });
        }
    }
    if let Some(flush) = api.flush_menu_themes {
        // SAFETY: Ordinal 136 is `fn()`, stable since 1903.
        unsafe {
            flush();
        }
    }
}

/// The two undocumented uxtheme exports used by [`set_process_dark_mode`].
struct MenuThemeApi {
    /// Ordinal 135, `SetPreferredAppMode(mode: i32) -> i32`.
    set_preferred_app_mode: Option<unsafe extern "system" fn(i32) -> i32>,
    /// Ordinal 136, `FlushMenuThemes()`: makes menus re-read their theme.
    flush_menu_themes: Option<unsafe extern "system" fn()>,
}

/// Look the uxtheme exports up once per process. `None` if uxtheme cannot
/// be loaded from System32.
#[expect(
    clippy::as_conversions,
    reason = "MAKEINTRESOURCE pattern: ordinal as *const u8 is the Win32 convention"
)]
fn menu_theme_api() -> Option<&'static MenuThemeApi> {
    use std::sync::OnceLock;
    use windows_sys::Win32::System::LibraryLoader::GetProcAddress;

    static API: OnceLock<Option<MenuThemeApi>> = OnceLock::new();
    API.get_or_init(|| {
        let module = super::loader::system_module("uxtheme.dll")?;
        // SAFETY: GetProcAddress with a MAKEINTRESOURCE-style ordinal (low
        // 16 bits = ordinal, high bits zero) is the documented way to look
        // an export up by number. Transmuting between function pointer types
        // of equal size is sound; the signatures are given above.
        unsafe {
            Some(MenuThemeApi {
                set_preferred_app_mode: GetProcAddress(module, 135_usize as *const u8)
                    .map(|f| std::mem::transmute(f)),
                flush_menu_themes: GetProcAddress(module, 136_usize as *const u8)
                    .map(|f| std::mem::transmute(f)),
            })
        }
    })
    .as_ref()
}
