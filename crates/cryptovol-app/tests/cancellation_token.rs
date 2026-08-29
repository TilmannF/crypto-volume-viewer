//! Verifies `CancellationToken` state transitions and shared-state behavior
//! across cloned handles.

use cryptovol_app::CancellationToken;

#[test]
fn new_token_starts_uncancelled() {
    let token = CancellationToken::new();
    assert!(!token.is_cancelled());
}

#[test]
fn cancel_flips_state_on_the_same_handle() {
    let token = CancellationToken::new();
    token.cancel();
    assert!(token.is_cancelled());
}

#[test]
fn cancelling_a_clone_is_visible_through_the_original() {
    let token = CancellationToken::new();
    let clone = token.clone();

    clone.cancel();

    assert!(token.is_cancelled());
}

#[test]
fn cancelling_the_original_is_visible_through_a_clone() {
    let token = CancellationToken::new();
    let clone = token.clone();

    token.cancel();

    assert!(clone.is_cancelled());
}
