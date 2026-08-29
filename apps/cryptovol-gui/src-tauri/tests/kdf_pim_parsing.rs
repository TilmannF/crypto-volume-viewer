#![allow(
    missing_docs,
    clippy::expect_used,
    reason = "parsing tests exercise the public parse_kdf_hint/parse_pim helpers directly"
)]

//! Verifies KDF-hint and PIM parsing accept the same token set the CLI uses
//! (crates/cryptovol-cli/src/commands.rs::parse_kdf_hint) and reject
//! anything else with a stable `invalid_input` error, since the frontend's
//! own validation must never be trusted as the sole gate.

use cryptovol_app::TcvcKdf;
use cryptovol_gui_lib::dto::session::{parse_kdf_hint, parse_pim};

#[test]
fn accepts_none_as_auto() {
    assert_eq!(parse_kdf_hint(None).expect("None is Auto"), None);
}

#[test]
fn accepts_every_supported_kdf_token() {
    assert_eq!(
        parse_kdf_hint(Some("sha512")).expect("sha512 should parse"),
        Some(TcvcKdf::Sha512)
    );
    assert_eq!(
        parse_kdf_hint(Some("sha256")).expect("sha256 should parse"),
        Some(TcvcKdf::Sha256)
    );
    assert_eq!(
        parse_kdf_hint(Some("whirlpool")).expect("whirlpool should parse"),
        Some(TcvcKdf::Whirlpool)
    );
    assert_eq!(
        parse_kdf_hint(Some("blake2s")).expect("blake2s should parse"),
        Some(TcvcKdf::Blake2s256)
    );
    assert_eq!(
        parse_kdf_hint(Some("streebog")).expect("streebog should parse"),
        Some(TcvcKdf::Streebog)
    );
}

#[test]
fn rejects_unknown_kdf_token_with_invalid_input() {
    let err = parse_kdf_hint(Some("md5")).expect_err("unsupported KDF token must be rejected");
    assert_eq!(err.code, "invalid_input");
    assert!(!err.message.is_empty());
}

#[test]
fn accepts_any_pim_value_or_none() {
    assert_eq!(parse_pim(None).expect("None PIM is the default"), None);
    assert_eq!(parse_pim(Some(0)).expect("PIM 0 should parse"), Some(0));
    assert_eq!(
        parse_pim(Some(4_000_000)).expect("large PIM should parse"),
        Some(4_000_000)
    );
}
