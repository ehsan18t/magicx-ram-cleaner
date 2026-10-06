//! Wall-clock time in the user's local time zone.

/// Get local date and time as `YYYY-MM-DD HH:MM:SS` string using Win32 `GetLocalTime`.
#[must_use]
pub fn local_now() -> String {
    use windows_sys::Win32::Foundation::SYSTEMTIME;
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    // SAFETY: SYSTEMTIME is a plain data struct; zeroed is a valid initial state.
    // GetLocalTime writes to the provided pointer and cannot fail.
    unsafe {
        let mut st: SYSTEMTIME = std::mem::zeroed();
        GetLocalTime(&raw mut st);
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            st.wYear, st.wMonth, st.wDay, st.wHour, st.wMinute, st.wSecond
        )
    }
}
