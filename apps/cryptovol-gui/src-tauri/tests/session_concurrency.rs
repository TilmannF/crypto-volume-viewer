#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::panic,
    reason = "concurrency regression tests open real synthetic volumes and assert on registry behavior"
)]

//! Verifies `GuiState` never holds its session registry lock while work runs
//! against a session, and that job registration is atomic with respect to
//! `close_session`: a job can only be registered for a session that is still
//! open, and closing a session cancels and removes every job it owns.

mod support;

use cryptovol_app::{open_volume, CancellationToken, OpenVolumeRequest, VolumeSession};
use cryptovol_gui_lib::commands::session::{list_dir_impl, stat_impl};
use cryptovol_gui_lib::dto::error::SESSION_NOT_FOUND;
use cryptovol_gui_lib::state::GuiState;
use secrecy::SecretString;
use std::sync::{mpsc, Arc};
use std::time::Duration;

const LOCK_TIMEOUT: Duration = Duration::from_secs(5);

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
fn session_lookup_does_not_hold_the_registry_lock_while_work_runs() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let state = Arc::new(GuiState::default());
    let session_id = state.insert_session(open_test_session(&dir, "lock.hc"));

    state.with_session(&session_id, |_session| {
        // Simulates a long-running operation (such as an extraction) against
        // this session while another command browses the same session.
        let (done_tx, done_rx) = mpsc::channel();
        let browsing_state = Arc::clone(&state);
        let browsing_id = session_id.clone();
        // Detached on purpose: if the lock is held, this thread blocks until
        // the closure unwinds, and joining it here would hang the test.
        std::thread::spawn(move || {
            let found = browsing_state.with_session(&browsing_id, |_| ()).is_some();
            let listed = list_dir_impl(&browsing_state, browsing_id.as_str(), "/");
            let statted = stat_impl(&browsing_state, browsing_id.as_str(), "/");
            let _ = done_tx.send((found, listed.err(), statted.err()));
        });

        let (found, list_err, stat_err) = done_rx
            .recv_timeout(LOCK_TIMEOUT)
            .unwrap_or_else(|_| panic!("browsing the session blocked on the registry lock"));
        assert!(found, "the session must still be visible to other callers");
        for err in [list_err, stat_err].into_iter().flatten() {
            assert_ne!(
                err.code, SESSION_NOT_FOUND,
                "browsing must reach the session, got {err:?}"
            );
        }
    });
}

#[test]
fn registering_a_job_for_a_closed_session_returns_session_not_found() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let state = GuiState::default();
    let session_id = state.insert_session(open_test_session(&dir, "closed.hc"));
    state
        .close_session(&session_id)
        .expect("close_session should succeed for an open session");

    let err = state
        .insert_job(session_id, CancellationToken::new())
        .expect_err("a job must not be registered for a closed session");

    assert_eq!(err.code, SESSION_NOT_FOUND);
    assert_eq!(state.active_job_count(), 0);
}

#[test]
fn closing_a_session_cancels_and_removes_its_registered_jobs() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let state = GuiState::default();
    let session_id = state.insert_session(open_test_session(&dir, "open.hc"));
    let token = CancellationToken::new();
    state
        .insert_job(session_id.clone(), token.clone())
        .expect("registering a job for an open session should succeed");

    state
        .close_session(&session_id)
        .expect("close_session should succeed with an active job");

    assert!(token.is_cancelled(), "the job's token must be cancelled");
    assert_eq!(state.active_job_count(), 0, "the job must be removed");
    assert!(
        state.session(&session_id).is_none(),
        "the session must be removed"
    );
}
