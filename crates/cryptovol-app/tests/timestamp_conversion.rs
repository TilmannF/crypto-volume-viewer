#![allow(
    clippy::expect_used,
    reason = "timestamp conversion tests assert with direct expect() calls"
)]

//! Verifies that FAT, exFAT, and NTFS timestamps convert into the unified
//! `AppTimestamp` calendar representation without panicking on edge cases.

use cryptovol_app::AppTimestamp;
use cryptovol_fs_exfat::ExfatTimestamp;
use cryptovol_fs_fat::{FatDate, FatTime, FatTimestamp};
use cryptovol_fs_ntfs::NtfsTimestamp;

#[test]
fn fat_timestamp_converts_with_full_precision() {
    let fat = FatTimestamp {
        date: FatDate {
            year: 2024,
            month: 1,
            day: 15,
        },
        time: FatTime {
            hour: 12,
            minute: 34,
            second: 56,
        },
    };
    let app = AppTimestamp::from(fat);
    assert_eq!(app.year, 2024);
    assert_eq!(app.month, 1);
    assert_eq!(app.day, 15);
    assert_eq!(app.hour, 12);
    assert_eq!(app.minute, 34);
    assert_eq!(app.second, 56);
}

#[test]
fn fat_date_only_converts_with_zeroed_time() {
    // DirectoryEntry::accessed is FatDate-only (no time component).
    let date = FatDate {
        year: 2026,
        month: 6,
        day: 28,
    };
    let app = AppTimestamp::from(date);
    assert_eq!(app.year, 2026);
    assert_eq!(app.month, 6);
    assert_eq!(app.day, 28);
    assert_eq!(app.hour, 0);
    assert_eq!(app.minute, 0);
    assert_eq!(app.second, 0);
}

#[test]
fn exfat_timestamp_converts_directly_without_epoch_math() {
    let exfat = ExfatTimestamp {
        year: 2024,
        month: 1,
        day: 15,
        hour: 12,
        minute: 34,
        second: 56,
    };
    let app = AppTimestamp::from(exfat);
    assert_eq!(app.year, 2024);
    assert_eq!(app.month, 1);
    assert_eq!(app.day, 15);
    assert_eq!(app.hour, 12);
    assert_eq!(app.minute, 34);
    assert_eq!(app.second, 56);
}

#[test]
fn ntfs_timestamp_converts_known_post_epoch_seconds() {
    // 2024-01-15T12:34:56Z
    let ntfs = NtfsTimestamp {
        unix_seconds: 1_705_322_096,
    };
    let app = AppTimestamp::from_ntfs(&ntfs).expect("post-epoch timestamp must convert");
    assert_eq!(app.year, 2024);
    assert_eq!(app.month, 1);
    assert_eq!(app.day, 15);
    assert_eq!(app.hour, 12);
    assert_eq!(app.minute, 34);
    assert_eq!(app.second, 56);
}

#[test]
fn ntfs_timestamp_converts_known_leap_day() {
    // 2000-02-29T00:00:00Z
    let ntfs = NtfsTimestamp {
        unix_seconds: 951_782_400,
    };
    let app = AppTimestamp::from_ntfs(&ntfs).expect("leap-day timestamp must convert");
    assert_eq!(app.year, 2000);
    assert_eq!(app.month, 2);
    assert_eq!(app.day, 29);
    assert_eq!(app.hour, 0);
    assert_eq!(app.minute, 0);
    assert_eq!(app.second, 0);
}

#[test]
fn ntfs_timestamp_pre_1970_does_not_panic_and_returns_none() {
    // 1969-12-31T00:00:00Z
    let ntfs = NtfsTimestamp {
        unix_seconds: -86_400,
    };
    let app = AppTimestamp::from_ntfs(&ntfs);
    assert!(
        app.is_none(),
        "pre-1970 NTFS timestamps must convert to None, not panic or wrap"
    );
}
