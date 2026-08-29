#![allow(
    missing_docs,
    clippy::expect_used,
    reason = "session registry tests open real synthetic volumes and assert on registry behavior"
)]

//! Verifies `GuiState`'s session registry generates opaque, non-derived
//! session ids, tolerates unknown-id lookups without panicking, and that
//! closing a session with an active extraction job cancels the job before
//! removing the session (rather than rejecting the close).

mod support;

use cryptovol_app::{open_volume, OpenVolumeRequest, VolumeSession};
use cryptovol_gui_lib::state::{GuiState, SessionId};
use secrecy::SecretString;

fn open_test_session(dir: &tempfile::TempDir, name: &str) -> VolumeSession {
    let path = support::write_synthetic_container(dir, name);
    open_volume(OpenVolumeRequest {
        container_path: path,
        password: SecretString::from(support::TEST_PASSWORD.to_string()),
        pim: None,
        kdf_hint: None,
    })
    .expect("open_volume should succeed with the correct password")
}

#[test]
fn generated_session_ids_are_unique_even_for_identical_inputs() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let state = GuiState::default();

    // Same container bytes, same password, same PIM/KDF hint (both default):
    // the id must still differ, since it must never be derived from these.
    let session_a = open_test_session(&dir, "a.hc");
    let session_b = open_test_session(&dir, "a.hc");

    let id_a = state.insert_session(session_a);
    let id_b = state.insert_session(session_b);

    assert_ne!(id_a.as_str(), id_b.as_str());
    assert!(!id_a.as_str().is_empty());
}

#[test]
fn unknown_session_id_lookup_returns_none_without_panicking() {
    let state = GuiState::default();
    let unknown = SessionId::new();

    assert!(state.with_session(&unknown, |_| ()).is_none());
}

#[test]
fn close_session_on_unknown_id_returns_an_error() {
    let state = GuiState::default();
    let unknown = SessionId::new();

    assert!(state.close_session(&unknown).is_err());
}

#[test]
fn close_session_cancels_active_jobs_then_removes_the_session() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let state = GuiState::default();

    let session = open_test_session(&dir, "c.hc");
    let session_id = state.insert_session(session);

    let token = cryptovol_app::CancellationToken::new();
    let job_id = state.insert_job(session_id.clone(), token.clone());

    state
        .close_session(&session_id)
        .expect("close_session must succeed even while a job is active");

    assert!(
        token.is_cancelled(),
        "the active job's cancellation token should be cancelled on close"
    );
    assert!(
        state.with_session(&session_id, |_| ()).is_none(),
        "the session should be removed after close"
    );
    assert!(
        state.cancel_job(&job_id).is_err(),
        "the job should already be removed from the registry after close"
    );
}
