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

use cryptovol_app::{open_volume, CancellationToken, OpenVolumeRequest, TcvcKdf, VolumeSession};
use cryptovol_gui_lib::commands::extraction::{
    cancel_extract_impl, extract_file_impl, run_extraction_job,
};
use cryptovol_gui_lib::dto::extraction::ExtractFileRequestDto;
use cryptovol_gui_lib::events::ExtractionEvent;
use cryptovol_gui_lib::state::{GuiState, SessionId};
use secrecy::SecretString;
use std::path::{Path, PathBuf};

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
        .expect("extract_file_impl should accept a known file");

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

    let started = extract_file_impl(&state, &req).expect("extract_file_impl should succeed");
    let job_id = cryptovol_gui_lib::state::JobId::from(started.job_id.as_str());

    let mut events: Vec<ExtractionEvent> = Vec::new();
    run_extraction_job(
        &state,
        &job_id,
        &session_id,
        &req,
        CancellationToken::new(),
        |event| events.push(event),
    );

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

    let started = extract_file_impl(&state, &req).expect("extract_file_impl should succeed");
    let job_id = cryptovol_gui_lib::state::JobId::from(started.job_id.as_str());

    let token = CancellationToken::new();
    let cancel_after_first_progress = token.clone();
    let mut events: Vec<ExtractionEvent> = Vec::new();

    run_extraction_job(&state, &job_id, &session_id, &req, token, |event| {
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
fn cancel_extract_impl_on_unknown_job_returns_job_not_found() {
    let state = GuiState::default();
    let err = cancel_extract_impl(&state, "00000000-0000-0000-0000-000000000000")
        .expect_err("an unknown job id must error, not panic");
    assert_eq!(err.code, "job_not_found");
}
