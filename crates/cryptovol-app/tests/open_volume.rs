#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::panic,
    reason = "open_volume tests build a synthetic TC/VC container with direct assertions"
)]

//! Verifies that `open_volume` opens a TC/VC volume with the correct
//! password, rejects the wrong one, and never retains the password on the
//! returned `VolumeSession`.
//!
//! The synthetic container builder here is a trimmed, local copy of the
//! one in `crates/cryptovol-tcvc/tests/data_reader.rs`: cryptovol-tcvc does
//! not export a test-support API, and `open_volume` opens a real file via
//! `FileBlockReader`, so the encrypted bytes are written to a temp file
//! instead of wrapped in an in-memory `BlockReader`.

mod support;

use cryptovol_app::{open_volume, AppError, OpenVolumeRequest};
use secrecy::SecretString;
use support::write_synthetic_container;

#[test]
fn opens_with_the_correct_password() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let path = write_synthetic_container(&dir, "container.hc");

    let session = open_volume(OpenVolumeRequest {
        container_path: path,
        password: SecretString::from(support::TEST_PASSWORD.to_string()),
        pim: None,
        kdf_hint: None,
    })
    .expect("open_volume should succeed with the correct password");

    let _ = session; // session existing at all is the assertion here
}

#[test]
fn rejects_the_wrong_password() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let path = write_synthetic_container(&dir, "container.hc");

    // Pin kdf_hint to the KDF the fixture was actually encrypted with: this
    // test verifies wrong-password -> AuthFailed mapping, not the multi-KDF
    // autoprobe behavior (already covered by cryptovol-tcvc's own tests),
    // and autoprobing here would mean 5x 500k-iteration PBKDF2 attempts.
    let result = open_volume(OpenVolumeRequest {
        container_path: path,
        password: SecretString::from("wrong password".to_string()),
        pim: None,
        kdf_hint: Some(cryptovol_tcvc::TcvcKdf::Sha512),
    });

    assert!(
        matches!(result, Err(AppError::AuthFailed)),
        "expected AuthFailed, got {result:?}"
    );
}

#[test]
fn session_debug_output_never_contains_the_password() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let path = write_synthetic_container(&dir, "container.hc");

    let session = open_volume(OpenVolumeRequest {
        container_path: path,
        password: SecretString::from(support::TEST_PASSWORD.to_string()),
        pim: None,
        kdf_hint: None,
    })
    .expect("open_volume should succeed with the correct password");

    let debug = format!("{session:?}");
    assert!(
        !debug.contains(support::TEST_PASSWORD),
        "VolumeSession debug output must not contain the password: {debug}"
    );
}
