//! Native file/save dialog commands, backed by the official
//! `tauri-plugin-dialog` plugin.
//!
//! These are the only commands allowed to touch host filesystem browsing;
//! they never read/write file contents themselves, only return a path
//! string for the frontend to pass back into `open_container`/
//! `extract_file`. Manual path text entry remains a fully functional
//! fallback in the frontend if a dialog is dismissed or unavailable.

use crate::dto::error::GuiErrorDto;
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

/// Opens a native file picker for selecting a container file.
///
/// Declared `async` so Tauri dispatches this command off the main/UI
/// thread before calling into `blocking_pick_file()`. Calling
/// `blocking_pick_file()` from a non-async command deadlocks: it blocks
/// the calling thread waiting for the dialog to close, but the dialog
/// itself must run on the main thread, which non-async commands are
/// dispatched on -- see `tauri-plugin-dialog`'s own doc comment on
/// `blocking_pick_file`.
///
/// Returns `Ok(None)` if the user cancels the dialog, rather than an error.
#[tauri::command]
pub async fn select_container_file(app: AppHandle) -> Result<Option<String>, GuiErrorDto> {
    let path = app.dialog().file().blocking_pick_file();
    Ok(path.map(|p| p.simplified().to_string()))
}

/// Opens a native save-file picker for choosing an extraction destination,
/// optionally pre-filled with `default_file_name`.
///
/// See [`select_container_file`] for why this must be `async`.
///
/// Returns `Ok(None)` if the user cancels the dialog, rather than an error.
#[tauri::command]
pub async fn select_extract_destination(
    app: AppHandle,
    default_file_name: Option<String>,
) -> Result<Option<String>, GuiErrorDto> {
    let mut dialog = app.dialog().file();
    if let Some(name) = default_file_name {
        dialog = dialog.set_file_name(name);
    }
    let path = dialog.blocking_save_file();
    Ok(path.map(|p| p.simplified().to_string()))
}
