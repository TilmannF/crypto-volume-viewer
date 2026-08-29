//! Commands for starting, running, and cancelling a single-file extraction.
//!
//! `extract_file_impl` and `run_extraction_job` are plain, synchronous, and
//! independently testable: `extract_file_impl` validates the request and
//! registers a job without running the copy, and `run_extraction_job` runs
//! a copy to completion and reports events via a plain callback. The
//! `#[tauri::command]` wrappers are the only place that touch threading or
//! `AppHandle`/event emission, gluing the two together.

use crate::dto::error::{GuiErrorDto, DIRECTORY_EXTRACTION_UNSUPPORTED, SESSION_NOT_FOUND};
use crate::dto::extraction::{ExtractFileRequestDto, ExtractStartedDto};
use crate::events::{
    ExtractCancelledEvent, ExtractFailedEvent, ExtractFinishedEvent, ExtractProgressEvent,
    ExtractStartedEvent, ExtractionEvent,
};
use crate::state::{GuiState, JobId, SessionId};
use cryptovol_app::{AppError, CancellationToken, ExtractOptions, ProgressEvent};
use tauri::{AppHandle, Emitter, Manager};

/// Validates `request` and registers a new extraction job, without starting
/// the copy itself. Returns immediately.
///
/// # Errors
///
/// Returns a [`GuiErrorDto`] with code `session_not_found` if
/// `request.session_id` is unknown, `directory_extraction_unsupported` if
/// `request.source_path` names a directory, or another mapped
/// [`AppError`] if `stat` itself fails.
pub fn extract_file_impl(
    state: &GuiState,
    request: &ExtractFileRequestDto,
) -> Result<ExtractStartedDto, GuiErrorDto> {
    let session_id = SessionId::from(request.session_id.as_str());

    let entry = state
        .with_session(&session_id, |session| session.stat(&request.source_path))
        .ok_or_else(|| GuiErrorDto::new(SESSION_NOT_FOUND, "no open session with that id"))?
        .map_err(GuiErrorDto::from)?;

    if entry.is_dir {
        return Err(GuiErrorDto::new(
            DIRECTORY_EXTRACTION_UNSUPPORTED,
            format!("cannot extract a directory: {}", request.source_path),
        ));
    }

    let job_id = state.insert_job(session_id.clone(), CancellationToken::new());

    Ok(ExtractStartedDto {
        job_id: job_id.as_str().to_string(),
        session_id: session_id.as_str().to_string(),
        source_path: request.source_path.clone(),
        destination_path: request.destination_path.clone(),
        total_bytes: Some(entry.size),
    })
}

/// Runs the extraction for an already-registered job to completion,
/// reporting each lifecycle event to `on_event`, and removes the job from
/// `state`'s registry once it reaches a terminal state (finished, cancelled,
/// or failed). Does nothing if the session has since been closed.
pub fn run_extraction_job(
    state: &GuiState,
    job_id: &JobId,
    session_id: &SessionId,
    request: &ExtractFileRequestDto,
    cancellation_token: CancellationToken,
    mut on_event: impl FnMut(ExtractionEvent),
) {
    let result = state.with_session(session_id, |session| {
        session.extract_file(
            &request.source_path,
            &request.destination_path,
            ExtractOptions {
                overwrite: request.overwrite,
                parents: request.parents,
                cancellation_token: Some(cancellation_token),
            },
            |event| on_event(map_progress_event(job_id, session_id, event)),
        )
    });

    match result {
        Some(Ok(_summary)) => state.remove_finished_job(job_id),
        Some(Err(AppError::Cancelled)) => {
            on_event(ExtractionEvent::Cancelled(ExtractCancelledEvent {
                job_id: job_id.as_str().to_string(),
                session_id: session_id.as_str().to_string(),
            }));
            state.remove_finished_job(job_id);
        }
        Some(Err(other)) => {
            let gui_err = GuiErrorDto::from(other);
            on_event(ExtractionEvent::Failed(ExtractFailedEvent {
                job_id: job_id.as_str().to_string(),
                session_id: session_id.as_str().to_string(),
                code: gui_err.code,
                message: gui_err.message,
            }));
            state.remove_finished_job(job_id);
        }
        // The session was closed mid-extraction; close_session already
        // cancelled and removed this job, so there is nothing left to do.
        None => {}
    }
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
/// and registers the job synchronously, then spawns a background thread
/// running [`run_extraction_job`] against the exact `CancellationToken`
/// `extract_file_impl` stored, forwarding each event to the frontend.
#[tauri::command]
pub fn extract_file(
    app: AppHandle,
    state: tauri::State<GuiState>,
    request: ExtractFileRequestDto,
) -> Result<ExtractStartedDto, GuiErrorDto> {
    let started = extract_file_impl(&state, &request)?;

    let job_id = JobId::from(started.job_id.as_str());
    let session_id = SessionId::from(started.session_id.as_str());

    if let Some(token) = state.job_cancellation_token(&job_id) {
        let app_for_thread = app.clone();
        std::thread::spawn(move || {
            let gui_state = app_for_thread.state::<GuiState>();
            let app_for_emit = app_for_thread.clone();
            run_extraction_job(
                &gui_state,
                &job_id,
                &session_id,
                &request,
                token,
                move |event| emit_extraction_event(&app_for_emit, &event),
            );
        });
    }

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
