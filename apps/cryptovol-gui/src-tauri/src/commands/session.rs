//! Commands for opening a container and browsing/closing the resulting
//! session.

use crate::commands::run_blocking;
use crate::dto::error::GuiErrorDto;
use crate::dto::file_entry::FileEntryDto;
use crate::dto::session::{
    parse_kdf_hint, parse_pim, OpenContainerRequestDto, OpenContainerResponseDto, VolumeInfoDto,
};
use crate::state::{GuiState, SessionId};
use cryptovol_app::{open_volume, OpenVolumeRequest};
use secrecy::SecretString;
use std::path::PathBuf;
use tauri::AppHandle;

/// Opens a container with the given password/PIM/KDF hint and stores the
/// resulting session under a fresh opaque id.
///
/// # Errors
///
/// Returns a [`GuiErrorDto`] if the KDF hint or PIM is invalid, or if
/// `cryptovol_app::open_volume` fails (wrong password, unsupported format).
pub fn open_container_impl(
    state: &GuiState,
    request: OpenContainerRequestDto,
) -> Result<OpenContainerResponseDto, GuiErrorDto> {
    let kdf_hint = parse_kdf_hint(request.kdf_hint.as_deref())?;
    let pim = parse_pim(request.pim)?;

    let session = open_volume(OpenVolumeRequest {
        container_path: PathBuf::from(request.container_path),
        password: SecretString::from(request.password),
        pim,
        kdf_hint,
    })
    .map_err(GuiErrorDto::from)?;

    let volume_info = VolumeInfoDto::from(session.volume_info());
    let session_id = state.insert_session(session);

    Ok(OpenContainerResponseDto {
        session_id: session_id.as_str().to_string(),
        volume_info,
    })
}

/// Tauri command wrapper for [`open_container_impl`]. Runs on the blocking
/// thread pool: key derivation can take seconds.
#[tauri::command]
pub async fn open_container(
    app: AppHandle,
    request: OpenContainerRequestDto,
) -> Result<OpenContainerResponseDto, GuiErrorDto> {
    run_blocking(app, move |state| open_container_impl(state, request)).await
}

/// Lists the entries of the directory at `path` within the session for
/// `session_id`.
///
/// # Errors
///
/// Returns a [`GuiErrorDto`] with code `session_not_found` if `session_id`
/// is unknown, or another mapped [`cryptovol_app::AppError`] otherwise.
pub fn list_dir_impl(
    state: &GuiState,
    session_id: &str,
    path: &str,
) -> Result<Vec<FileEntryDto>, GuiErrorDto> {
    let id = SessionId::from(session_id);
    let entries = state
        .with_session(&id, |session| session.list_dir(path))
        .ok_or_else(session_not_found)?;

    entries
        .map(|entries| entries.into_iter().map(FileEntryDto::from).collect())
        .map_err(GuiErrorDto::from)
}

/// Tauri command wrapper for [`list_dir_impl`]. Runs on the blocking
/// thread pool.
#[tauri::command]
pub async fn list_dir(
    app: AppHandle,
    session_id: String,
    path: String,
) -> Result<Vec<FileEntryDto>, GuiErrorDto> {
    run_blocking(app, move |state| list_dir_impl(state, &session_id, &path)).await
}

/// Returns metadata for a single file or directory at `path` within the
/// session for `session_id`.
///
/// # Errors
///
/// Returns a [`GuiErrorDto`] with code `session_not_found` if `session_id`
/// is unknown, or another mapped [`cryptovol_app::AppError`] otherwise.
pub fn stat_impl(
    state: &GuiState,
    session_id: &str,
    path: &str,
) -> Result<FileEntryDto, GuiErrorDto> {
    let id = SessionId::from(session_id);
    let entry = state
        .with_session(&id, |session| session.stat(path))
        .ok_or_else(session_not_found)?;

    entry.map(FileEntryDto::from).map_err(GuiErrorDto::from)
}

/// Tauri command wrapper for [`stat_impl`]. Runs on the blocking thread
/// pool.
#[tauri::command]
pub async fn stat(
    app: AppHandle,
    session_id: String,
    path: String,
) -> Result<FileEntryDto, GuiErrorDto> {
    run_blocking(app, move |state| stat_impl(state, &session_id, &path)).await
}

/// Closes the session for `session_id`, cancelling any active extraction
/// jobs it owns first.
///
/// # Errors
///
/// Returns a [`GuiErrorDto`] with code `session_not_found` if `session_id`
/// is unknown.
pub fn close_session_impl(state: &GuiState, session_id: &str) -> Result<(), GuiErrorDto> {
    state.close_session(&SessionId::from(session_id))
}

/// Tauri command wrapper for [`close_session_impl`].
#[tauri::command]
pub fn close_session(state: tauri::State<GuiState>, session_id: String) -> Result<(), GuiErrorDto> {
    close_session_impl(&state, &session_id)
}

fn session_not_found() -> GuiErrorDto {
    GuiErrorDto::new(
        crate::dto::error::SESSION_NOT_FOUND,
        "no open session with that id",
    )
}
