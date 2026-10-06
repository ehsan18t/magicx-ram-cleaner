//! Conversions between Rust strings and the null-terminated UTF-16 strings
//! Win32 APIs use.

/// Encode a Rust `&str` as a null-terminated UTF-16 `Vec<u16>`.
///
/// Shared utility used by registry operations (`context_menu`) and privilege
/// management (`privilege`) to convert Rust strings for Win32 wide-string APIs.
#[must_use]
pub fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Compile-time conversion of an ASCII byte literal to a null-terminated
/// UTF-16 array. `N` must equal `src.len() + 1` (for the null terminator).
#[must_use]
pub const fn wide_literal<const N: usize>(src: &[u8]) -> [u16; N] {
    assert!(src.len() + 1 == N, "N must be src.len() + 1");
    let mut buf = [0u16; N];
    let mut i = 0;
    while i < src.len() {
        buf[i] = src[i] as u16;
        i += 1;
    }
    buf
}

/// Encode a Rust `&str` as null-terminated UTF-16 into a fixed-size buffer.
///
/// Silently truncates if `s` is longer than `buf.len() - 1`, never splitting
/// a surrogate pair (a character outside the Basic Multilingual Plane, such
/// as an emoji). An empty `buf` is left untouched.
pub fn write_wide_into(buf: &mut [u16], s: &str) {
    let Some(capacity) = buf.len().checked_sub(1) else {
        return;
    };
    let mut len = 0;
    for c in s.encode_utf16() {
        if len == capacity {
            // Cut before a high surrogate whose low half did not fit.
            if len > 0 && (0xD800..=0xDBFF).contains(&buf[len - 1]) {
                len -= 1;
            }
            break;
        }
        buf[len] = c;
        len += 1;
    }
    buf[len] = 0;
}

/// Extract a UTF-8 process name from a null-terminated UTF-16 `szExeFile` buffer.
#[must_use]
pub fn extract_exe_name(sz_exe_file: &[u16]) -> String {
    let len = sz_exe_file
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(sz_exe_file.len());
    String::from_utf16_lossy(&sz_exe_file[..len])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_exe_name_from_utf16() {
        // Simulate a null-terminated UTF-16 "chrome.exe"
        let name: Vec<u16> = "chrome.exe\0\0\0\0".encode_utf16().collect();
        assert_eq!(extract_exe_name(&name), "chrome.exe");
    }

    #[test]
    fn extract_exe_name_no_null() {
        // If the buffer has no null terminator, use the full slice
        let name: Vec<u16> = "svchost.exe".encode_utf16().collect();
        assert_eq!(extract_exe_name(&name), "svchost.exe");
    }

    #[test]
    fn extract_exe_name_empty() {
        let name: Vec<u16> = vec![0];
        assert_eq!(extract_exe_name(&name), "");
    }

    #[test]
    fn to_wide_appends_terminator() {
        assert_eq!(to_wide("ab"), vec![u16::from(b'a'), u16::from(b'b'), 0]);
    }

    #[test]
    fn wide_literal_matches_to_wide() {
        const LIT: [u16; 7] = wide_literal::<7>(b"STATIC");
        assert_eq!(LIT.to_vec(), to_wide("STATIC"));
    }

    #[test]
    fn write_wide_into_truncates_and_terminates() {
        let mut buf = [0xFFFFu16; 4];
        write_wide_into(&mut buf, "abcdef");
        assert_eq!(buf, [u16::from(b'a'), u16::from(b'b'), u16::from(b'c'), 0]);
    }

    #[test]
    fn write_wide_into_ignores_an_empty_buffer() {
        write_wide_into(&mut [], "abc");
    }

    #[test]
    fn write_wide_into_never_splits_a_surrogate_pair() {
        // "ab" plus an emoji (two UTF-16 units) needs 5 units with the
        // terminator; with room for 4, the whole emoji is dropped.
        let mut buf = [0xFFFFu16; 4];
        write_wide_into(&mut buf, "ab\u{1F600}");
        assert_eq!(buf[..3], [u16::from(b'a'), u16::from(b'b'), 0]);
    }

    #[test]
    fn write_wide_into_keeps_a_pair_that_fits() {
        let mut buf = [0xFFFFu16; 4];
        write_wide_into(&mut buf, "a\u{1F600}");
        assert_eq!(String::from_utf16(&buf[..3]).unwrap(), "a\u{1F600}");
        assert_eq!(buf[3], 0);
    }
}
