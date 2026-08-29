#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "streaming writer tests build synthetic temp files with direct assertions"
)]

//! Verifies the destination-writing policy (overwrite/parents/symlink
//! refusal, atomic persist on finish, cleanup on drop) of the streaming
//! writer moved into `cryptovol-app`.

use cryptovol_app::{open_streaming_writer, WriteError};
use std::io::Write as _;

fn tmp() -> tempfile::TempDir {
    tempfile::TempDir::new().expect("tmp dir")
}

#[test]
fn refuses_dest_exists_without_overwrite() {
    let dir = tmp();
    let dst = dir.path().join("out.bin");
    std::fs::write(&dst, b"existing").unwrap();

    let result = open_streaming_writer(&dst, false, false);
    assert!(
        matches!(result, Err(WriteError::DestinationExists(_))),
        "expected DestinationExists, got {result:?}"
    );
}

#[test]
fn allows_overwrite_when_flag_set() {
    let dir = tmp();
    let dst = dir.path().join("out.bin");
    std::fs::write(&dst, b"old").unwrap();

    let mut writer = open_streaming_writer(&dst, true, false).expect("open");
    writer.write_all(b"new content").unwrap();
    writer.finish().expect("finish");

    assert_eq!(std::fs::read(&dst).unwrap(), b"new content");
}

#[test]
fn creates_parent_dirs_when_parents_flag() {
    let dir = tmp();
    let dst = dir.path().join("sub/dir/out.bin");

    let mut writer = open_streaming_writer(&dst, false, true).expect("open");
    writer.write_all(b"hello").unwrap();
    writer.finish().expect("finish");

    assert_eq!(std::fs::read(&dst).unwrap(), b"hello");
}

#[test]
fn returns_error_without_parents_flag() {
    let dir = tmp();
    let dst = dir.path().join("missing/out.bin");

    let result = open_streaming_writer(&dst, false, false);
    assert!(
        matches!(result, Err(WriteError::ParentDirectoryMissing(_))),
        "expected ParentDirectoryMissing, got {result:?}"
    );
}

#[cfg(unix)]
#[test]
fn rejects_symlink_destination() {
    let dir = tmp();
    let target = dir.path().join("target.bin");
    let dst = dir.path().join("link.bin");
    std::fs::write(&target, b"").unwrap();
    std::os::unix::fs::symlink(&target, &dst).unwrap();

    let result = open_streaming_writer(&dst, false, false);
    assert!(
        matches!(result, Err(WriteError::DestinationIsSymlink(_))),
        "expected DestinationIsSymlink, got {result:?}"
    );
}

#[test]
fn zero_byte_write_creates_empty_file() {
    let dir = tmp();
    let dst = dir.path().join("empty.bin");

    let writer = open_streaming_writer(&dst, false, false).expect("open");
    writer.finish().expect("finish");

    assert!(dst.exists(), "destination must exist after finish");
    assert_eq!(std::fs::read(&dst).unwrap(), b"");
}

#[test]
fn multi_chunk_write_accumulates_in_order() {
    let dir = tmp();
    let dst = dir.path().join("multi.bin");

    let mut writer = open_streaming_writer(&dst, false, false).expect("open");
    writer.write_all(b"chunk_one").unwrap();
    writer.write_all(b"chunk_two").unwrap();
    writer.write_all(b"chunk_three").unwrap();
    writer.finish().expect("finish");

    assert_eq!(
        std::fs::read(&dst).unwrap(),
        b"chunk_onechunk_twochunk_three"
    );
}

#[test]
fn drop_without_finish_leaves_no_dest() {
    let dir = tmp();
    let dst = dir.path().join("partial.bin");

    let mut writer = open_streaming_writer(&dst, false, false).expect("open");
    writer.write_all(b"partial data").unwrap();
    drop(writer); // no finish — temp file cleaned up, dst never written

    assert!(
        !dst.exists(),
        "destination must not exist after drop without finish"
    );
}
