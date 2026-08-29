#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::panic,
    reason = "header-open tests build synthetic encrypted headers with direct assertions"
)]

use std::env;
use std::path::PathBuf;

use cryptovol_core::{BlockReader, CryptovolError, FileBlockReader};
use cryptovol_tcvc::{
    open_aes_sha512_header, open_with_options, validate_decrypted_header, TcvcKdf, TcvcOpenError,
    TcvcOpenInfo, TcvcOpenOptions,
};

#[derive(Debug)]
struct MemoryReader {
    data: Vec<u8>,
}

impl MemoryReader {
    fn new(data: Vec<u8>) -> Self {
        Self { data }
    }
}

impl BlockReader for MemoryReader {
    fn len(&self) -> u64 {
        self.data.len() as u64
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<(), CryptovolError> {
        let start = offset as usize;
        let end = start
            .checked_add(buf.len())
            .ok_or(CryptovolError::OutOfBounds {
                offset,
                length: buf.len(),
                file_len: self.len(),
            })?;

        if end > self.data.len() {
            return Err(CryptovolError::OutOfBounds {
                offset,
                length: buf.len(),
                file_len: self.len(),
            });
        }

        buf.copy_from_slice(&self.data[start..end]);
        Ok(())
    }
}

#[test]
#[ignore = "requires scripts/test-with-veracrypt-fixtures.sh to generate CRYPTOVOL_TEST_CONTAINER"]
fn opens_generated_aes_sha512_fixture_with_test_password() {
    let container = match env::var("CRYPTOVOL_TEST_CONTAINER") {
        Ok(c) => c,
        Err(_) => {
            eprintln!("skipped: CRYPTOVOL_TEST_CONTAINER not set");
            return;
        }
    };
    let reader = FileBlockReader::open(container).expect("fixture container should open read-only");

    let info = open_aes_sha512_header(&reader, b"test-password")
        .expect("generated AES/SHA-512 fixture should open");

    assert_supported_profile(&info);
    assert!(info.volume_size > 0);
    assert!(info.data_length > 0);
}

#[test]
#[ignore = "requires scripts/test-with-veracrypt-fixtures.sh to generate CRYPTOVOL_TEST_CONTAINER"]
fn generated_aes_sha512_fixture_rejects_wrong_password() {
    let container = match env::var("CRYPTOVOL_TEST_CONTAINER") {
        Ok(c) => c,
        Err(_) => {
            eprintln!("skipped: CRYPTOVOL_TEST_CONTAINER not set");
            return;
        }
    };
    let reader = FileBlockReader::open(container).expect("fixture container should open read-only");

    let error = open_aes_sha512_header(&reader, b"wrong-test-password")
        .expect_err("wrong password should not open fixture");

    assert_eq!(error, TcvcOpenError::AuthenticationOrUnsupported);
    assert_error_formatting_is_non_secret(&error);
}

#[test]
fn validates_decrypted_header_and_returns_safe_metadata() {
    let header = valid_decrypted_header();

    let info = validate_decrypted_header(&header).expect("synthetic header should validate");

    assert_supported_profile(&info);
    assert_eq!(info.volume_size, 20 * 1024 * 1024);
    assert_eq!(info.data_offset, 131_072);
    assert_eq!(info.data_length, 20 * 1024 * 1024 - 131_072);
    assert_eq!(info.sector_size, 512);
    assert_info_formatting_is_non_secret(&info);
}

#[test]
fn rejects_decrypted_header_without_vera_magic() {
    let mut header = valid_decrypted_header();
    header[64..68].copy_from_slice(b"NOPE");
    refresh_header_crc(&mut header);

    let error = validate_decrypted_header(&header).expect_err("bad magic should fail");

    assert_eq!(error, TcvcOpenError::AuthenticationOrUnsupported);
    assert_error_formatting_is_non_secret(&error);
}

#[test]
fn rejects_decrypted_header_with_bad_key_area_crc() {
    let mut header = valid_decrypted_header();
    header[300] ^= 0xff;

    let error = validate_decrypted_header(&header).expect_err("bad key-area CRC should fail");

    assert_eq!(error, TcvcOpenError::AuthenticationOrUnsupported);
    assert_error_formatting_is_non_secret(&error);
}

#[test]
fn rejects_decrypted_header_with_bad_metadata_crc() {
    let mut header = valid_decrypted_header();
    header[100] ^= 0xff;

    let error = validate_decrypted_header(&header).expect_err("bad metadata CRC should fail");

    assert_eq!(error, TcvcOpenError::AuthenticationOrUnsupported);
    assert_error_formatting_is_non_secret(&error);
}

#[test]
fn rejects_nonzero_hidden_volume_size() {
    let mut header = valid_decrypted_header();
    write_u64_be(&mut header, 92, 4096);
    refresh_header_crc(&mut header);

    let error = validate_decrypted_header(&header).expect_err("hidden volumes are unsupported");

    assert!(matches!(error, TcvcOpenError::UnsupportedProfile { .. }));
    assert_error_formatting_is_non_secret(&error);
}

#[test]
fn rejects_system_or_in_place_flags() {
    let mut header = valid_decrypted_header();
    write_u32_be(&mut header, 124, 0x0000_0001);
    refresh_header_crc(&mut header);

    let error =
        validate_decrypted_header(&header).expect_err("system or in-place flags are unsupported");

    assert!(matches!(error, TcvcOpenError::UnsupportedProfile { .. }));
    assert_error_formatting_is_non_secret(&error);
}

#[test]
fn open_api_maps_random_data_to_non_secret_failure() {
    let reader = MemoryReader::new(vec![0x5a; 1024]);

    let error =
        open_aes_sha512_header(&reader, b"test-password").expect_err("random data should not open");

    assert_eq!(error, TcvcOpenError::AuthenticationOrUnsupported);
    assert_error_formatting_is_non_secret(&error);
}

fn assert_supported_profile(info: &TcvcOpenInfo) {
    assert_eq!(info.backend, "tcvc");
    assert_eq!(info.profile, "tcvc-aes-sha512-basic");
    assert!(info.read_only);
}

fn assert_info_formatting_is_non_secret(info: &TcvcOpenInfo) {
    let debug = format!("{info:?}");
    for secret in [
        "test-password",
        "wrong-test-password",
        "derived",
        "header key",
        "master key",
        "raw decrypted",
        "aaaaaaaaaaaaaaaa",
    ] {
        assert!(
            !debug.contains(secret),
            "TcvcOpenInfo debug output should not expose secret marker {secret:?}: {debug}"
        );
    }
}

fn assert_error_formatting_is_non_secret(error: &TcvcOpenError) {
    let display = error.to_string();
    let debug = format!("{error:?}");
    for rendered in [display, debug] {
        for secret in [
            "test-password",
            "wrong-test-password",
            "derived",
            "salt plus derived",
            "header key",
            "master key",
            "raw decrypted",
            "aaaaaaaaaaaaaaaa",
        ] {
            assert!(
                !rendered.contains(secret),
                "TcvcOpenError output should not expose secret marker {secret:?}: {rendered}"
            );
        }
    }
}

fn valid_decrypted_header() -> [u8; 512] {
    let mut header = [0u8; 512];
    header[64..68].copy_from_slice(b"VERA");
    write_u16_be(&mut header, 68, 5);
    write_u16_be(&mut header, 70, 5);
    write_u64_be(&mut header, 92, 0);
    write_u64_be(&mut header, 100, 20 * 1024 * 1024);
    write_u64_be(&mut header, 108, 131_072);
    write_u64_be(&mut header, 116, 20 * 1024 * 1024 - 131_072);
    write_u32_be(&mut header, 124, 0);
    write_u32_be(&mut header, 128, 512);
    header[256..512].fill(0xa5);
    refresh_header_crc(&mut header);
    header
}

fn refresh_header_crc(header: &mut [u8; 512]) {
    let key_area_crc = crc32(&header[256..512]);
    write_u32_be(header, 72, key_area_crc);

    let metadata_crc = crc32(&header[64..252]);
    write_u32_be(header, 252, metadata_crc);
}

fn write_u16_be(buf: &mut [u8], offset: usize, value: u16) {
    buf[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
}

fn write_u32_be(buf: &mut [u8], offset: usize, value: u32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}

fn write_u64_be(buf: &mut [u8], offset: usize, value: u64) {
    buf[offset..offset + 8].copy_from_slice(&value.to_be_bytes());
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;

    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }

    !crc
}

// --- open_with_options autoprobing tests (6 ignored, 1 non-ignored) ---

fn crypto_matrix_fixture(name: &str) -> Option<PathBuf> {
    let dir = env::var("CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR").ok()?;
    let path = PathBuf::from(dir).join(name);
    if path.exists() {
        Some(path)
    } else {
        None
    }
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR pointing to testdata/static/crypto-matrix/"]
fn open_with_options_autoprobes_sha256_container() {
    let path = match crypto_matrix_fixture("tcvc-aes-sha256-pim-default-fat-lfn-unicode.hc") {
        Some(p) => p,
        None => {
            eprintln!("skipped: CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR not set or fixture absent");
            return;
        }
    };
    let reader = FileBlockReader::open(path).expect("SHA-256 fixture should open read-only");
    let opts = TcvcOpenOptions {
        password: b"test-password".to_vec(),
        pim: None,
        kdf_hint: None,
    };
    let matched = open_with_options(&reader, &opts).expect("SHA-256 fixture should autoprobe open");
    assert_eq!(matched.matched_profile().kdf, TcvcKdf::Sha256);
    assert!(
        matched.matched_profile().pim_is_default(),
        "default-PIM SHA-256 fixture must report pim_is_default() == true"
    );
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR pointing to testdata/static/crypto-matrix/"]
fn open_with_options_kdf_hint_restricts_to_sha256() {
    let path = match crypto_matrix_fixture("tcvc-aes-sha256-pim-default-fat-lfn-unicode.hc") {
        Some(p) => p,
        None => {
            eprintln!("skipped: CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR not set or fixture absent");
            return;
        }
    };
    let reader = FileBlockReader::open(path).expect("SHA-256 fixture should open read-only");
    let opts = TcvcOpenOptions {
        password: b"test-password".to_vec(),
        pim: None,
        kdf_hint: Some(TcvcKdf::Sha256),
    };
    open_with_options(&reader, &opts).expect("SHA-256 hint on SHA-256 fixture must succeed");
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR pointing to testdata/static/crypto-matrix/"]
fn open_with_options_wrong_kdf_hint_fails() {
    let path = match crypto_matrix_fixture("tcvc-aes-sha256-pim-default-fat-lfn-unicode.hc") {
        Some(p) => p,
        None => {
            eprintln!("skipped: CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR not set or fixture absent");
            return;
        }
    };
    let reader = FileBlockReader::open(path).expect("SHA-256 fixture should open read-only");
    let opts = TcvcOpenOptions {
        password: b"test-password".to_vec(),
        pim: None,
        kdf_hint: Some(TcvcKdf::Sha512),
    };
    let err =
        open_with_options(&reader, &opts).expect_err("SHA-512 hint on SHA-256 fixture must fail");
    assert_eq!(err, TcvcOpenError::AuthenticationOrUnsupported);
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR pointing to testdata/static/crypto-matrix/"]
fn open_with_options_autoprobes_sha512_pim500_container() {
    let path = match crypto_matrix_fixture("tcvc-aes-sha512-pim-500-fat-lfn-unicode.hc") {
        Some(p) => p,
        None => {
            eprintln!("skipped: CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR not set or fixture absent");
            return;
        }
    };
    let reader = FileBlockReader::open(path).expect("PIM-500 fixture should open read-only");
    let opts = TcvcOpenOptions {
        password: b"test-password".to_vec(),
        pim: Some(500),
        kdf_hint: None,
    };
    let matched =
        open_with_options(&reader, &opts).expect("PIM-500 fixture should open with PIM=500");
    assert_eq!(matched.matched_profile().kdf, TcvcKdf::Sha512);
    assert_eq!(
        matched.matched_profile().pim_value(),
        Some(500),
        "PIM-500 fixture must report pim_value() == Some(500)"
    );
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR pointing to testdata/static/crypto-matrix/"]
fn open_with_options_default_pim_fails_on_pim500_container() {
    let path = match crypto_matrix_fixture("tcvc-aes-sha512-pim-500-fat-lfn-unicode.hc") {
        Some(p) => p,
        None => {
            eprintln!("skipped: CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR not set or fixture absent");
            return;
        }
    };
    let reader = FileBlockReader::open(path).expect("PIM-500 fixture should open read-only");
    let opts = TcvcOpenOptions {
        password: b"test-password".to_vec(),
        pim: None,
        kdf_hint: None,
    };
    let err =
        open_with_options(&reader, &opts).expect_err("default PIM must fail on PIM-500 fixture");
    assert_eq!(err, TcvcOpenError::AuthenticationOrUnsupported);
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR pointing to testdata/static/crypto-matrix/"]
fn open_with_options_wrong_pim_fails_on_pim500_container() {
    let path = match crypto_matrix_fixture("tcvc-aes-sha512-pim-500-fat-lfn-unicode.hc") {
        Some(p) => p,
        None => {
            eprintln!("skipped: CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR not set or fixture absent");
            return;
        }
    };
    let reader = FileBlockReader::open(path).expect("PIM-500 fixture should open read-only");
    let opts = TcvcOpenOptions {
        password: b"test-password".to_vec(),
        pim: Some(1),
        kdf_hint: None,
    };
    let err =
        open_with_options(&reader, &opts).expect_err("wrong PIM (1) must fail on PIM-500 fixture");
    assert_eq!(err, TcvcOpenError::AuthenticationOrUnsupported);
}

#[test]
fn open_with_options_wrong_password_fails() {
    let reader = MemoryReader::new(vec![0x5a; 1024]);
    // Use a fixed KDF hint; testing autoprobe is not the purpose of this test.
    // Without a hint, all 5 KDFs × 500K iterations run on random bytes (very slow).
    let opts = TcvcOpenOptions {
        password: b"wrong-password".to_vec(),
        pim: None,
        kdf_hint: Some(TcvcKdf::Sha512),
    };
    let err =
        open_with_options(&reader, &opts).expect_err("random bytes with wrong password must fail");
    assert_eq!(err, TcvcOpenError::AuthenticationOrUnsupported);
    assert_error_formatting_is_non_secret(&err);
}
