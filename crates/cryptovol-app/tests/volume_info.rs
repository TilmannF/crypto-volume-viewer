#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::panic,
    reason = "volume_info tests build a synthetic TC/VC container with direct assertions"
)]

//! Verifies that `VolumeSession::volume_info` reports safe, non-secret
//! metadata matching how the session was opened.

mod support;

use cryptovol_app::{open_volume, FilesystemKind, OpenVolumeRequest};
use cryptovol_tcvc::{HeaderCandidateRole, PimState, TcvcKdf};
use secrecy::SecretString;
use support::write_synthetic_container;

#[test]
fn reports_safe_metadata_matching_how_the_session_was_opened() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let path = write_synthetic_container(&dir, "container.hc");
    let expected_size = std::fs::metadata(&path).expect("read file metadata").len();

    let session = open_volume(OpenVolumeRequest {
        container_path: path.clone(),
        password: SecretString::from(support::TEST_PASSWORD.to_string()),
        pim: None,
        kdf_hint: Some(TcvcKdf::Sha512),
    })
    .expect("open_volume should succeed with the correct password");

    let info = session.volume_info();

    assert_eq!(info.container_path, path);
    assert_eq!(info.container_size_bytes, expected_size);
    assert_eq!(info.backend, "tcvc");
    assert!(
        info.cipher.contains("AES-XTS"),
        "expected cipher to mention AES-XTS, got {:?}",
        info.cipher
    );
    assert_eq!(info.kdf, TcvcKdf::Sha512);
    assert_eq!(info.pim, PimState::Default);
    assert_eq!(info.header_role, HeaderCandidateRole::Primary);
    assert_eq!(info.filesystem, FilesystemKind::Fat);
    assert!(info.read_only, "opened volumes must always be read-only");
}

#[test]
fn reports_decrypted_data_region_offset_and_length() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let path = write_synthetic_container(&dir, "container.hc");

    let session = open_volume(OpenVolumeRequest {
        container_path: path,
        password: SecretString::from(support::TEST_PASSWORD.to_string()),
        pim: None,
        kdf_hint: Some(TcvcKdf::Sha512),
    })
    .expect("open_volume should succeed with the correct password");

    let info = session.volume_info();

    let offset = info
        .decrypted_data_offset
        .expect("decrypted_data_offset should be known for a tcvc volume");
    let len = info
        .decrypted_data_len
        .expect("decrypted_data_len should be known for a tcvc volume");

    assert!(
        offset > 0,
        "decrypted data offset should be past the header"
    );
    assert!(
        offset + len <= info.container_size_bytes,
        "decrypted data region (offset {offset} + len {len}) must fit within the container ({} bytes)",
        info.container_size_bytes
    );
}

#[test]
fn debug_output_never_exposes_secrets() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let path = write_synthetic_container(&dir, "container.hc");

    let session = open_volume(OpenVolumeRequest {
        container_path: path,
        password: SecretString::from(support::TEST_PASSWORD.to_string()),
        pim: None,
        kdf_hint: Some(TcvcKdf::Sha512),
    })
    .expect("open_volume should succeed with the correct password");

    let debug = format!("{:?}", session.volume_info());
    assert!(
        !debug.contains(support::TEST_PASSWORD),
        "VolumeInfo debug output must not contain the password: {debug}"
    );
}
