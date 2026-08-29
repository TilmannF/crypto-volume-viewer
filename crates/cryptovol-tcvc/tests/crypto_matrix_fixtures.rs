#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::panic,
    reason = "crypto-matrix fixture tests assert full-pipeline contract against static containers"
)]

use std::path::{Path, PathBuf};

use cryptovol_core::FileBlockReader;
use cryptovol_fs_fat::FatFileSystem;
use cryptovol_tcvc::{open_with_options, TcvcKdf, TcvcOpenError, TcvcOpenOptions};
use sha2::{Digest, Sha256};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crate should live two levels below workspace root")
        .to_path_buf()
}

fn crypto_matrix_fixture(name: &str) -> Option<PathBuf> {
    let dir = std::env::var("CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR").ok()?;
    let p = PathBuf::from(dir).join(name);
    if p.exists() {
        Some(p)
    } else {
        None
    }
}

fn read_lfn_original(name: &str) -> Vec<u8> {
    let base = workspace_root()
        .join("testdata")
        .join("static")
        .join("fs-fat-lfn-original");
    let path = name.split('/').fold(base, |acc, part| acc.join(part));
    std::fs::read(&path)
        .unwrap_or_else(|e| panic!("ground truth file {name:?} should be readable: {e}"))
}

fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

// --- SHA-256 / default PIM fixture tests ---

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR pointing to testdata/static/crypto-matrix/"]
fn sha256_fixture_opens_with_autoprobe() {
    let path = match crypto_matrix_fixture("tcvc-aes-sha256-pim-default-fat-lfn-unicode.hc") {
        Some(p) => p,
        None => {
            eprintln!("skipped: CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR not set or fixture absent");
            return;
        }
    };
    let reader = FileBlockReader::open(&path).expect("SHA-256 fixture should open read-only");
    let opts = TcvcOpenOptions {
        password: b"test-password".to_vec(),
        pim: None,
        kdf_hint: None,
    };
    let matched = open_with_options(&reader, &opts).expect("SHA-256 fixture should autoprobe open");
    assert_eq!(
        matched.matched_profile().kdf,
        TcvcKdf::Sha256,
        "autoprobe must match SHA-256"
    );
    assert!(
        matched.matched_profile().pim_is_default(),
        "default-PIM SHA-256 fixture must report pim_is_default() == true"
    );
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR pointing to testdata/static/crypto-matrix/"]
fn sha256_fixture_root_listing() {
    let path = match crypto_matrix_fixture("tcvc-aes-sha256-pim-default-fat-lfn-unicode.hc") {
        Some(p) => p,
        None => {
            eprintln!("skipped: CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR not set or fixture absent");
            return;
        }
    };
    let reader = FileBlockReader::open(&path).expect("SHA-256 fixture should open read-only");
    let opts = TcvcOpenOptions {
        password: b"test-password".to_vec(),
        pim: None,
        kdf_hint: None,
    };
    let matched = open_with_options(&reader, &opts).expect("SHA-256 fixture should open");
    let data_reader = matched
        .data_reader(&reader)
        .expect("data reader should be created");
    let fs = FatFileSystem::open(&data_reader).expect("FAT should parse");
    let entries = fs.list_dir("/").expect("root listing should succeed");
    let names: Vec<String> = entries.iter().map(|e| e.name.clone()).collect();

    for expected in [
        "Emoji Rocket 🚀 Test.txt",
        "Project Notes Final.txt",
        "Folder With Spaces",
        "Sydney Sweeney at the 2025 Toronto International Film Festival.jpg",
    ] {
        assert!(
            names.contains(&expected.to_owned()),
            "root listing must include {expected:?}: got {names:?}"
        );
    }
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR pointing to testdata/static/crypto-matrix/"]
fn sha256_fixture_extract_and_verify_rocket_txt() {
    let path = match crypto_matrix_fixture("tcvc-aes-sha256-pim-default-fat-lfn-unicode.hc") {
        Some(p) => p,
        None => {
            eprintln!("skipped: CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR not set or fixture absent");
            return;
        }
    };
    let reader = FileBlockReader::open(&path).expect("SHA-256 fixture should open read-only");
    let opts = TcvcOpenOptions {
        password: b"test-password".to_vec(),
        pim: None,
        kdf_hint: None,
    };
    let matched = open_with_options(&reader, &opts).expect("SHA-256 fixture should open");
    let data_reader = matched
        .data_reader(&reader)
        .expect("data reader should be created");
    let fs = FatFileSystem::open(&data_reader).expect("FAT should parse");

    let name = "Emoji Rocket 🚀 Test.txt";
    let extracted = fs
        .read_file(&format!("/{name}"))
        .unwrap_or_else(|e| panic!("{name:?} should be readable: {e}"));
    let expected = read_lfn_original(name);

    assert_eq!(
        sha256_hex(&extracted),
        sha256_hex(&expected),
        "SHA-256 of {name:?} must match ground truth"
    );

    let mut streamed = Vec::new();
    let n = fs
        .read_file_to_writer(&format!("/{name}"), &mut streamed)
        .unwrap_or_else(|e| panic!("read_file_to_writer({name:?}) failed: {e}"));
    assert_eq!(
        n as usize,
        streamed.len(),
        "streaming byte count mismatch for {name:?}"
    );
    assert_eq!(
        streamed, extracted,
        "streaming output must match read_file for {name:?}"
    );
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR pointing to testdata/static/crypto-matrix/"]
fn sha256_fixture_extract_and_verify_jpg() {
    let path = match crypto_matrix_fixture("tcvc-aes-sha256-pim-default-fat-lfn-unicode.hc") {
        Some(p) => p,
        None => {
            eprintln!("skipped: CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR not set or fixture absent");
            return;
        }
    };
    let reader = FileBlockReader::open(&path).expect("SHA-256 fixture should open read-only");
    let opts = TcvcOpenOptions {
        password: b"test-password".to_vec(),
        pim: None,
        kdf_hint: None,
    };
    let matched = open_with_options(&reader, &opts).expect("SHA-256 fixture should open");
    let data_reader = matched
        .data_reader(&reader)
        .expect("data reader should be created");
    let fs = FatFileSystem::open(&data_reader).expect("FAT should parse");

    let name = "Sydney Sweeney at the 2025 Toronto International Film Festival.jpg";
    let extracted = fs
        .read_file(&format!("/{name}"))
        .unwrap_or_else(|e| panic!("{name:?} should be readable: {e}"));
    let expected = read_lfn_original(name);

    assert_eq!(
        sha256_hex(&extracted),
        sha256_hex(&expected),
        "SHA-256 of {name:?} must match ground truth"
    );

    let mut streamed = Vec::new();
    let n = fs
        .read_file_to_writer(&format!("/{name}"), &mut streamed)
        .unwrap_or_else(|e| panic!("read_file_to_writer({name:?}) failed: {e}"));
    assert_eq!(
        n as usize,
        streamed.len(),
        "streaming byte count mismatch for {name:?}"
    );
    assert_eq!(
        streamed, extracted,
        "streaming output must match read_file for {name:?}"
    );
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR pointing to testdata/static/crypto-matrix/"]
fn sha256_fixture_wrong_password_fails() {
    let path = match crypto_matrix_fixture("tcvc-aes-sha256-pim-default-fat-lfn-unicode.hc") {
        Some(p) => p,
        None => {
            eprintln!("skipped: CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR not set or fixture absent");
            return;
        }
    };
    let reader = FileBlockReader::open(&path).expect("SHA-256 fixture should open read-only");
    let opts = TcvcOpenOptions {
        password: b"wrong-password".to_vec(),
        pim: None,
        kdf_hint: None,
    };
    let err =
        open_with_options(&reader, &opts).expect_err("wrong password must fail on SHA-256 fixture");
    assert_eq!(err, TcvcOpenError::AuthenticationOrUnsupported);
}

// --- SHA-512 / PIM-500 fixture tests ---

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR pointing to testdata/static/crypto-matrix/"]
fn sha512_pim500_fixture_opens_with_correct_pim() {
    let path = match crypto_matrix_fixture("tcvc-aes-sha512-pim-500-fat-lfn-unicode.hc") {
        Some(p) => p,
        None => {
            eprintln!("skipped: CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR not set or fixture absent");
            return;
        }
    };
    let reader = FileBlockReader::open(&path).expect("PIM-500 fixture should open read-only");
    let opts = TcvcOpenOptions {
        password: b"test-password".to_vec(),
        pim: Some(500),
        kdf_hint: None,
    };
    let matched =
        open_with_options(&reader, &opts).expect("PIM-500 fixture should open with PIM=500");
    assert_eq!(
        matched.matched_profile().kdf,
        TcvcKdf::Sha512,
        "PIM-500 fixture must match SHA-512"
    );
    assert_eq!(
        matched.matched_profile().pim_value(),
        Some(500),
        "PIM-500 fixture must report pim_value() == Some(500)"
    );
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR pointing to testdata/static/crypto-matrix/"]
fn sha512_pim500_fixture_listing() {
    let path = match crypto_matrix_fixture("tcvc-aes-sha512-pim-500-fat-lfn-unicode.hc") {
        Some(p) => p,
        None => {
            eprintln!("skipped: CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR not set or fixture absent");
            return;
        }
    };
    let reader = FileBlockReader::open(&path).expect("PIM-500 fixture should open read-only");
    let opts = TcvcOpenOptions {
        password: b"test-password".to_vec(),
        pim: Some(500),
        kdf_hint: None,
    };
    let matched = open_with_options(&reader, &opts).expect("PIM-500 fixture should open");
    let data_reader = matched
        .data_reader(&reader)
        .expect("data reader should be created");
    let fs = FatFileSystem::open(&data_reader).expect("FAT should parse");
    let entries = fs.list_dir("/").expect("root listing should succeed");
    let names: Vec<String> = entries.iter().map(|e| e.name.clone()).collect();

    for expected in [
        "Emoji Rocket 🚀 Test.txt",
        "Project Notes Final.txt",
        "Folder With Spaces",
        "Sydney Sweeney at the 2025 Toronto International Film Festival.jpg",
    ] {
        assert!(
            names.contains(&expected.to_owned()),
            "root listing must include {expected:?}: got {names:?}"
        );
    }
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR pointing to testdata/static/crypto-matrix/"]
fn sha512_pim500_fixture_extract_and_verify() {
    let path = match crypto_matrix_fixture("tcvc-aes-sha512-pim-500-fat-lfn-unicode.hc") {
        Some(p) => p,
        None => {
            eprintln!("skipped: CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR not set or fixture absent");
            return;
        }
    };
    let reader = FileBlockReader::open(&path).expect("PIM-500 fixture should open read-only");
    let opts = TcvcOpenOptions {
        password: b"test-password".to_vec(),
        pim: Some(500),
        kdf_hint: None,
    };
    let matched = open_with_options(&reader, &opts).expect("PIM-500 fixture should open");
    let data_reader = matched
        .data_reader(&reader)
        .expect("data reader should be created");
    let fs = FatFileSystem::open(&data_reader).expect("FAT should parse");

    let name = "Project Notes Final.txt";
    let extracted = fs
        .read_file(&format!("/{name}"))
        .unwrap_or_else(|e| panic!("{name:?} should be readable: {e}"));
    let expected = read_lfn_original(name);

    assert_eq!(
        sha256_hex(&extracted),
        sha256_hex(&expected),
        "SHA-256 of {name:?} must match ground truth"
    );

    let mut streamed = Vec::new();
    let n = fs
        .read_file_to_writer(&format!("/{name}"), &mut streamed)
        .unwrap_or_else(|e| panic!("read_file_to_writer({name:?}) failed: {e}"));
    assert_eq!(
        n as usize,
        streamed.len(),
        "streaming byte count mismatch for {name:?}"
    );
    assert_eq!(
        streamed, extracted,
        "streaming output must match read_file for {name:?}"
    );
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR pointing to testdata/static/crypto-matrix/"]
fn sha512_pim500_default_pim_fails() {
    let path = match crypto_matrix_fixture("tcvc-aes-sha512-pim-500-fat-lfn-unicode.hc") {
        Some(p) => p,
        None => {
            eprintln!("skipped: CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR not set or fixture absent");
            return;
        }
    };
    let reader = FileBlockReader::open(&path).expect("PIM-500 fixture should open read-only");
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
fn sha512_pim500_wrong_pim_fails() {
    let path = match crypto_matrix_fixture("tcvc-aes-sha512-pim-500-fat-lfn-unicode.hc") {
        Some(p) => p,
        None => {
            eprintln!("skipped: CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR not set or fixture absent");
            return;
        }
    };
    let reader = FileBlockReader::open(&path).expect("PIM-500 fixture should open read-only");
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
#[ignore = "requires CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR pointing to testdata/static/crypto-matrix/"]
fn sha512_pim500_nested_folder_listing() {
    let path = match crypto_matrix_fixture("tcvc-aes-sha512-pim-500-fat-lfn-unicode.hc") {
        Some(p) => p,
        None => {
            eprintln!("skipped: CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR not set or fixture absent");
            return;
        }
    };
    let reader = FileBlockReader::open(&path).expect("PIM-500 fixture should open read-only");
    let opts = TcvcOpenOptions {
        password: b"test-password".to_vec(),
        pim: Some(500),
        kdf_hint: None,
    };
    let matched = open_with_options(&reader, &opts).expect("PIM-500 fixture should open");
    let data_reader = matched
        .data_reader(&reader)
        .expect("data reader should be created");
    let fs = FatFileSystem::open(&data_reader).expect("FAT should parse");
    let entries = fs
        .list_dir("/Folder With Spaces")
        .expect("nested folder listing should succeed");
    let names: Vec<String> = entries.iter().map(|e| e.name.clone()).collect();

    assert!(
        names.contains(&"Rocket Science 🚀 For Beginners.txt".to_owned()),
        "nested listing must include 'Rocket Science 🚀 For Beginners.txt': got {names:?}"
    );
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR pointing to testdata/static/crypto-matrix/"]
fn sha512_pim500_extract_nested_file() {
    let path = match crypto_matrix_fixture("tcvc-aes-sha512-pim-500-fat-lfn-unicode.hc") {
        Some(p) => p,
        None => {
            eprintln!("skipped: CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR not set or fixture absent");
            return;
        }
    };
    let reader = FileBlockReader::open(&path).expect("PIM-500 fixture should open read-only");
    let opts = TcvcOpenOptions {
        password: b"test-password".to_vec(),
        pim: Some(500),
        kdf_hint: None,
    };
    let matched = open_with_options(&reader, &opts).expect("PIM-500 fixture should open");
    let data_reader = matched
        .data_reader(&reader)
        .expect("data reader should be created");
    let fs = FatFileSystem::open(&data_reader).expect("FAT should parse");

    let container_path = "/Folder With Spaces/Rocket Science 🚀 For Beginners.txt";
    let ground_truth_name = "Folder With Spaces/Rocket Science 🚀 For Beginners.txt";

    let extracted = fs
        .read_file(container_path)
        .unwrap_or_else(|e| panic!("{container_path:?} should be readable: {e}"));
    let expected = read_lfn_original(ground_truth_name);

    assert_eq!(
        sha256_hex(&extracted),
        sha256_hex(&expected),
        "SHA-256 of nested file must match ground truth"
    );

    let mut streamed = Vec::new();
    let n = fs
        .read_file_to_writer(container_path, &mut streamed)
        .unwrap_or_else(|e| panic!("read_file_to_writer({container_path:?}) failed: {e}"));
    assert_eq!(
        n as usize,
        streamed.len(),
        "nested file streaming byte count mismatch"
    );
    assert_eq!(
        streamed, extracted,
        "nested file streaming output must match read_file"
    );
}
