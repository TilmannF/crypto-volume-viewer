#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "progress/cancellation tests build synthetic in-memory sources with direct assertions"
)]

//! Verifies that the streaming copy core reports Started/Advanced/Finished
//! progress and honors cancellation, using an in-memory source so no TC/VC
//! volume or fixture is required.

use cryptovol_app::{
    copy_with_progress_and_cancellation, open_streaming_writer, AppError, CancellationToken,
    ProgressEvent,
};
use std::io::{Cursor, Read};
use std::path::Path;

/// A `Read` wrapper that cancels `token` once at least `threshold` bytes
/// have been read through it, simulating a cancellation request that
/// arrives mid-extraction.
struct CancelAfter<R> {
    inner: R,
    threshold: usize,
    read_so_far: usize,
    token: CancellationToken,
}

impl<R: Read> Read for CancelAfter<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.read_so_far += n;
        if self.read_so_far >= self.threshold {
            self.token.cancel();
        }
        Ok(n)
    }
}

const SECRET_MARKER: &str = "do-not-leak-this-content-marker";

fn synthetic_source(total_bytes: usize) -> Vec<u8> {
    SECRET_MARKER.bytes().cycle().take(total_bytes).collect()
}

#[test]
fn reports_started_advanced_and_finished_with_byte_counts() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let dst = dir.path().join("out.bin");
    let source = synthetic_source(1024);
    let total_bytes = Some(source.len() as u64);

    let writer = open_streaming_writer(&dst, false, false).expect("open writer");

    let mut events = Vec::new();
    let summary = copy_with_progress_and_cancellation(
        Cursor::new(source.clone()),
        writer,
        "/source.bin",
        &dst,
        total_bytes,
        None,
        |event| events.push(event),
    )
    .expect("copy should succeed");

    assert_eq!(summary.bytes_written, source.len() as u64);
    assert_eq!(std::fs::read(&dst).expect("read dst"), source);

    assert!(
        matches!(
            events.first(),
            Some(ProgressEvent::Started { total_bytes: Some(n), .. }) if *n == source.len() as u64
        ),
        "expected a leading Started event with the known total, got {events:?}"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ProgressEvent::Advanced { .. })),
        "expected at least one Advanced event, got {events:?}"
    );
    assert!(
        matches!(
            events.last(),
            Some(ProgressEvent::Finished { bytes_written }) if *bytes_written == source.len() as u64
        ),
        "expected a trailing Finished event with the final count, got {events:?}"
    );

    let mut previous = 0u64;
    for event in &events {
        if let ProgressEvent::Advanced { bytes_written, .. } = event {
            assert!(
                *bytes_written >= previous,
                "Advanced bytes_written must be monotonically increasing"
            );
            previous = *bytes_written;
        }
    }
}

#[test]
fn progress_events_never_contain_source_content() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let dst = dir.path().join("out.bin");
    let source = synthetic_source(4096);

    let writer = open_streaming_writer(&dst, false, false).expect("open writer");

    let mut events = Vec::new();
    copy_with_progress_and_cancellation(
        Cursor::new(source),
        writer,
        "/source.bin",
        &dst,
        None,
        None,
        |event| events.push(event),
    )
    .expect("copy should succeed");

    for event in &events {
        let debug = format!("{event:?}");
        assert!(
            !debug.contains(SECRET_MARKER),
            "progress event Debug output must never contain source content: {debug}"
        );
    }
}

#[test]
fn cancellation_before_copy_starts_returns_cancelled_with_no_destination() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let dst = dir.path().join("out.bin");
    let source = synthetic_source(1024);

    let writer = open_streaming_writer(&dst, false, false).expect("open writer");
    let token = CancellationToken::new();
    token.cancel();

    let mut events = Vec::new();
    let result = copy_with_progress_and_cancellation(
        Cursor::new(source),
        writer,
        "/source.bin",
        &dst,
        None,
        Some(&token),
        |event| events.push(event),
    );

    assert!(
        matches!(result, Err(AppError::Cancelled)),
        "expected Cancelled, got {result:?}"
    );
    assert!(
        events.is_empty(),
        "no progress events should fire when already cancelled, got {events:?}"
    );
    assert!(
        !dst.exists(),
        "destination must not exist when cancelled before copying started"
    );
}

#[test]
fn cancellation_mid_copy_stops_early_and_leaves_no_destination() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let dst: std::path::PathBuf = dir.path().join("out.bin");
    // Larger than one EXTRACTION_CHUNK_SIZE (256 KiB) chunk so the copy loop
    // must perform more than one read, giving cancellation a chance to take
    // effect before the source is fully drained.
    let source = synthetic_source(300 * 1024);

    let writer = open_streaming_writer(&dst, false, false).expect("open writer");
    let token = CancellationToken::new();
    let cancelling_reader = CancelAfter {
        inner: Cursor::new(source.clone()),
        threshold: 1,
        read_so_far: 0,
        token: token.clone(),
    };

    let mut events = Vec::new();
    let result = copy_with_progress_and_cancellation(
        cancelling_reader,
        writer,
        "/source.bin",
        &dst,
        Some(source.len() as u64),
        Some(&token),
        |event| events.push(event),
    );

    assert!(
        matches!(result, Err(AppError::Cancelled)),
        "expected Cancelled, got {result:?}"
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, ProgressEvent::Finished { .. })),
        "no Finished event should fire on cancellation, got {events:?}"
    );
    assert!(
        !dst.exists(),
        "destination must not exist after a mid-copy cancellation"
    );
    assert!(
        !Path::new(dir.path())
            .read_dir()
            .expect("read temp dir")
            .any(|entry| entry.expect("dir entry").path() == dst),
        "no leftover destination-named file should remain in the destination directory"
    );
}
