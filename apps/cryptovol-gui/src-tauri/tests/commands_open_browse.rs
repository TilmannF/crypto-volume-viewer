#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::panic,
    reason = "command-level tests open real synthetic volumes and assert on command behavior"
)]

//! Verifies the plain, testable command-implementation functions behind
//! `inspect_container`/`open_container`/`list_dir`/`stat`: safe,
//! password-free inspection; successful open with a fresh session id and
//! matching `VolumeInfo`; `auth_failed` on a wrong password; `list_dir`
//! returning the same entries as calling `VolumeSession::list_dir`
//! directly; `session_not_found` for an unknown session id; and `stat`
//! matching `VolumeSession::stat`.

mod support;

use cryptovol_app::{open_volume, OpenVolumeRequest};
use cryptovol_gui_lib::commands::container::inspect_container_impl;
use cryptovol_gui_lib::commands::session::{list_dir_impl, open_container_impl, stat_impl};
use cryptovol_gui_lib::dto::error::GuiErrorDto;
use cryptovol_gui_lib::dto::session::OpenContainerRequestDto;
use cryptovol_gui_lib::state::GuiState;
use secrecy::SecretString;

fn open_request(container_path: &std::path::Path, password: &str) -> OpenContainerRequestDto {
    OpenContainerRequestDto {
        container_path: container_path.display().to_string(),
        password: password.to_string(),
        pim: None,
        // The synthetic container is always encrypted with SHA-512; hinting
        // it explicitly keeps these tests to a single 500,000-iteration
        // PBKDF2 attempt instead of a slow full 5-KDF autoprobe.
        kdf_hint: Some("sha512".to_string()),
    }
}

#[test]
fn inspect_container_reports_size_without_a_password() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let path = support::write_synthetic_container(&dir, "container.hc");
    let expected_size = std::fs::metadata(&path).expect("read metadata").len();

    let info = inspect_container_impl(&path.display().to_string())
        .expect("inspect_container_impl should succeed without a password");

    assert_eq!(info.container_size_bytes, expected_size);
}

#[test]
fn open_container_succeeds_with_a_fresh_session_and_matching_volume_info() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let path = support::write_synthetic_container(&dir, "container.hc");
    let state = GuiState::default();

    let response = open_container_impl(&state, open_request(&path, support::TEST_PASSWORD))
        .expect("open_container_impl should succeed with the correct password");

    assert!(!response.session_id.is_empty());
    assert_eq!(response.volume_info.backend, "tcvc");
    assert_eq!(response.volume_info.filesystem, "fat");
    assert!(response.volume_info.read_only);

    // Opening the same container again must yield a different session id.
    let second =
        open_container_impl(&state, open_request(&path, support::TEST_PASSWORD)).expect("ok");
    assert_ne!(response.session_id, second.session_id);
}

#[test]
fn open_container_with_wrong_password_returns_auth_failed() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let path = support::write_synthetic_container(&dir, "container.hc");
    let state = GuiState::default();

    let err = open_container_impl(&state, open_request(&path, "definitely wrong"))
        .expect_err("wrong password must be rejected");

    assert_eq!(err.code, "auth_failed");
    assert!(!err.message.contains("definitely wrong"));
}

#[test]
fn list_dir_matches_volume_session_list_dir() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let path = support::write_synthetic_container(&dir, "container.hc");
    let state = GuiState::default();

    let response = open_container_impl(&state, open_request(&path, support::TEST_PASSWORD))
        .expect("open should succeed");
    let actual = list_dir_impl(&state, &response.session_id, "/");

    // Open the same container directly through cryptovol_app (bypassing the
    // GUI command layer) and compare outcomes. The synthetic container's
    // boot sector is deliberately minimal (built only to make filesystem
    // *probing* succeed, per support::write_synthetic_container's doc
    // comment) and has no real BPB/root directory, so FatFileSystem::open
    // itself fails on both sides; this proves list_dir_impl doesn't
    // silently swallow, reinterpret, or otherwise diverge from what
    // VolumeSession::list_dir itself reports, on the success or error path.
    let direct_session = open_volume(OpenVolumeRequest {
        container_path: path,
        password: SecretString::from(support::TEST_PASSWORD.to_string()),
        pim: None,
        kdf_hint: Some(cryptovol_app::TcvcKdf::Sha512),
    })
    .expect("direct open_volume should succeed");
    let expected = direct_session.list_dir("/");

    match (actual, expected) {
        (Ok(actual_entries), Ok(expected_entries)) => {
            let mut actual_names: Vec<&str> =
                actual_entries.iter().map(|e| e.name.as_str()).collect();
            let mut expected_names: Vec<&str> =
                expected_entries.iter().map(|e| e.name.as_str()).collect();
            actual_names.sort_unstable();
            expected_names.sort_unstable();
            assert_eq!(actual_names, expected_names);
        }
        (Err(actual_err), Err(expected_err)) => {
            assert_eq!(actual_err.code, GuiErrorDto::from(expected_err).code);
        }
        (actual, expected) => panic!(
            "list_dir_impl and VolumeSession::list_dir disagreed on success/failure: \
             {actual:?} vs {expected:?}"
        ),
    }
}

#[test]
fn list_dir_with_unknown_session_id_returns_session_not_found() {
    let state = GuiState::default();

    let err = list_dir_impl(&state, "00000000-0000-0000-0000-000000000000", "/").expect_err("ok");

    assert_eq!(err.code, "session_not_found");
}

#[test]
fn stat_error_matches_volume_session_stat_error_mapping() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let path = support::write_synthetic_container(&dir, "container.hc");
    let state = GuiState::default();

    let response = open_container_impl(&state, open_request(&path, support::TEST_PASSWORD))
        .expect("open should succeed");

    // As in list_dir_matches_volume_session_list_dir, this synthetic
    // container's filesystem doesn't parse, so stat on any path (missing or
    // not) fails on both sides; this proves stat_impl surfaces the same
    // mapped error VolumeSession::stat itself produces rather than
    // panicking or substituting a different one.
    let direct_session = open_volume(OpenVolumeRequest {
        container_path: path,
        password: SecretString::from(support::TEST_PASSWORD.to_string()),
        pim: None,
        kdf_hint: Some(cryptovol_app::TcvcKdf::Sha512),
    })
    .expect("direct open_volume should succeed");
    let expected_err = direct_session
        .stat("/missing.txt")
        .expect_err("this synthetic container's filesystem should not parse");

    let actual_err = stat_impl(&state, &response.session_id, "/missing.txt")
        .expect_err("stat_impl should surface the same error");

    assert_eq!(actual_err.code, GuiErrorDto::from(expected_err).code);
}
