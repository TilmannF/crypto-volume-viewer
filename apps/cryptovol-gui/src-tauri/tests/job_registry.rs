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

use cryptovol_app::CancellationToken;
use cryptovol_gui_lib::dto::error::JOB_NOT_FOUND;
use cryptovol_gui_lib::state::{GuiState, JobId, SessionId};

#[test]
fn generated_job_ids_are_unique_even_for_the_same_session_and_source_path() {
    let state = GuiState::default();
    let session_id = SessionId::new();

    let job_a = state.insert_job(session_id.clone(), CancellationToken::new());
    let job_b = state.insert_job(session_id, CancellationToken::new());

    assert_ne!(job_a.as_str(), job_b.as_str());
    assert!(!job_a.as_str().is_empty());
}

#[test]
fn cancelling_a_known_job_flips_its_token_and_removes_it() {
    let state = GuiState::default();
    let session_id = SessionId::new();
    let token = CancellationToken::new();
    let job_id = state.insert_job(session_id, token.clone());

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
