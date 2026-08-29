#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::panic,
    reason = "regression guard test asserts against source text, not runtime behavior"
)]

//! Regression guard: the password-carrying `OpenContainerRequestDto` must
//! never derive `Debug`/`Clone`/`Copy` (so it can't be accidentally logged
//! or duplicated into long-lived state), and `GuiState` (src-tauri/src/state.rs)
//! must never reference passwords at all.
//!
//! This also does a compile-time smoke check that `OpenContainerRequestDto`
//! exists with the expected shape, since a source-text scan alone can't
//! catch a derive added via a type alias or macro.

use cryptovol_gui_lib::dto::session::OpenContainerRequestDto;
use std::path::{Path, PathBuf};

fn src_tauri_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

#[test]
fn open_container_request_dto_can_be_constructed_from_deserialize_only_fields() {
    // Exercised via serde, not a struct literal, so this also proves no
    // additional non-Deserialize-friendly fields were added.
    let json = r#"{"containerPath":"/tmp/c.hc","password":"secret","pim":null,"kdfHint":null}"#;
    let request: OpenContainerRequestDto =
        serde_json::from_str(json).expect("OpenContainerRequestDto should deserialize");
    assert_eq!(request.container_path, "/tmp/c.hc");
    assert_eq!(request.password, "secret");
}

#[test]
fn open_container_request_dto_does_not_derive_debug_or_clone() {
    let dto_session_path = src_tauri_dir().join("dto").join("session.rs");
    let contents = std::fs::read_to_string(&dto_session_path)
        .unwrap_or_else(|e| panic!("{} should be readable: {e}", dto_session_path.display()));

    let struct_marker = "pub struct OpenContainerRequestDto";
    let struct_index = contents
        .find(struct_marker)
        .expect("OpenContainerRequestDto struct definition should exist in dto/session.rs");

    // The nearest preceding #[derive(...)] line is this struct's derive.
    let preceding = &contents[..struct_index];
    let derive_line = preceding
        .rfind("#[derive(")
        .map(|start| {
            let end = preceding[start..]
                .find(')')
                .map(|rel| start + rel)
                .unwrap_or(preceding.len());
            &preceding[start..=end]
        })
        .expect("OpenContainerRequestDto should have a derive attribute");

    assert!(
        !derive_line.contains("Debug"),
        "OpenContainerRequestDto must not derive Debug: {derive_line}"
    );
    assert!(
        !derive_line.contains("Clone"),
        "OpenContainerRequestDto must not derive Clone: {derive_line}"
    );
    assert!(
        !derive_line.contains("Copy"),
        "OpenContainerRequestDto must not derive Copy: {derive_line}"
    );
}

#[test]
fn gui_state_source_never_mentions_password() {
    let state_path = src_tauri_dir().join("state.rs");
    let contents = std::fs::read_to_string(&state_path)
        .unwrap_or_else(|e| panic!("{} should be readable: {e}", state_path.display()));

    assert!(
        !contents.to_lowercase().contains("password"),
        "src-tauri/src/state.rs must never reference passwords"
    );
}
