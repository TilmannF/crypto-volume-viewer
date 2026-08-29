#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::panic,
    reason = "kdf/pim unit tests assert behavioral correctness with direct assertions"
)]

use cryptovol_tcvc::{compute_iterations, TcvcKdf, TcvcOpenError, TcvcOpenOptions};

#[test]
fn kdf_enum_has_required_variants() {
    let _: Vec<TcvcKdf> = vec![TcvcKdf::Sha512, TcvcKdf::Sha256];
}

#[test]
fn kdf_display_names_are_correct() {
    assert_eq!(TcvcKdf::Sha512.display_name(), "SHA-512");
    assert_eq!(TcvcKdf::Sha256.display_name(), "SHA-256");
}

#[test]
fn default_pim_gives_500000_iterations() {
    assert_eq!(
        compute_iterations(TcvcKdf::Sha512, None).expect("default PIM should not error"),
        500_000
    );
    assert_eq!(
        compute_iterations(TcvcKdf::Sha256, None).expect("default PIM should not error"),
        500_000
    );
}

#[test]
fn pim_zero_gives_500000_iterations() {
    assert_eq!(
        compute_iterations(TcvcKdf::Sha512, Some(0)).expect("PIM=0 should not error"),
        500_000
    );
}

#[test]
fn custom_pim_500_gives_515000_iterations() {
    assert_eq!(
        compute_iterations(TcvcKdf::Sha512, Some(500)).expect("PIM=500 should not error"),
        515_000
    );
    assert_eq!(
        compute_iterations(TcvcKdf::Sha256, Some(500)).expect("PIM=500 should not error"),
        515_000
    );
}

#[test]
fn custom_pim_1_gives_16000_iterations() {
    assert_eq!(
        compute_iterations(TcvcKdf::Sha512, Some(1)).expect("PIM=1 should not error"),
        16_000
    );
}

#[test]
fn overflow_pim_is_rejected() {
    let result = compute_iterations(TcvcKdf::Sha512, Some(u32::MAX));
    assert!(
        matches!(result, Err(TcvcOpenError::InvalidPim { .. })),
        "overflow PIM must return InvalidPim, got: {result:?}"
    );
}

#[test]
fn kdf_open_options_debug_does_not_expose_password() {
    let opts = TcvcOpenOptions {
        password: b"secret-password".to_vec(),
        pim: None,
        kdf_hint: None,
    };
    let debug = format!("{opts:?}");
    assert!(
        !debug.contains("secret-password"),
        "TcvcOpenOptions Debug must not expose raw password: {debug}"
    );
}

#[test]
fn invalid_pim_error_is_non_secret() {
    let err = TcvcOpenError::InvalidPim { reason: "overflow" };
    let display = err.to_string();
    let debug = format!("{err:?}");
    for output in [&display, &debug] {
        assert!(
            !output.contains("secret-password"),
            "TcvcOpenError::InvalidPim output must not contain secret marker: {output}"
        );
    }
}
