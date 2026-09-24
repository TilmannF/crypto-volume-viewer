#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::panic,
    reason = "fixture tests assert against documented ground truth in docs/test-containers.md"
)]

//! Env-gated integration tests verifying the extraction job lifecycle:
//! `extract_file_impl` creates a job with the correct total bytes and
//! rejects directories without creating a job; `run_extraction_job` runs a
//! known file to completion emitting Started/Progress/Finished events and
//! writing the destination byte-for-byte, and removes the job from the
//! registry when done; cancelling mid-copy emits Cancelled (never
//! Finished), leaves no destination file, and also removes the job;
//! `cancel_extract_impl` on an unknown job id returns `job_not_found`
//! rather than panicking.
//!
//! `#[ignore]`d like `crates/cryptovol-app/tests/extract_file_fixtures.rs`,
//! so `cargo test --workspace --all-targets` passes without any static
//! fixtures present. Run with `CRYPTOVOL_STATIC_FAT_LFN_FIXTURE` set and
//! `-- --ignored`; see `docs/test-containers.md`.

use cryptovol_app::{open_volume, OpenVolumeRequest, TcvcKdf, VolumeSession};
use cryptovol_gui_lib::commands::extraction::{
    cancel_extract_impl, extract_file_impl, run_extraction_job,
};
use cryptovol_gui_lib::dto::extraction::ExtractFileRequestDto;
use cryptovol_gui_lib::events::ExtractionEvent;
use cryptovol_gui_lib::state::{GuiState, SessionId};
use secrecy::SecretString;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::time::Duration;

const TEST_PASSWORD: &str = "test-password";
const KNOWN_FILE: &str = "/Project Notes Final.txt";
const KNOWN_FILE_SIZE: u64 = 49;
const KNOWN_DIRECTORY: &str = "/Folder With Spaces";
const LARGE_FILE: &str = "/Sydney Sweeney at the 2025 Toronto International Film Festival.jpg";

fn fixture_path() -> Option<PathBuf> {
    let val = std::env::var("CRYPTOVOL_STATIC_FAT_LFN_FIXTURE").ok()?;
    let p = PathBuf::from(val);
    p.exists().then_some(p)
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("src-tauri crate should live three levels below workspace root")
        .to_path_buf()
}

fn ground_truth_bytes() -> Vec<u8> {
    let path = workspace_root()
        .join("testdata/static/fs-fat-lfn-original")
        .join("Project Notes Final.txt");
    std::fs::read(&path).expect("ground truth file should be readable")
}

fn open_fixture_session(path: &Path) -> VolumeSession {
    open_volume(OpenVolumeRequest {
        container_path: path.to_path_buf(),
        password: SecretString::from(TEST_PASSWORD.to_string()),
        pim: None,
        kdf_hint: Some(TcvcKdf::Sha512),
    })
    .expect("fixture should open with the documented test password")
}

fn request(
    session_id: &SessionId,
    source_path: &str,
    destination_path: &Path,
) -> ExtractFileRequestDto {
    ExtractFileRequestDto {
        session_id: session_id.as_str().to_string(),
        source_path: source_path.to_string(),
        destination_path: destination_path.display().to_string(),
        overwrite: false,
        parents: false,
    }
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_FAT_LFN_FIXTURE pointing to testdata/static/tcvc-aes-sha512-fat-lfn-unicode.hc"]
fn extract_file_impl_creates_a_job_with_known_total_bytes() {
    let Some(path) = fixture_path() else {
        eprintln!("skipped: CRYPTOVOL_STATIC_FAT_LFN_FIXTURE not set or absent");
        return;
    };
    let state = GuiState::default();
    let session_id = state.insert_session(open_fixture_session(&path));

    let dir = tempfile::TempDir::new().expect("temp dir");
    let dst = dir.path().join("out.txt");

    let started = extract_file_impl(&state, &request(&session_id, KNOWN_FILE, &dst))
        .expect("extract_file_impl should accept a known file")
        .started;

    assert_eq!(started.total_bytes, Some(KNOWN_FILE_SIZE));
    assert_eq!(started.session_id, session_id.as_str());
    assert!(!started.job_id.is_empty());
    assert_eq!(state.active_job_count(), 1);
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_FAT_LFN_FIXTURE pointing to testdata/static/tcvc-aes-sha512-fat-lfn-unicode.hc"]
fn extract_file_impl_rejects_a_directory_without_creating_a_job() {
    let Some(path) = fixture_path() else {
        eprintln!("skipped: CRYPTOVOL_STATIC_FAT_LFN_FIXTURE not set or absent");
        return;
    };
    let state = GuiState::default();
    let session_id = state.insert_session(open_fixture_session(&path));

    let dir = tempfile::TempDir::new().expect("temp dir");
    let dst = dir.path().join("should-not-be-created.txt");

    let err = extract_file_impl(&state, &request(&session_id, KNOWN_DIRECTORY, &dst))
        .expect_err("extracting a directory must be rejected");

    assert_eq!(err.code, "directory_extraction_unsupported");
    assert_eq!(state.active_job_count(), 0);
    assert!(!dst.exists());
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_FAT_LFN_FIXTURE pointing to testdata/static/tcvc-aes-sha512-fat-lfn-unicode.hc"]
fn run_extraction_job_writes_the_file_and_emits_started_progress_finished() {
    let Some(path) = fixture_path() else {
        eprintln!("skipped: CRYPTOVOL_STATIC_FAT_LFN_FIXTURE not set or absent");
        return;
    };
    let state = GuiState::default();
    let session_id = state.insert_session(open_fixture_session(&path));

    let dir = tempfile::TempDir::new().expect("temp dir");
    let dst = dir.path().join("out.txt");
    let req = request(&session_id, KNOWN_FILE, &dst);

    let job = extract_file_impl(&state, &req).expect("extract_file_impl should succeed");

    let mut events: Vec<ExtractionEvent> = Vec::new();
    run_extraction_job(&state, &job, &req, |event| events.push(event));

    assert_eq!(
        std::fs::read(&dst).expect("read extracted file"),
        ground_truth_bytes()
    );
    assert_eq!(
        state.active_job_count(),
        0,
        "job must be removed once finished"
    );

    assert!(
        matches!(events.first(), Some(ExtractionEvent::Started(e)) if e.total_bytes == Some(KNOWN_FILE_SIZE)),
        "expected a leading Started event with the known total, got {events:?}"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ExtractionEvent::Progress(_))),
        "expected at least one Progress event, got {events:?}"
    );
    assert!(
        matches!(events.last(), Some(ExtractionEvent::Finished(e)) if e.bytes_written == KNOWN_FILE_SIZE),
        "expected a trailing Finished event with the final count, got {events:?}"
    );
    assert!(
        !events.iter().any(|e| matches!(
            e,
            ExtractionEvent::Cancelled(_) | ExtractionEvent::Failed(_)
        )),
        "a successful extraction must not emit Cancelled/Failed, got {events:?}"
    );
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_FAT_LFN_FIXTURE pointing to testdata/static/tcvc-aes-sha512-fat-lfn-unicode.hc"]
fn run_extraction_job_cancelled_mid_copy_emits_cancelled_not_finished() {
    let Some(path) = fixture_path() else {
        eprintln!("skipped: CRYPTOVOL_STATIC_FAT_LFN_FIXTURE not set or absent");
        return;
    };
    let state = GuiState::default();
    let session_id = state.insert_session(open_fixture_session(&path));

    let dir = tempfile::TempDir::new().expect("temp dir");
    let dst = dir.path().join("cancelled.jpg");
    let req = request(&session_id, LARGE_FILE, &dst);

    let job = extract_file_impl(&state, &req).expect("extract_file_impl should succeed");

    // The token registered for this job, as cancel_extract would cancel it.
    let cancel_after_first_progress = job.cancellation_token.clone();
    let mut events: Vec<ExtractionEvent> = Vec::new();

    run_extraction_job(&state, &job, &req, |event| {
        if matches!(event, ExtractionEvent::Progress(_)) {
            cancel_after_first_progress.cancel();
        }
        events.push(event);
    });

    assert!(
        matches!(events.last(), Some(ExtractionEvent::Cancelled(_))),
        "expected a trailing Cancelled event, got {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, ExtractionEvent::Finished(_))),
        "no Finished event should fire on cancellation, got {events:?}"
    );
    assert!(
        !dst.exists(),
        "destination must not exist after a mid-extraction cancellation"
    );
    assert_eq!(
        state.active_job_count(),
        0,
        "job must be removed once cancelled"
    );
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_FAT_LFN_FIXTURE pointing to testdata/static/tcvc-aes-sha512-fat-lfn-unicode.hc"]
fn closing_the_session_mid_extraction_emits_exactly_one_terminal_event() {
    let Some(path) = fixture_path() else {
        eprintln!("skipped: CRYPTOVOL_STATIC_FAT_LFN_FIXTURE not set or absent");
        return;
    };
    let state = Arc::new(GuiState::default());
    let session_id = state.insert_session(open_fixture_session(&path));

    let dir = tempfile::TempDir::new().expect("temp dir");
    let dst = dir.path().join("closed.jpg");
    let req = request(&session_id, LARGE_FILE, &dst);

    let job = extract_file_impl(&state, &req).expect("extract_file_impl should succeed");

    let mut close_requested = false;
    let mut events: Vec<ExtractionEvent> = Vec::new();
    run_extraction_job(&state, &job, &req, |event| {
        if matches!(event, ExtractionEvent::Progress(_)) && !close_requested {
            close_requested = true;
            // Closes from another thread, as the frontend's close_session
            // command would, and waits for it: close must not block on the
            // running extraction.
            let (closed_tx, closed_rx) = mpsc::channel();
            let closing_state = Arc::clone(&state);
            let closing_id = session_id.clone();
            std::thread::spawn(move || {
                let _ = closed_tx.send(closing_state.close_session(&closing_id));
            });
            closed_rx
                .recv_timeout(Duration::from_secs(5))
                .unwrap_or_else(|_| panic!("close_session blocked on the running extraction"))
                .expect("close_session should succeed for an open session");
        }
        events.push(event);
    });

    let terminal: Vec<&ExtractionEvent> = events
        .iter()
        .filter(|e| {
            matches!(
                e,
                ExtractionEvent::Finished(_)
                    | ExtractionEvent::Cancelled(_)
                    | ExtractionEvent::Failed(_)
            )
        })
        .collect();
    assert_eq!(
        terminal.len(),
        1,
        "expected exactly one terminal event, got {events:?}"
    );
    assert!(
        matches!(terminal[0], ExtractionEvent::Cancelled(_)),
        "closing the session mid-copy must cancel the job, got {events:?}"
    );
    assert_eq!(
        state.active_job_count(),
        0,
        "job must be removed once cancelled"
    );
    assert!(
        !dst.exists(),
        "destination must not exist after the job was cancelled"
    );
    assert!(
        state.with_session(&session_id, |_| ()).is_none(),
        "the session must be closed"
    );
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_FAT_LFN_FIXTURE pointing to testdata/static/tcvc-aes-sha512-fat-lfn-unicode.hc"]
fn concurrent_jobs_to_the_same_destination_never_overwrite_each_other() {
    let Some(path) = fixture_path() else {
        eprintln!("skipped: CRYPTOVOL_STATIC_FAT_LFN_FIXTURE not set or absent");
        return;
    };
    let state = Arc::new(GuiState::default());
    let session_id = state.insert_session(open_fixture_session(&path));

    let dir = tempfile::TempDir::new().expect("temp dir");
    let dst = dir.path().join("same.txt");
    let req = request(&session_id, KNOWN_FILE, &dst);
    let jobs = [
        extract_file_impl(&state, &req).expect("first job should register"),
        extract_file_impl(&state, &req).expect("second job should register"),
    ];

    // Both jobs wait at their Started event (emitted after the open-time
    // destination check) until the other has also passed that check, so
    // both race to persist the same, initially absent destination.
    let started = Arc::new((Mutex::new(0_usize), Condvar::new()));
    let outcomes: Vec<Vec<ExtractionEvent>> = std::thread::scope(|scope| {
        let handles: Vec<_> = jobs
            .iter()
            .map(|job| {
                let state = Arc::clone(&state);
                let started = Arc::clone(&started);
                let req = &req;
                scope.spawn(move || {
                    let mut events = Vec::new();
                    run_extraction_job(&state, job, req, |event| {
                        if matches!(event, ExtractionEvent::Started(_)) {
                            let (count, all_started) = &*started;
                            let mut count = count.lock().expect("started count");
                            *count += 1;
                            all_started.notify_all();
                            let (_count, timeout) = all_started
                                .wait_timeout_while(count, Duration::from_secs(5), |n| *n < 2)
                                .expect("started count");
                            assert!(!timeout.timed_out(), "the other job never started");
                        }
                        events.push(event);
                    });
                    events
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("job thread"))
            .collect()
    });

    let finished = outcomes
        .iter()
        .filter(|events| matches!(events.last(), Some(ExtractionEvent::Finished(_))))
        .count();
    let refused = outcomes
        .iter()
        .filter(|events| {
            matches!(events.last(), Some(ExtractionEvent::Failed(e)) if e.code == "invalid_input")
        })
        .count();
    assert_eq!(
        (finished, refused),
        (1, 1),
        "expected one finished and one refused job, got {outcomes:?}"
    );
    assert_eq!(
        std::fs::read(&dst).expect("read extracted file"),
        ground_truth_bytes()
    );
    assert_eq!(
        std::fs::read_dir(dir.path()).expect("list dir").count(),
        1,
        "the refused job's temp file must be removed"
    );
    assert_eq!(state.active_job_count(), 0);
}

#[test]
fn cancel_extract_impl_on_unknown_job_returns_job_not_found() {
    let state = GuiState::default();
    let err = cancel_extract_impl(&state, "00000000-0000-0000-0000-000000000000")
        .expect_err("an unknown job id must error, not panic");
    assert_eq!(err.code, "job_not_found");
}
