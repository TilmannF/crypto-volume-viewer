//! Application-level error type shared by every `cryptovol-app` operation.

use cryptovol_core::CryptovolError;
use cryptovol_fs_exfat::ExfatError;
use cryptovol_fs_fat::FatError;
use cryptovol_fs_ntfs::NtfsError;
use cryptovol_tcvc::{FilesystemProbeCandidate, TcvcOpenError};

/// Errors returned by `cryptovol-app` operations.
///
/// Display/Debug output never includes passwords, derived key material, or
/// decrypted file contents.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// A host filesystem or container read/write operation failed.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// Authentication failed, or the volume uses unsupported open parameters.
    #[error("authentication failed or unsupported parameters")]
    AuthFailed,
    /// The container or volume profile is outside current support.
    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),
    /// The decrypted volume's filesystem could not be recognized.
    #[error("filesystem not recognized")]
    FilesystemNotRecognized,
    /// The requested container path does not exist.
    #[error("path not found: {0}")]
    PathNotFound(String),
    /// The caller attempted to extract a directory as if it were a file.
    #[error("cannot extract a directory: {0}")]
    DirectoryExtractionUnsupported(String),
    /// The requested operation is not supported by this filesystem or backend.
    #[error("unsupported feature: {0}")]
    UnsupportedFeature(String),
    /// Extraction failed after the volume and filesystem were opened.
    #[error("extraction failed: {0}")]
    ExtractionFailed(String),
    /// The caller cancelled the operation via a `CancellationToken`.
    #[error("operation cancelled")]
    Cancelled,
    /// The caller supplied a request that cannot be fulfilled as given.
    #[error("invalid input: {0}")]
    InvalidInput(String),
}

impl From<CryptovolError> for AppError {
    fn from(err: CryptovolError) -> Self {
        AppError::Io(std::io::Error::other(err.to_string()))
    }
}

impl From<TcvcOpenError> for AppError {
    fn from(err: TcvcOpenError) -> Self {
        match err {
            TcvcOpenError::AuthenticationOrUnsupported | TcvcOpenError::InvalidPim { .. } => {
                AppError::AuthFailed
            }
            TcvcOpenError::UnsupportedProfile { reason } => {
                AppError::UnsupportedFormat(reason.to_string())
            }
            TcvcOpenError::ReadFailed { error } => AppError::Io(std::io::Error::other(error)),
        }
    }
}

impl From<FatError> for AppError {
    fn from(err: FatError) -> Self {
        // Most FAT structural/read errors are only meaningful in the context
        // of the caller's operation (opening vs. listing vs. extracting), so
        // call sites that need that distinction should map them explicitly
        // instead of relying on this generic conversion.
        let message = err.to_string();
        match err {
            FatError::PathNotFound { path } => AppError::PathNotFound(path),
            FatError::IsADirectory { path } => AppError::DirectoryExtractionUnsupported(path),
            FatError::NotADirectory { path } => {
                AppError::InvalidInput(format!("not a directory: {path}"))
            }
            FatError::WriteError(io_err) => AppError::ExtractionFailed(io_err.to_string()),
            FatError::InvalidBootSector
            | FatError::UnsupportedFatType
            | FatError::MalformedEntry
            | FatError::ReadError(_)
            | FatError::OutOfBounds
            | FatError::SizeOverflow
            | FatError::ClusterChainCycle
            | FatError::ClusterChainTooShort
            | FatError::InvalidCluster { .. } => AppError::ExtractionFailed(message),
        }
    }
}

impl From<ExfatError> for AppError {
    fn from(err: ExfatError) -> Self {
        let message = err.to_string();
        match err {
            ExfatError::PathNotFound { path } => AppError::PathNotFound(path),
            ExfatError::AttemptedDirectoryExtraction { path } => {
                AppError::DirectoryExtractionUnsupported(path)
            }
            ExfatError::WriteError(io_err) => AppError::ExtractionFailed(io_err.to_string()),
            ExfatError::InvalidBootSector { .. }
            | ExfatError::InvalidClusterNumber { .. }
            | ExfatError::InvalidClusterChain
            | ExfatError::FatChainCycle
            | ExfatError::MalformedEntrySet
            | ExfatError::MissingStreamExtension
            | ExfatError::InvalidFilenameEntry
            | ExfatError::InvalidUtf16
            | ExfatError::OutOfBoundsRead
            | ExfatError::UnsupportedFilesystem
            | ExfatError::ReadError(_) => AppError::ExtractionFailed(message),
        }
    }
}

impl From<NtfsError> for AppError {
    fn from(err: NtfsError) -> Self {
        let message = err.to_string();
        match err {
            NtfsError::PathNotFound { path } => AppError::PathNotFound(path),
            NtfsError::AttemptedDirectoryExtraction { path } => {
                AppError::DirectoryExtractionUnsupported(path)
            }
            NtfsError::WriteError(io_err) => AppError::ExtractionFailed(io_err.to_string()),
            NtfsError::InvalidBootSector { .. }
            | NtfsError::InvalidMftLocation
            | NtfsError::InvalidFileRecord
            | NtfsError::FixupValidationFailed
            | NtfsError::MalformedAttribute
            | NtfsError::UnsupportedAttributeList
            | NtfsError::MalformedRunlist
            | NtfsError::UnsupportedCompressedData
            | NtfsError::UnsupportedNamedDataStream
            | NtfsError::InvalidDirectoryIndex
            | NtfsError::InvalidClusterNumber { .. }
            | NtfsError::InvalidUtf16
            | NtfsError::OutOfBoundsRead
            | NtfsError::ReadError(_) => AppError::ExtractionFailed(message),
        }
    }
}

/// Returns [`AppError::FilesystemNotRecognized`] when `candidate` is
/// [`FilesystemProbeCandidate::Unknown`], otherwise passes it through.
pub fn require_recognized_filesystem(
    candidate: FilesystemProbeCandidate,
) -> Result<FilesystemProbeCandidate, AppError> {
    match candidate {
        FilesystemProbeCandidate::Unknown => Err(AppError::FilesystemNotRecognized),
        recognized => Ok(recognized),
    }
}
