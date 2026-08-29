#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::panic,
    reason = "CLI rendering tests use direct assertions"
)]

use cryptovol_cli::{render_ntfs_entry, render_ntfs_entry_long};
use cryptovol_fs_ntfs::{NtfsAttributes, NtfsEntry, NtfsTimestamp};

fn ntfs_entry(name: &str, is_dir: bool, size: u64) -> NtfsEntry {
    NtfsEntry {
        name: name.to_string(),
        is_dir,
        size,
        attributes: NtfsAttributes::default(),
        created: None,
        modified: None,
        accessed: None,
    }
}

#[test]
fn render_ntfs_entry_file() {
    let entry = ntfs_entry("hello.txt", false, 5);

    let rendered = render_ntfs_entry(&entry);

    assert!(
        rendered.contains("hello.txt"),
        "rendered entry should contain filename: {rendered}"
    );
    assert!(
        rendered.contains('5'),
        "rendered entry should contain file size: {rendered}"
    );
}

#[test]
fn render_ntfs_entry_dir() {
    let entry = ntfs_entry("Folder With Spaces", true, 0);

    let rendered = render_ntfs_entry(&entry);

    assert!(
        rendered.starts_with('d'),
        "directory entry should start with d: {rendered}"
    );
}

#[test]
fn render_ntfs_entry_long_file() {
    let mut entry = ntfs_entry("hello.txt", false, 5);
    entry.modified = Some(NtfsTimestamp {
        unix_seconds: 1_704_067_200,
    });

    let rendered = render_ntfs_entry_long(&entry);

    assert!(
        rendered.starts_with('-'),
        "file entry should start with -: {rendered}"
    );
    assert!(
        rendered.contains("hello.txt"),
        "long entry should contain filename: {rendered}"
    );
    assert!(
        rendered.contains('5'),
        "long entry should contain file size: {rendered}"
    );
    assert!(
        rendered.contains("2024-01-01"),
        "long entry should contain UTC date: {rendered}"
    );
}

#[test]
fn render_ntfs_entry_long_dir() {
    let entry = ntfs_entry("Documents", true, 0);

    let rendered = render_ntfs_entry_long(&entry);

    assert!(
        rendered.starts_with('d'),
        "directory long entry should start with d: {rendered}"
    );
}

#[test]
fn render_ntfs_entry_long_no_timestamp() {
    let entry = ntfs_entry("hello.txt", false, 5);

    let rendered = render_ntfs_entry_long(&entry);

    assert!(
        rendered.contains("hello.txt"),
        "long entry without timestamp should contain filename: {rendered}"
    );
}

#[test]
fn render_ntfs_entry_unicode_emoji() {
    let entry = ntfs_entry("Rocket Science 🚀 For Beginners.txt", false, 13);

    let rendered = render_ntfs_entry(&entry);

    assert!(
        rendered.contains('🚀'),
        "rendered entry should preserve emoji filename: {rendered}"
    );
}
