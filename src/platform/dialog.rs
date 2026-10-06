//! Native Win32 file dialogs (COMDLG32) for JSON files.

use std::ffi::OsString;
use std::mem::size_of;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;

use windows_sys::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, GetSaveFileNameW, OFN_FILEMUSTEXIST, OFN_HIDEREADONLY, OFN_NOCHANGEDIR,
    OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};

use super::wide::to_wide;

/// File type filter: pairs separated by single NULs; `to_wide` appends the
/// final NUL that makes the required double-NUL terminator.
const JSON_FILTER: &str = "JSON files (*.json)\0*.json\0All files (*.*)\0*.*\0";

/// Capacity of the file name buffer, in UTF-16 units.
const FILE_BUF_LEN: usize = 512;

/// Open a **Save File** dialog titled `title`, pre-filled with
/// `default_name` and filtered to `*.json`.
///
/// Returns the chosen path, or [`None`] if the user cancels.
#[must_use]
pub fn pick_save_json(title: &str, default_name: &str) -> Option<PathBuf> {
    let filter = to_wide(JSON_FILTER);
    let title = to_wide(title);
    let ext = to_wide("json");

    // Pre-populate the file name buffer with the suggested default.
    let mut file_buf = vec![0u16; FILE_BUF_LEN];
    for (slot, unit) in file_buf
        .iter_mut()
        .zip(default_name.encode_utf16().take(260))
    {
        *slot = unit;
    }

    // SAFETY: OPENFILENAMEW is a plain C struct; zero-initialisation is the
    // documented pattern. Every pointer assigned below references a local
    // buffer that outlives the call.
    let mut ofn: OPENFILENAMEW = unsafe { std::mem::zeroed() };
    ofn.lStructSize = size_of::<OPENFILENAMEW>() as u32;
    ofn.lpstrFilter = filter.as_ptr();
    ofn.lpstrFile = file_buf.as_mut_ptr();
    ofn.nMaxFile = file_buf.len() as u32;
    ofn.lpstrTitle = title.as_ptr();
    ofn.lpstrDefExt = ext.as_ptr();
    ofn.Flags = OFN_OVERWRITEPROMPT | OFN_HIDEREADONLY | OFN_NOCHANGEDIR;

    // SAFETY: `ofn` satisfies the GetSaveFileNameW contract (see above).
    if unsafe { GetSaveFileNameW(&raw mut ofn) } == 0 {
        return None;
    }
    Some(buffer_to_path(&file_buf))
}

/// Open an **Open File** dialog titled `title`, filtered to `*.json`.
///
/// Returns the chosen path, or [`None`] if the user cancels.
#[must_use]
pub fn pick_open_json(title: &str) -> Option<PathBuf> {
    let filter = to_wide(JSON_FILTER);
    let title = to_wide(title);
    let mut file_buf = vec![0u16; FILE_BUF_LEN];

    // SAFETY: As in `pick_save_json`: zero-initialised C struct whose
    // pointers reference local buffers that outlive the call.
    let mut ofn: OPENFILENAMEW = unsafe { std::mem::zeroed() };
    ofn.lStructSize = size_of::<OPENFILENAMEW>() as u32;
    ofn.lpstrFilter = filter.as_ptr();
    ofn.lpstrFile = file_buf.as_mut_ptr();
    ofn.nMaxFile = file_buf.len() as u32;
    ofn.lpstrTitle = title.as_ptr();
    ofn.Flags = OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_HIDEREADONLY | OFN_NOCHANGEDIR;

    // SAFETY: `ofn` satisfies the GetOpenFileNameW contract (see above).
    if unsafe { GetOpenFileNameW(&raw mut ofn) } == 0 {
        return None;
    }
    Some(buffer_to_path(&file_buf))
}

/// Convert a null-terminated UTF-16 buffer filled by a dialog into a path.
fn buffer_to_path(buf: &[u16]) -> PathBuf {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    PathBuf::from(OsString::from_wide(&buf[..end]))
}
