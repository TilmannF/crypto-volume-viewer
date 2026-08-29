#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "fixture tests use direct setup assertions"
)]

use sha2::{Digest, Sha256};
use std::path::PathBuf;

use cryptovol_core::FileBlockReader;
use cryptovol_fs_fat::{FatError, FatFileSystem};
use cryptovol_tcvc::{open_aes_sha512_volume, TcvcOpenError};

/// Return the LFN fixture path from CRYPTOVOL_STATIC_FAT_LFN_FIXTURE, or None if unset/absent.
fn lfn_fixture_path() -> Option<PathBuf> {
    let val = std::env::var("CRYPTOVOL_STATIC_FAT_LFN_FIXTURE").ok()?;
    let p = PathBuf::from(val);
    if p.exists() {
        Some(p)
    } else {
        None
    }
}

fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Read a ground-truth file from testdata/static/fs-fat-lfn-original/ relative to project root.
/// Crate manifest is at crates/cryptovol-fs-fat/Cargo.toml, so go up two levels.
fn read_lfn_original(name: &str) -> Vec<u8> {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest
        .join("../../testdata/static/fs-fat-lfn-original")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("cannot read ground truth {name}: {e}"))
}

fn open_lfn_fixture() -> Option<(FileBlockReader, cryptovol_tcvc::TcvcOpenedVolume)> {
    let path = lfn_fixture_path()?;
    let reader = FileBlockReader::open(&path).expect("should open LFN fixture file");
    let opened = open_aes_sha512_volume(&reader, b"test-password")
        .expect("should open LFN fixture with test-password");
    Some((reader, opened))
}

#[test]
#[ignore]
fn lfn_fixture_opens() {
    if lfn_fixture_path().is_none() {
        eprintln!("skipped: CRYPTOVOL_STATIC_FAT_LFN_FIXTURE not set or file absent");
        return;
    }
    let (reader, opened) = open_lfn_fixture().unwrap();
    let data_reader = opened.data_reader(&reader).expect("data reader");
    FatFileSystem::open(&data_reader).expect("should parse FAT filesystem from LFN fixture");
}

#[test]
#[ignore]
fn lfn_fixture_root_listing() {
    let (reader, opened) = match open_lfn_fixture() {
        Some(v) => v,
        None => {
            eprintln!("skipped: CRYPTOVOL_STATIC_FAT_LFN_FIXTURE not set or file absent");
            return;
        }
    };
    let data_reader = opened.data_reader(&reader).expect("data reader");
    let fs = FatFileSystem::open(&data_reader).expect("FAT open");

    let entries = fs.list_dir("/").expect("root listing");
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();

    assert!(
        names.contains(&"Emoji Rocket \u{1F680} Test.txt"),
        "missing emoji rocket file; got {names:?}"
    );
    assert!(
        names.contains(&"Folder With Spaces"),
        "missing nested dir; got {names:?}"
    );
    assert!(
        names.contains(&"Please Do Not Open \u{1F605}.txt"),
        "missing sweat emoji file; got {names:?}"
    );
    assert!(
        names.contains(&"Project Notes Final.txt"),
        "missing project notes; got {names:?}"
    );
    assert!(
        names.contains(&"Sydney Sweeney at the 2025 Toronto International Film Festival.jpg"),
        "missing jpg; got {names:?}"
    );
    // The umlaut file may use decomposed sequences on disk — check by prefix
    let has_umlaut = names.iter().any(|n| n.starts_with("Unicode Umlaut "));
    assert!(has_umlaut, "missing Unicode Umlaut file; got {names:?}");
}

#[test]
#[ignore]
fn lfn_fixture_nested_listing() {
    let (reader, opened) = match open_lfn_fixture() {
        Some(v) => v,
        None => {
            eprintln!("skipped: CRYPTOVOL_STATIC_FAT_LFN_FIXTURE not set or file absent");
            return;
        }
    };
    let data_reader = opened.data_reader(&reader).expect("data reader");
    let fs = FatFileSystem::open(&data_reader).expect("FAT open");

    let entries = fs.list_dir("/Folder With Spaces").expect("nested listing");
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert!(
        names.contains(&"Rocket Science \u{1F680} For Beginners.txt"),
        "missing nested file; got {names:?}"
    );
}

#[test]
#[ignore]
fn lfn_fixture_extraction_hashes() {
    let (reader, opened) = match open_lfn_fixture() {
        Some(v) => v,
        None => {
            eprintln!("skipped: CRYPTOVOL_STATIC_FAT_LFN_FIXTURE not set or file absent");
            return;
        }
    };
    let data_reader = opened.data_reader(&reader).expect("data reader");
    let fs = FatFileSystem::open(&data_reader).expect("FAT open");

    // Discover the actual umlaut filename from the listing (preserves on-disk normalisation)
    let root_entries = fs.list_dir("/").expect("root listing");
    let umlaut_name = root_entries
        .iter()
        .find(|e| e.name.starts_with("Unicode Umlaut "))
        .map(|e| e.name.clone())
        .expect("umlaut file should be in root listing");

    let cases: &[(&str, &str)] = &[
        (
            "Emoji Rocket \u{1F680} Test.txt",
            "99d1c67af30cc495fe28836483c8fb86a41bea4a8510d9a39be4f3ae4075bd0e",
        ),
        (
            "Please Do Not Open \u{1F605}.txt",
            "ede269a0be0c6d600305a67e44b1fab1fa2c6741ab0f35856d4200afc486a9bc",
        ),
        (
            "Project Notes Final.txt",
            "3497b7fb6e5d93370e0212b439bb4516cbf7b03180ae33ac06401d6ba063463d",
        ),
        (
            "Sydney Sweeney at the 2025 Toronto International Film Festival.jpg",
            "e2ee40fdb8cb5dcff4a2870f7d773cb1979054bd55ee00245ac151251edd48d3",
        ),
    ];

    for (name, expected_hash) in cases {
        let path = format!("/{name}");
        let data = fs
            .read_file(&path)
            .unwrap_or_else(|e| panic!("read_file({path}) failed: {e}"));
        let hash = sha256_hex(&data);
        assert_eq!(
            hash, *expected_hash,
            "SHA-256 mismatch for {name}: got {hash}"
        );
        // Also compare byte-for-byte against ground truth
        let ground_truth = read_lfn_original(name);
        assert_eq!(data, ground_truth, "byte content mismatch for {name}");

        let mut streamed = Vec::new();
        let n = fs
            .read_file_to_writer(&path, &mut streamed)
            .unwrap_or_else(|e| panic!("read_file_to_writer({path}) failed: {e}"));
        assert_eq!(
            n as usize,
            streamed.len(),
            "streaming byte count mismatch for {name}"
        );
        assert_eq!(
            streamed, data,
            "streaming output must match read_file for {name}"
        );
    }

    // Umlaut file: use the discovered name
    let umlaut_path = format!("/{umlaut_name}");
    let umlaut_data = fs
        .read_file(&umlaut_path)
        .unwrap_or_else(|e| panic!("read_file({umlaut_path}) failed: {e}"));
    let umlaut_hash = sha256_hex(&umlaut_data);
    assert_eq!(
        umlaut_hash, "6cb589b1df71ebdb4ae405c1d33e806e2e23e2bc78e15c4a33835a12b905c87a",
        "SHA-256 mismatch for umlaut file"
    );
    // Compare against ground-truth using the filesystem-normalised name
    let umlaut_ground = read_lfn_original(&umlaut_name);
    assert_eq!(umlaut_data, umlaut_ground, "byte mismatch for umlaut file");

    let mut umlaut_streamed = Vec::new();
    let un = fs
        .read_file_to_writer(&umlaut_path, &mut umlaut_streamed)
        .unwrap_or_else(|e| panic!("read_file_to_writer({umlaut_path}) failed: {e}"));
    assert_eq!(
        un as usize,
        umlaut_streamed.len(),
        "streaming byte count mismatch for umlaut"
    );
    assert_eq!(
        umlaut_streamed, umlaut_data,
        "streaming output must match read_file for umlaut"
    );

    // Nested file
    let nested_path = "/Folder With Spaces/Rocket Science \u{1F680} For Beginners.txt";
    let nested_data = fs.read_file(nested_path).expect("nested file read");
    let nested_hash = sha256_hex(&nested_data);
    assert_eq!(
        nested_hash, "7a05f383d6f29a0456153fd0a4f6dbcba2ce48081d7aa4a8d410c2a644a79034",
        "SHA-256 mismatch for nested file"
    );
    let nested_ground =
        read_lfn_original("Folder With Spaces/Rocket Science \u{1F680} For Beginners.txt");
    assert_eq!(nested_data, nested_ground, "byte mismatch for nested file");

    let mut nested_streamed = Vec::new();
    let nn = fs
        .read_file_to_writer(nested_path, &mut nested_streamed)
        .expect("read_file_to_writer should succeed for nested file");
    assert_eq!(
        nn as usize,
        nested_streamed.len(),
        "streaming byte count mismatch for nested"
    );
    assert_eq!(
        nested_streamed, nested_data,
        "streaming output must match read_file for nested"
    );
}

#[test]
#[ignore]
fn lfn_fixture_decomposed_unicode_path() {
    let (reader, opened) = match open_lfn_fixture() {
        Some(v) => v,
        None => {
            eprintln!("skipped: CRYPTOVOL_STATIC_FAT_LFN_FIXTURE not set or file absent");
            return;
        }
    };
    let data_reader = opened.data_reader(&reader).expect("data reader");
    let fs = FatFileSystem::open(&data_reader).expect("FAT open");

    // Discover the exact on-disk name first
    let root_entries = fs.list_dir("/").expect("root listing");
    let umlaut_entry = root_entries
        .iter()
        .find(|e| e.name.starts_with("Unicode Umlaut "))
        .expect("umlaut file must be in root listing");
    let umlaut_name = umlaut_entry.name.clone();

    // Path lookup using the exact on-disk name (may be decomposed)
    let path = format!("/{umlaut_name}");
    let result = fs.read_file(&path);
    assert!(
        result.is_ok(),
        "decomposed unicode path lookup failed: {umlaut_name:?} -> {:?}",
        result.err()
    );

    let data = result.unwrap();
    let hash = sha256_hex(&data);
    assert_eq!(
        hash, "6cb589b1df71ebdb4ae405c1d33e806e2e23e2bc78e15c4a33835a12b905c87a",
        "SHA-256 mismatch via decomposed path"
    );

    let mut streamed = Vec::new();
    let n = fs
        .read_file_to_writer(&path, &mut streamed)
        .expect("read_file_to_writer should succeed via decomposed unicode path");
    assert_eq!(
        n as usize,
        streamed.len(),
        "streaming byte count mismatch via decomposed path"
    );
    assert_eq!(
        streamed, data,
        "streaming output must match read_file via decomposed path"
    );
}

#[test]
#[ignore]
fn lfn_fixture_wrong_password() {
    if lfn_fixture_path().is_none() {
        eprintln!("skipped: CRYPTOVOL_STATIC_FAT_LFN_FIXTURE not set or file absent");
        return;
    }
    let path = lfn_fixture_path().unwrap();
    let reader = FileBlockReader::open(&path).expect("open fixture file");
    let result = open_aes_sha512_volume(&reader, b"wrong-password");
    assert!(
        matches!(result, Err(TcvcOpenError::AuthenticationOrUnsupported)),
        "expected AuthenticationOrUnsupported, got {:?}",
        result
    );
}

#[test]
#[ignore]
fn lfn_fixture_unknown_path_not_found() {
    let (reader, opened) = match open_lfn_fixture() {
        Some(v) => v,
        None => {
            eprintln!("skipped: CRYPTOVOL_STATIC_FAT_LFN_FIXTURE not set or file absent");
            return;
        }
    };
    let data_reader = opened.data_reader(&reader).expect("data reader");
    let fs = FatFileSystem::open(&data_reader).expect("FAT open");
    let result = fs.read_file("/does-not-exist.txt");
    assert!(
        matches!(result, Err(FatError::PathNotFound { .. })),
        "expected PathNotFound, got {:?}",
        result
    );
}
