//! `#[tauri::command]` functions exposing `cryptovol_app` to the frontend.
//!
//! Commands stay thin adapters: deserialize a DTO, look up session/job
//! state, call `cryptovol_app`, map the result to a DTO, and return a
//! sanitized `GuiErrorDto` (see `crate::dto::error`) on failure. Each
//! `#[tauri::command]` function here is a thin wrapper around a plain,
//! independently testable `*_impl` function of the same name in the
//! matching submodule, so command logic can be exercised in tests without a
//! real Tauri runtime.

pub mod container;
pub mod dialogs;
pub mod extraction;
pub mod session;

use crate::dto::error::{GuiErrorDto, INTERNAL_ERROR};
use crate::state::GuiState;
use tauri::{AppHandle, Manager};

/// Runs a blocking command body against the managed [`GuiState`] on Tauri's
/// blocking thread pool, so neither the main (UI) thread nor an async
/// executor thread waits on key derivation or filesystem reads.
async fn run_blocking<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&GuiState) -> Result<T, GuiErrorDto> + Send + 'static,
) -> Result<T, GuiErrorDto> {
    tauri::async_runtime::spawn_blocking(move || f(&app.state::<GuiState>()))
        .await
        .map_err(|_| GuiErrorDto::new(INTERNAL_ERROR, "background task failed"))?
}
