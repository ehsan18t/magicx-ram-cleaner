//! Native Win32 file dialogs (COMDLG32) for JSON files.

use std::ffi::OsString;
use std::mem::size_of;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;

use anyhow::{Result, bail};
use windows_sys::Win32::UI::Controls::Dialogs::{
    CommDlgExtendedError, GetOpenFileNameW, GetSaveFileNameW, OFN_FILEMUSTEXIST, OFN_HIDEREADONLY,
    OFN_NOCHANGEDIR, OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};

use super::wide::to_wide;

/// File type filter: pairs separated by single NULs; `to_wide` appends the
/// final NUL that makes the required double-NUL terminator.
const JSON_FILTER: &str = "JSON files (*.json)\0*.json\0All files (*.*)\0*.*\0";

/// Capacity of the file name buffer, in UTF-16 units: the longest path
/// Windows supports, so long-path folders work too.
const FILE_BUF_LEN: usize = 32_768;

/// Open a **Save File** dialog titled `title`, pre-filled with
/// `default_name` and filtered to `*.json`.
///
/// The dialog is modal to `owner` (the app's main window, or `0` for none):
/// the window is disabled while it is open and the dialog stays on top.
///
/// Returns the chosen path, or `Ok(None)` if the user cancels.
///
/// # Errors
///
/// Fails if the dialog itself fails (as opposed to being cancelled).
pub fn pick_save_json(owner: isize, title: &str, default_name: &str) -> Result<Option<PathBuf>> {
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
    ofn.hwndOwner = owner as _;
    ofn.lpstrFilter = filter.as_ptr();
    ofn.lpstrFile = file_buf.as_mut_ptr();
    ofn.nMaxFile = file_buf.len() as u32;
    ofn.lpstrTitle = title.as_ptr();
    ofn.lpstrDefExt = ext.as_ptr();
    ofn.Flags = OFN_OVERWRITEPROMPT | OFN_HIDEREADONLY | OFN_NOCHANGEDIR;

    // SAFETY: `ofn` satisfies the GetSaveFileNameW contract (see above).
    let chosen = unsafe { GetSaveFileNameW(&raw mut ofn) } != 0;
    dialog_result(chosen, &file_buf)
}

/// Open an **Open File** dialog titled `title`, filtered to `*.json`, modal
/// to `owner` (see [`pick_save_json`]).
///
/// Returns the chosen path, or `Ok(None)` if the user cancels.
///
/// # Errors
///
/// Fails if the dialog itself fails (as opposed to being cancelled).
pub fn pick_open_json(owner: isize, title: &str) -> Result<Option<PathBuf>> {
    let filter = to_wide(JSON_FILTER);
    let title = to_wide(title);
    let mut file_buf = vec![0u16; FILE_BUF_LEN];

    // SAFETY: As in `pick_save_json`: zero-initialised C struct whose
    // pointers reference local buffers that outlive the call.
    let mut ofn: OPENFILENAMEW = unsafe { std::mem::zeroed() };
    ofn.lStructSize = size_of::<OPENFILENAMEW>() as u32;
    ofn.hwndOwner = owner as _;
    ofn.lpstrFilter = filter.as_ptr();
    ofn.lpstrFile = file_buf.as_mut_ptr();
    ofn.nMaxFile = file_buf.len() as u32;
    ofn.lpstrTitle = title.as_ptr();
    ofn.Flags = OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_HIDEREADONLY | OFN_NOCHANGEDIR;

    // SAFETY: `ofn` satisfies the GetOpenFileNameW contract (see above).
    let chosen = unsafe { GetOpenFileNameW(&raw mut ofn) } != 0;
    dialog_result(chosen, &file_buf)
}

/// Turn a dialog's return value into a result. A dialog that returns
/// without a file was either cancelled (no extended error) or failed.
fn dialog_result(chosen: bool, file_buf: &[u16]) -> Result<Option<PathBuf>> {
    if chosen {
        return Ok(Some(buffer_to_path(file_buf)));
    }
    // SAFETY: Reads the common dialog error of the call that just returned.
    let error = unsafe { CommDlgExtendedError() };
    if error != 0 {
        bail!("the file dialog failed (error 0x{error:04X})");
    }
    Ok(None)
}

/// Convert a null-terminated UTF-16 buffer filled by a dialog into a path.
fn buffer_to_path(buf: &[u16]) -> PathBuf {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    PathBuf::from(OsString::from_wide(&buf[..end]))
}
