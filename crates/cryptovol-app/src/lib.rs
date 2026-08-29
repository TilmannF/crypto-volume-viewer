//! Framework-neutral application core for `cryptovol`.
//!
//! This crate owns volume opening, filesystem probing/dispatch, directory
//! listing, stat, and progress/cancellation-aware streaming extraction behind
//! a stable API shared by `cryptovol-cli` today and a future GUI later. It
//! never prompts for passwords and never prints to stdout/stderr.

mod cancellation;
mod container;
mod error;
mod extract;
mod model;
mod session;

pub use cancellation::CancellationToken;
pub use container::{
    inspect_container, ContainerBackupHeaderCandidate, ContainerHeaderCandidate,
    ContainerHeaderInspection, ContainerInfo,
};
pub use cryptovol_tcvc::{HeaderCandidateRole, PimState, TcvcKdf};
pub use error::{require_recognized_filesystem, AppError};
pub use extract::{
    copy_with_progress_and_cancellation, open_streaming_writer, ExtractOptions, ExtractSummary,
    ProgressEvent, StreamingWriter, WriteError,
};
pub use model::{AppTimestamp, FileAttributes, FileEntry, FilesystemKind};
pub use session::{open_volume, OpenVolumeRequest, VolumeInfo, VolumeSession};
