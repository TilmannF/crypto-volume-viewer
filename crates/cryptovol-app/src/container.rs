//! Password-free container inspection.

use crate::error::AppError;
use cryptovol_core::{BlockReader, FileBlockReader};
use cryptovol_tcvc::{
    inspect_header_candidates, CandidateReadStatus, HeaderCandidateState, TcvcInspection,
};
use std::path::{Path, PathBuf};

/// Safe, non-secret metadata about a container file, gathered without a
/// password and without attempting decryption.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerInfo {
    /// Path to the inspected container file.
    pub container_path: PathBuf,
    /// Container file size in bytes.
    pub container_size_bytes: u64,
    /// TC/VC header candidate inspection result.
    pub header_inspection: ContainerHeaderInspection,
}

/// Result of inspecting TC/VC header candidate locations in a container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerHeaderInspection {
    /// The container is too small to contain a header candidate.
    TooSmall {
        /// Minimum file size required for one candidate header.
        required_minimum: u64,
    },
    /// Primary and backup header candidate locations were identified.
    Candidates {
        /// Primary header candidate at the start of the container.
        primary: ContainerHeaderCandidate,
        /// Backup header candidate state.
        backup: ContainerBackupHeaderCandidate,
    },
}

/// Metadata about a readable header candidate location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerHeaderCandidate {
    /// Candidate byte offset in the container.
    pub offset: u64,
    /// Candidate length in bytes.
    pub length: u64,
    /// Whether the candidate bytes were readable.
    pub readable: bool,
}

/// Backup header candidate state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerBackupHeaderCandidate {
    /// A distinct backup candidate exists.
    Candidate(ContainerHeaderCandidate),
    /// The backup candidate would overlap the primary candidate.
    OverlapsPrimary,
}

/// Inspects a container file's size and TC/VC header candidate locations
/// without requiring a password and without attempting decryption.
///
/// # Errors
///
/// Returns [`AppError`] if the container file cannot be opened or its size
/// cannot be read.
pub fn inspect_container(path: impl AsRef<Path>) -> Result<ContainerInfo, AppError> {
    let path = path.as_ref();
    let reader = FileBlockReader::open(path)?;
    let container_size_bytes = reader.len();
    let inspection = inspect_header_candidates(&reader)?;

    Ok(ContainerInfo {
        container_path: path.to_path_buf(),
        container_size_bytes,
        header_inspection: inspection.into(),
    })
}

impl From<TcvcInspection> for ContainerHeaderInspection {
    fn from(inspection: TcvcInspection) -> Self {
        match inspection {
            TcvcInspection::TooSmall {
                required_minimum, ..
            } => ContainerHeaderInspection::TooSmall { required_minimum },
            TcvcInspection::Candidates { primary, backup } => {
                ContainerHeaderInspection::Candidates {
                    primary: ContainerHeaderCandidate {
                        offset: primary.offset,
                        length: primary.length,
                        readable: matches!(primary.read_status, CandidateReadStatus::Readable),
                    },
                    backup: backup.into(),
                }
            }
        }
    }
}

impl From<HeaderCandidateState> for ContainerBackupHeaderCandidate {
    fn from(state: HeaderCandidateState) -> Self {
        match state {
            HeaderCandidateState::Candidate {
                offset,
                length,
                read_status,
                ..
            } => ContainerBackupHeaderCandidate::Candidate(ContainerHeaderCandidate {
                offset,
                length,
                readable: matches!(read_status, CandidateReadStatus::Readable),
            }),
            HeaderCandidateState::OverlapsPrimary => {
                ContainerBackupHeaderCandidate::OverlapsPrimary
            }
        }
    }
}
