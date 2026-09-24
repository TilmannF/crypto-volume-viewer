//! Commands for starting, running, and cancelling a single-file extraction.
//!
//! `extract_file_impl` and `run_extraction_job` are plain, synchronous, and
//! independently testable: `extract_file_impl` validates the request and
//! registers a job without running the copy, and `run_extraction_job` runs
//! a copy to completion and reports events via a plain callback. The
//! `#[tauri::command]` wrappers are the only place that touch threading or
//! `AppHandle`/event emission, gluing the two together.

use crate::commands::run_blocking;
use crate::dto::error::{GuiErrorDto, DIRECTORY_EXTRACTION_UNSUPPORTED, SESSION_NOT_FOUND};
use crate::dto::extraction::{ExtractFileRequestDto, ExtractStartedDto};
use crate::events::{
    ExtractCancelledEvent, ExtractFailedEvent, ExtractFinishedEvent, ExtractProgressEvent,
    ExtractStartedEvent, ExtractionEvent,
};
use crate::state::{GuiState, JobId, SessionId};
use cryptovol_app::{AppError, CancellationToken, ExtractOptions, ProgressEvent, VolumeSession};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};

/// A validated, registered extraction job that has not started copying yet.
///
/// Carries the session handle and the exact `CancellationToken` stored in the
/// job registry, so the thread running the copy needs no further registry
/// lookups and a later `cancel_extract`/`close_session` stops that copy.
#[derive(Debug)]
pub struct PreparedExtraction {
    /// The response returned to the frontend.
    pub started: ExtractStartedDto,
    /// The registered job's id.
    pub job_id: JobId,
    /// The session that owns the job.
    pub session_id: SessionId,
    /// The session to extract from, kept alive for the copy's duration.
    pub session: Arc<VolumeSession>,
    /// The token stored for this job in the registry.
    pub cancellation_token: CancellationToken,
}

/// Validates `request` and registers a new extraction job, without starting
/// the copy itself. Returns immediately.
///
/// # Errors
///
/// Returns a [`GuiErrorDto`] with code `session_not_found` if
/// `request.session_id` is unknown or is closed before the job is
/// registered, `directory_extraction_unsupported` if `request.source_path`
/// names a directory, or another mapped [`AppError`] if `stat` itself fails.
pub fn extract_file_impl(
    state: &GuiState,
    request: &ExtractFileRequestDto,
) -> Result<PreparedExtraction, GuiErrorDto> {
    let session_id = SessionId::from(request.session_id.as_str());

    let session = state
        .session(&session_id)
        .ok_or_else(|| GuiErrorDto::new(SESSION_NOT_FOUND, "no open session with that id"))?;
    let entry = session
        .stat(&request.source_path)
        .map_err(GuiErrorDto::from)?;

    if entry.is_dir {
        return Err(GuiErrorDto::new(
            DIRECTORY_EXTRACTION_UNSUPPORTED,
            format!("cannot extract a directory: {}", request.source_path),
        ));
    }

    let cancellation_token = CancellationToken::new();
    let job_id = state.insert_job(session_id.clone(), cancellation_token.clone())?;

    Ok(PreparedExtraction {
        started: ExtractStartedDto {
            job_id: job_id.as_str().to_string(),
            session_id: session_id.as_str().to_string(),
            source_path: request.source_path.clone(),
            destination_path: request.destination_path.clone(),
            total_bytes: Some(entry.size),
        },
        job_id,
        session_id,
        session,
        cancellation_token,
    })
}

/// Runs a prepared extraction job to completion, reporting each lifecycle
/// event to `on_event`, and removes the job from `state`'s registry once it
/// reaches a terminal state. Every call emits exactly one terminal event:
/// `Finished`, `Cancelled` (also when the session is closed mid-copy, since
/// `close_session` cancels the job's token), or `Failed`.
pub fn run_extraction_job(
    state: &GuiState,
    job: &PreparedExtraction,
    request: &ExtractFileRequestDto,
    mut on_event: impl FnMut(ExtractionEvent),
) {
    let job_id = &job.job_id;
    let session_id = &job.session_id;
    let result = job.session.extract_file(
        &request.source_path,
        &request.destination_path,
        ExtractOptions {
            overwrite: request.overwrite,
            parents: request.parents,
            cancellation_token: Some(job.cancellation_token.clone()),
        },
        |event| on_event(map_progress_event(job_id, session_id, event)),
    );

    match result {
        Ok(_summary) => {}
        Err(AppError::Cancelled) => {
            on_event(ExtractionEvent::Cancelled(ExtractCancelledEvent {
                job_id: job_id.as_str().to_string(),
                session_id: session_id.as_str().to_string(),
            }));
        }
        Err(other) => {
            let gui_err = GuiErrorDto::from(other);
            on_event(ExtractionEvent::Failed(ExtractFailedEvent {
                job_id: job_id.as_str().to_string(),
                session_id: session_id.as_str().to_string(),
                code: gui_err.code,
                message: gui_err.message,
            }));
        }
    }
    state.remove_finished_job(job_id);
}

fn map_progress_event(
    job_id: &JobId,
    session_id: &SessionId,
    event: ProgressEvent,
) -> ExtractionEvent {
    match event {
        ProgressEvent::Started {
            source_path,
            destination_path,
            total_bytes,
        } => ExtractionEvent::Started(ExtractStartedEvent {
            job_id: job_id.as_str().to_string(),
            session_id: session_id.as_str().to_string(),
            source_path,
            destination_path: destination_path.display().to_string(),
            total_bytes,
        }),
        ProgressEvent::Advanced {
            bytes_written,
            total_bytes,
        } => ExtractionEvent::Progress(ExtractProgressEvent {
            job_id: job_id.as_str().to_string(),
            session_id: session_id.as_str().to_string(),
            bytes_written,
            total_bytes,
        }),
        ProgressEvent::Finished { bytes_written } => {
            ExtractionEvent::Finished(ExtractFinishedEvent {
                job_id: job_id.as_str().to_string(),
                session_id: session_id.as_str().to_string(),
                bytes_written,
            })
        }
    }
}

/// Emits `event` to the frontend under its wire event name. Tolerates the
/// frontend having no listeners; a send failure here must never corrupt
/// extraction state, so the result is deliberately ignored.
fn emit_extraction_event(app: &AppHandle, event: &ExtractionEvent) {
    let name = event.name();
    let _ = match event {
        ExtractionEvent::Started(payload) => app.emit(name, payload),
        ExtractionEvent::Progress(payload) => app.emit(name, payload),
        ExtractionEvent::Finished(payload) => app.emit(name, payload),
        ExtractionEvent::Cancelled(payload) => app.emit(name, payload),
        ExtractionEvent::Failed(payload) => app.emit(name, payload),
    };
}

/// Tauri command wrapper for [`extract_file_impl`]: validates the request
/// and registers the job on the blocking thread pool, then spawns a
/// background thread running [`run_extraction_job`] for that prepared job,
/// forwarding each event to the frontend.
#[tauri::command]
pub async fn extract_file(
    app: AppHandle,
    request: ExtractFileRequestDto,
) -> Result<ExtractStartedDto, GuiErrorDto> {
    let (job, request) = run_blocking(app.clone(), move |state| {
        extract_file_impl(state, &request).map(|job| (job, request))
    })
    .await?;
    let started = job.started.clone();

    std::thread::spawn(move || {
        let gui_state = app.state::<GuiState>();
        run_extraction_job(&gui_state, &job, &request, |event| {
            emit_extraction_event(&app, &event);
        });
    });

    Ok(started)
}

/// Cancels and removes the extraction job for `job_id`.
///
/// # Errors
///
/// Returns a [`GuiErrorDto`] with code `job_not_found` if `job_id` is
/// unknown or the job has already finished and been removed.
pub fn cancel_extract_impl(state: &GuiState, job_id: &str) -> Result<(), GuiErrorDto> {
    state.cancel_job(&JobId::from(job_id))
}

/// Tauri command wrapper for [`cancel_extract_impl`].
#[tauri::command]
pub fn cancel_extract(state: tauri::State<GuiState>, job_id: String) -> Result<(), GuiErrorDto> {
    cancel_extract_impl(&state, &job_id)
}
