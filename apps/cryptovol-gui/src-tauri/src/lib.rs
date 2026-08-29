//! Tauri desktop bootstrap for the `cryptovol` GUI.
//!
//! This crate is a thin adapter over `cryptovol_app`: it registers Tauri
//! commands, manages GUI-only state (open sessions, extraction jobs), and
//! forwards progress/cancellation events to the frontend. It must never
//! reimplement TC/VC, filesystem, or extraction logic itself -- that logic
//! lives in `cryptovol_app` and its lower-layer crates, shared with the CLI.
//!
//! Split into a lib (this crate) and a thin `main.rs` binary so integration
//! tests can exercise DTOs, error mapping, and the session/job registries
//! directly, and so the library and binary don't share a name on Windows.

#[cfg(all(feature = "e2e-webdriver", target_os = "macos"))]
mod app_nap;
pub mod commands;
pub mod dto;
pub mod error;
pub mod events;
pub mod state;

use state::GuiState;

/// Registers development/E2E-only plugins. Under the `e2e-webdriver` Cargo
/// feature this adds `tauri-plugin-wdio` (the execute/mocking API the E2E
/// harness's `browser.tauri.execute()` calls depend on) and
/// `tauri-plugin-wdio-webdriver` (the embedded WebDriver server) that the
/// local Tauri E2E harness (`apps/cryptovol-gui/e2e/`) drives against; that
/// feature is never enabled by the default build or by `tauri dev`/`tauri
/// build`, so this is a no-op in every normal and production build (see
/// `docs/gui-testing.md`).
#[cfg(feature = "e2e-webdriver")]
fn register_e2e_plugins<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder
        .plugin(tauri_plugin_wdio::init())
        .plugin(tauri_plugin_wdio_webdriver::init())
}

#[cfg(not(feature = "e2e-webdriver"))]
fn register_e2e_plugins<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder
}

/// Boots the Tauri application. Called by the `cryptovol-gui` binary's `main`.
pub fn run() {
    // Kept alive for the whole process (this binding lives across the
    // blocking `.run()` call below) so App Nap stays disabled for the
    // e2e-webdriver harness's entire run -- see app_nap.rs.
    #[cfg(all(feature = "e2e-webdriver", target_os = "macos"))]
    let _app_nap_guard = app_nap::disable();

    let builder =
        register_e2e_plugins(tauri::Builder::default().plugin(tauri_plugin_dialog::init()));
    if let Err(err) = builder
        .manage(GuiState::default())
        .invoke_handler(tauri::generate_handler![
            commands::container::inspect_container,
            commands::session::open_container,
            commands::session::list_dir,
            commands::session::stat,
            commands::session::close_session,
            commands::extraction::extract_file,
            commands::extraction::cancel_extract,
            commands::dialogs::select_container_file,
            commands::dialogs::select_extract_destination,
        ])
        .run(tauri::generate_context!())
    {
        eprintln!("cryptovol-gui failed to start: {err}");
        std::process::exit(1);
    }
}
