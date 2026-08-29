#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "fixture tests use direct setup assertions"
)]

use std::path::PathBuf;

use cryptovol_core::FileBlockReader;
use cryptovol_fs_fat::{FatError, FatFileSystem};
use cryptovol_tcvc::{open_aes_sha512_volume, TcvcOpenError};

/// Return the fixture path from CRYPTOVOL_STATIC_FAT_FIXTURE, or None if unset/absent.
fn fixture_path() -> Option<PathBuf> {
    let val = std::env::var("CRYPTOVOL_STATIC_FAT_FIXTURE").ok()?;
    let p = PathBuf::from(val);
    if p.exists() {
        Some(p)
    } else {
        None
    }
}

/// Read a ground-truth file from testdata/static/fs-fat-original/ relative to project root.
/// crate manifest is at crates/cryptovol-fs-fat/Cargo.toml, so go up two levels.
fn read_original(name: &str) -> Vec<u8> {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest
        .join("../../testdata/static/fs-fat-original")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("cannot read ground truth {name}: {e}"))
}

#[test]
#[ignore]
fn fixture_root_listing() {
    let path = match fixture_path() {
        Some(p) => p,
        None => {
            eprintln!("skipped: CRYPTOVOL_STATIC_FAT_FIXTURE not set or file absent");
            return;
        }
    };

    let reader = FileBlockReader::open(&path).expect("should open fixture");
    let opened =
        open_aes_sha512_volume(&reader, b"test-password").expect("should open with test-password");
    let data_reader = opened
        .data_reader(&reader)
        .expect("should create data reader");
    let fs = FatFileSystem::open(&data_reader).expect("should parse FAT filesystem");

    let entries = fs.list_dir("/").expect("root listing should succeed");
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();

    assert!(
        names.contains(&"HELLO.TXT"),
        "missing HELLO.TXT; got {names:?}"
    );
    assert!(
        names.contains(&"SYDNEY.JPG"),
        "missing SYDNEY.JPG; got {names:?}"
    );
    assert!(names.contains(&"DIR"), "missing DIR; got {names:?}");

    let hello = entries.iter().find(|e| e.name == "HELLO.TXT").unwrap();
    assert!(!hello.is_dir, "HELLO.TXT should be a file");
    assert_eq!(hello.size, 36, "HELLO.TXT size mismatch");

    let jpg = entries.iter().find(|e| e.name == "SYDNEY.JPG").unwrap();
    assert!(!jpg.is_dir, "SYDNEY.JPG should be a file");
    assert_eq!(jpg.size, 89489, "SYDNEY.JPG size mismatch");

    let dir = entries.iter().find(|e| e.name == "DIR").unwrap();
    assert!(dir.is_dir, "DIR should be a directory");
}

#[test]
#[ignore]
fn fixture_nested_listing() {
    let path = match fixture_path() {
        Some(p) => p,
        None => {
            eprintln!("skipped: CRYPTOVOL_STATIC_FAT_FIXTURE not set or file absent");
            return;
        }
    };

    let reader = FileBlockReader::open(&path).expect("should open fixture");
    let opened =
        open_aes_sha512_volume(&reader, b"test-password").expect("should open with test-password");
    let data_reader = opened
        .data_reader(&reader)
        .expect("should create data reader");
    let fs = FatFileSystem::open(&data_reader).expect("should parse FAT filesystem");

    let entries = fs.list_dir("/DIR").expect("/DIR listing should succeed");
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();

    assert!(
        names.contains(&"NESTED.TXT"),
        "missing NESTED.TXT; got {names:?}"
    );

    let nested = entries.iter().find(|e| e.name == "NESTED.TXT").unwrap();
    assert!(!nested.is_dir, "NESTED.TXT should be a file");
    assert_eq!(nested.size, 17, "NESTED.TXT size mismatch");
}

#[test]
#[ignore]
fn fixture_wrong_password() {
    let path = match fixture_path() {
        Some(p) => p,
        None => {
            eprintln!("skipped: CRYPTOVOL_STATIC_FAT_FIXTURE not set or file absent");
            return;
        }
    };

    let reader = FileBlockReader::open(&path).expect("should open fixture");
    let err =
        open_aes_sha512_volume(&reader, b"wrong-password").expect_err("wrong password should fail");
    assert!(
        matches!(err, TcvcOpenError::AuthenticationOrUnsupported),
        "expected AuthenticationOrUnsupported, got {err:?}"
    );
}

#[test]
#[ignore]
fn fixture_path_not_found() {
    let path = match fixture_path() {
        Some(p) => p,
        None => {
            eprintln!("skipped: CRYPTOVOL_STATIC_FAT_FIXTURE not set or file absent");
            return;
        }
    };

    let reader = FileBlockReader::open(&path).expect("should open fixture");
    let opened =
        open_aes_sha512_volume(&reader, b"test-password").expect("should open with test-password");
    let data_reader = opened
        .data_reader(&reader)
        .expect("should create data reader");
    let fs = FatFileSystem::open(&data_reader).expect("should parse FAT filesystem");

    let err = fs
        .list_dir("/MISSING")
        .expect_err("/MISSING should not exist");
    assert!(
        matches!(err, FatError::PathNotFound { .. }),
        "expected PathNotFound, got {err}"
    );
}

#[test]
#[ignore]
fn fixture_extract_hello_txt() {
    let path = match fixture_path() {
        Some(p) => p,
        None => {
            eprintln!("skipped: CRYPTOVOL_STATIC_FAT_FIXTURE not set or file absent");
            return;
        }
    };
    let reader = FileBlockReader::open(&path).expect("should open fixture");
    let opened =
        open_aes_sha512_volume(&reader, b"test-password").expect("should open with test-password");
    let data_reader = opened
        .data_reader(&reader)
        .expect("should create data reader");
    let fs = FatFileSystem::open(&data_reader).expect("should parse FAT filesystem");

    let bytes = fs
        .read_file("/HELLO.TXT")
        .expect("read_file should succeed");
    assert_eq!(bytes.len(), 36, "HELLO.TXT size mismatch");
    assert_eq!(
        bytes,
        read_original("HELLO.TXT"),
        "HELLO.TXT content mismatch"
    );

    let mut streamed = Vec::new();
    let n = fs
        .read_file_to_writer("/HELLO.TXT", &mut streamed)
        .expect("read_file_to_writer should succeed for HELLO.TXT");
    assert_eq!(
        n as usize,
        streamed.len(),
        "HELLO.TXT streaming byte count mismatch"
    );
    assert_eq!(
        streamed, bytes,
        "HELLO.TXT streaming output must match read_file"
    );
}

#[test]
#[ignore]
fn fixture_extract_nested_txt() {
    let path = match fixture_path() {
        Some(p) => p,
        None => {
            eprintln!("skipped: CRYPTOVOL_STATIC_FAT_FIXTURE not set or file absent");
            return;
        }
    };
    let reader = FileBlockReader::open(&path).expect("should open fixture");
    let opened =
        open_aes_sha512_volume(&reader, b"test-password").expect("should open with test-password");
    let data_reader = opened
        .data_reader(&reader)
        .expect("should create data reader");
    let fs = FatFileSystem::open(&data_reader).expect("should parse FAT filesystem");

    let bytes = fs
        .read_file("/DIR/NESTED.TXT")
        .expect("read_file should succeed");
    assert_eq!(bytes.len(), 17, "NESTED.TXT size mismatch");
    assert_eq!(
        bytes,
        read_original("DIR/NESTED.TXT"),
        "NESTED.TXT content mismatch"
    );

    let mut streamed = Vec::new();
    let n = fs
        .read_file_to_writer("/DIR/NESTED.TXT", &mut streamed)
        .expect("read_file_to_writer should succeed for NESTED.TXT");
    assert_eq!(
        n as usize,
        streamed.len(),
        "NESTED.TXT streaming byte count mismatch"
    );
    assert_eq!(
        streamed, bytes,
        "NESTED.TXT streaming output must match read_file"
    );
}

#[test]
#[ignore]
fn fixture_extract_sydney_jpg() {
    let path = match fixture_path() {
        Some(p) => p,
        None => {
            eprintln!("skipped: CRYPTOVOL_STATIC_FAT_FIXTURE not set or file absent");
            return;
        }
    };
    let reader = FileBlockReader::open(&path).expect("should open fixture");
    let opened =
        open_aes_sha512_volume(&reader, b"test-password").expect("should open with test-password");
    let data_reader = opened
        .data_reader(&reader)
        .expect("should create data reader");
    let fs = FatFileSystem::open(&data_reader).expect("should parse FAT filesystem");

    let bytes = fs
        .read_file("/SYDNEY.JPG")
        .expect("read_file should succeed");
    assert_eq!(bytes.len(), 89489, "SYDNEY.JPG size mismatch");
    assert_eq!(
        bytes,
        read_original("SYDNEY.JPG"),
        "SYDNEY.JPG content mismatch"
    );

    let mut streamed = Vec::new();
    let n = fs
        .read_file_to_writer("/SYDNEY.JPG", &mut streamed)
        .expect("read_file_to_writer should succeed for SYDNEY.JPG");
    assert_eq!(
        n as usize,
        streamed.len(),
        "SYDNEY.JPG streaming byte count mismatch"
    );
    assert_eq!(
        streamed, bytes,
        "SYDNEY.JPG streaming output must match read_file"
    );
}

#[test]
#[ignore]
fn fixture_extract_dir_rejected() {
    let path = match fixture_path() {
        Some(p) => p,
        None => {
            eprintln!("skipped: CRYPTOVOL_STATIC_FAT_FIXTURE not set or file absent");
            return;
        }
    };
    let reader = FileBlockReader::open(&path).expect("should open fixture");
    let opened =
        open_aes_sha512_volume(&reader, b"test-password").expect("should open with test-password");
    let data_reader = opened
        .data_reader(&reader)
        .expect("should create data reader");
    let fs = FatFileSystem::open(&data_reader).expect("should parse FAT filesystem");

    let err = fs
        .read_file("/DIR")
        .expect_err("reading a directory should fail");
    assert!(
        matches!(err, FatError::IsADirectory { .. }),
        "expected IsADirectory, got {err}"
    );
}

#[test]
#[ignore]
fn fixture_extract_unknown_path() {
    let path = match fixture_path() {
        Some(p) => p,
        None => {
            eprintln!("skipped: CRYPTOVOL_STATIC_FAT_FIXTURE not set or file absent");
            return;
        }
    };
    let reader = FileBlockReader::open(&path).expect("should open fixture");
    let opened =
        open_aes_sha512_volume(&reader, b"test-password").expect("should open with test-password");
    let data_reader = opened
        .data_reader(&reader)
        .expect("should create data reader");
    let fs = FatFileSystem::open(&data_reader).expect("should parse FAT filesystem");

    let err = fs
        .read_file("/DOESNOTEXIST.TXT")
        .expect_err("nonexistent path should fail");
    assert!(
        matches!(err, FatError::PathNotFound { .. }),
        "expected PathNotFound, got {err}"
    );
}
