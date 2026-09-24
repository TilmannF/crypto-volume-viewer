#![allow(
    missing_docs,
    clippy::expect_used,
    reason = "job registry tests assert on registry behavior directly"
)]

//! Verifies `GuiState`'s extraction-job registry generates opaque,
//! non-derived job ids, and that cancelling a known job flips its
//! `CancellationToken` and removes it, while cancelling an unknown or
//! already-removed job id returns a typed not-found error instead of
//! panicking.

mod support;

use cryptovol_app::{open_volume, CancellationToken, OpenVolumeRequest};
use cryptovol_gui_lib::dto::error::JOB_NOT_FOUND;
use cryptovol_gui_lib::state::{GuiState, JobId, SessionId};
use secrecy::SecretString;

/// Registers a real synthetic session, since jobs can only be registered for
/// an open session.
fn insert_test_session(state: &GuiState, dir: &tempfile::TempDir) -> SessionId {
    let session = open_volume(OpenVolumeRequest {
        container_path: support::write_synthetic_container(dir, "jobs.hc"),
        password: SecretString::from(support::TEST_PASSWORD.to_string()),
        pim: None,
        kdf_hint: None,
    })
    .expect("open_volume should succeed with the correct password");
    state.insert_session(session)
}

#[test]
fn generated_job_ids_are_unique_even_for_the_same_session_and_source_path() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let state = GuiState::default();
    let session_id = insert_test_session(&state, &dir);

    let job_a = state
        .insert_job(session_id.clone(), CancellationToken::new())
        .expect("register job a");
    let job_b = state
        .insert_job(session_id, CancellationToken::new())
        .expect("register job b");

    assert_ne!(job_a.as_str(), job_b.as_str());
    assert!(!job_a.as_str().is_empty());
}

#[test]
fn cancelling_a_known_job_flips_its_token_and_removes_it() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let state = GuiState::default();
    let session_id = insert_test_session(&state, &dir);
    let token = CancellationToken::new();
    let job_id = state
        .insert_job(session_id, token.clone())
        .expect("register job");

    state
        .cancel_job(&job_id)
        .expect("cancelling a known job should succeed");

    assert!(token.is_cancelled());
    assert!(
        state.cancel_job(&job_id).is_err(),
        "the job should be removed from the registry once cancelled"
    );
}

#[test]
fn cancelling_an_unknown_job_returns_job_not_found_without_panicking() {
    let state = GuiState::default();
    let unknown = JobId::new();

    let err = state
        .cancel_job(&unknown)
        .expect_err("an unknown job id must error, not panic");
    assert_eq!(err.code, JOB_NOT_FOUND);
}
