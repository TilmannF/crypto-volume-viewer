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
