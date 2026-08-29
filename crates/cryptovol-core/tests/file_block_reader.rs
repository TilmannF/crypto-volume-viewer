#![allow(
    missing_docs,
    clippy::expect_used,
    reason = "block reader tests use direct setup assertions"
)]

use cryptovol_core::{BlockReader, CryptovolError, FileBlockReader};

#[test]
fn reads_exact_byte_range() {
    let file = tempfile::NamedTempFile::new().expect("temp file should be created");
    std::fs::write(file.path(), b"0123456789").expect("test file should be writable");
    let reader = FileBlockReader::open(file.path()).expect("reader should open");
    let mut buf = [0_u8; 4];

    reader.read_at(3, &mut buf).expect("read should succeed");

    assert_eq!(&buf, b"3456");
}

#[test]
fn reads_from_offset_zero() {
    let file = tempfile::NamedTempFile::new().expect("temp file should be created");
    std::fs::write(file.path(), b"abcdef").expect("test file should be writable");
    let reader = FileBlockReader::open(file.path()).expect("reader should open");
    let mut buf = [0_u8; 3];

    reader.read_at(0, &mut buf).expect("read should succeed");

    assert_eq!(&buf, b"abc");
}

#[test]
fn reads_near_eof() {
    let file = tempfile::NamedTempFile::new().expect("temp file should be created");
    std::fs::write(file.path(), b"abcdef").expect("test file should be writable");
    let reader = FileBlockReader::open(file.path()).expect("reader should open");
    let mut buf = [0_u8; 2];

    reader.read_at(4, &mut buf).expect("read should succeed");

    assert_eq!(&buf, b"ef");
}

#[test]
fn fails_when_read_starts_beyond_eof() {
    let file = tempfile::NamedTempFile::new().expect("temp file should be created");
    std::fs::write(file.path(), b"abc").expect("test file should be writable");
    let reader = FileBlockReader::open(file.path()).expect("reader should open");
    let mut buf = [0_u8; 1];

    let err = reader
        .read_at(4, &mut buf)
        .expect_err("read should fail beyond EOF");

    assert!(matches!(err, CryptovolError::OutOfBounds { .. }));
}

#[test]
fn fails_when_read_extends_beyond_eof() {
    let file = tempfile::NamedTempFile::new().expect("temp file should be created");
    std::fs::write(file.path(), b"abc").expect("test file should be writable");
    let reader = FileBlockReader::open(file.path()).expect("reader should open");
    let mut buf = [0_u8; 2];

    let err = reader
        .read_at(2, &mut buf)
        .expect_err("read should fail beyond EOF");

    assert!(matches!(err, CryptovolError::OutOfBounds { .. }));
}

#[test]
fn reports_correct_length() {
    let file = tempfile::NamedTempFile::new().expect("temp file should be created");
    std::fs::write(file.path(), b"abcdef").expect("test file should be writable");
    let reader = FileBlockReader::open(file.path()).expect("reader should open");

    assert_eq!(reader.len(), 6);
    assert!(!reader.is_empty());
}

#[test]
fn handles_empty_files() {
    let file = tempfile::NamedTempFile::new().expect("temp file should be created");
    let reader = FileBlockReader::open(file.path()).expect("reader should open");
    let mut buf = [];

    assert_eq!(reader.len(), 0);
    assert!(reader.is_empty());
    reader
        .read_at(0, &mut buf)
        .expect("empty read should succeed");
}
