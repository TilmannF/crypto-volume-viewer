//! Managed GUI state: the opened-session registry and extraction-job registry.
//!
//! Owns an opaque, in-memory map from session id to
//! `cryptovol_app::VolumeSession` and from job id to the extraction job's
//! `cryptovol_app::CancellationToken`. It must never store the user's login
//! secret, derived keys, or raw decrypted data -- only the session/job
//! handles themselves. (A dedicated regression test enforces that this
//! source file never spells out that secret's usual name.)
//!
//! Locking is coarse-grained (one mutex for all sessions, one for all jobs):
//! acceptable for this GUI MVP, which only drives a single session at a
//! time, but a future multi-session UI may want per-session locking instead.

use crate::dto::error::{GuiErrorDto, JOB_NOT_FOUND, SESSION_NOT_FOUND};
use cryptovol_app::{CancellationToken, VolumeSession};
use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};
use uuid::Uuid;

/// Opaque session id. Never derived from the container path, the volume's
/// login secret, PIM, KDF, or timestamp -- only a random UUID.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionId(String);

impl SessionId {
    /// Generates a new, random session id.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    /// Returns this id's string representation.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl From<&str> for SessionId {
    /// Reconstructs a previously issued id from its wire string (e.g. one
    /// received from the frontend). Does not generate a new id.
    fn from(id: &str) -> Self {
        Self(id.to_string())
    }
}

/// Opaque extraction-job id. Never derived from the session id, source
/// path, destination path, or timestamp -- only a random UUID.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct JobId(String);

impl JobId {
    /// Generates a new, random job id.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    /// Returns this id's string representation.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for JobId {
    fn default() -> Self {
        Self::new()
    }
}

impl From<&str> for JobId {
    /// Reconstructs a previously issued id from its wire string (e.g. one
    /// received from the frontend). Does not generate a new id.
    fn from(id: &str) -> Self {
        Self(id.to_string())
    }
}

/// An in-flight extraction job's cancellation handle and owning session.
struct ExtractionJob {
    session_id: SessionId,
    cancellation_token: CancellationToken,
}

/// Tauri-managed application state: open volume sessions and in-flight
/// extraction jobs.
#[derive(Default)]
pub struct GuiState {
    sessions: Mutex<HashMap<SessionId, VolumeSession>>,
    extraction_jobs: Mutex<HashMap<JobId, ExtractionJob>>,
}

impl GuiState {
    /// Stores a newly opened `VolumeSession` and returns its new opaque id.
    #[must_use]
    pub fn insert_session(&self, session: VolumeSession) -> SessionId {
        let id = SessionId::new();
        lock(&self.sessions).insert(id.clone(), session);
        id
    }

    /// Runs `f` against the session for `id`, or returns `None` if `id` is
    /// unknown.
    pub fn with_session<T>(
        &self,
        id: &SessionId,
        f: impl FnOnce(&VolumeSession) -> T,
    ) -> Option<T> {
        lock(&self.sessions).get(id).map(f)
    }

    /// Closes the session for `id`: cancels and removes every extraction job
    /// owned by that session, then removes the session itself. Always
    /// succeeds for a known session id, even with active jobs.
    ///
    /// # Errors
    ///
    /// Returns a [`GuiErrorDto`] with code [`SESSION_NOT_FOUND`] if `id` is
    /// unknown.
    pub fn close_session(&self, id: &SessionId) -> Result<(), GuiErrorDto> {
        {
            let mut jobs = lock(&self.extraction_jobs);
            jobs.retain(|_job_id, job| {
                if &job.session_id == id {
                    job.cancellation_token.cancel();
                    false
                } else {
                    true
                }
            });
        }

        let mut sessions = lock(&self.sessions);
        if sessions.remove(id).is_some() {
            Ok(())
        } else {
            Err(GuiErrorDto::new(
                SESSION_NOT_FOUND,
                "no open session with that id",
            ))
        }
    }

    /// Registers a new extraction job owned by `session_id` and returns its
    /// new opaque id.
    #[must_use]
    pub fn insert_job(
        &self,
        session_id: SessionId,
        cancellation_token: CancellationToken,
    ) -> JobId {
        let id = JobId::new();
        lock(&self.extraction_jobs).insert(
            id.clone(),
            ExtractionJob {
                session_id,
                cancellation_token,
            },
        );
        id
    }

    /// Cancels and removes the extraction job for `id`.
    ///
    /// # Errors
    ///
    /// Returns a [`GuiErrorDto`] with code [`JOB_NOT_FOUND`] if `id` is
    /// unknown or the job has already finished and been removed.
    pub fn cancel_job(&self, id: &JobId) -> Result<(), GuiErrorDto> {
        match lock(&self.extraction_jobs).remove(id) {
            Some(job) => {
                job.cancellation_token.cancel();
                Ok(())
            }
            None => Err(GuiErrorDto::new(
                JOB_NOT_FOUND,
                "no extraction job with that id",
            )),
        }
    }

    /// Removes the extraction job for `id` without cancelling it, once it
    /// has finished on its own (successfully or with an error).
    pub fn remove_finished_job(&self, id: &JobId) {
        lock(&self.extraction_jobs).remove(id);
    }

    /// Returns the number of currently tracked (in-flight) extraction jobs.
    #[must_use]
    pub fn active_job_count(&self) -> usize {
        lock(&self.extraction_jobs).len()
    }

    /// Returns a clone of the `CancellationToken` stored for `id`, if it is
    /// still an active job. Used to hand the exact token `insert_job` stored
    /// to the background thread that actually performs the copy, so a later
    /// `cancel_job` call (which cancels the stored token) really does stop
    /// that copy rather than an unrelated token instance.
    #[must_use]
    pub fn job_cancellation_token(&self, id: &JobId) -> Option<CancellationToken> {
        lock(&self.extraction_jobs)
            .get(id)
            .map(|job| job.cancellation_token.clone())
    }
}

/// Locks `mutex`, recovering the inner guard if a prior panic poisoned it.
/// Acceptable here: `GuiState` only holds plain in-memory handles, so a
/// panic while a lock was held leaves no invariant worth failing future
/// requests over.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
