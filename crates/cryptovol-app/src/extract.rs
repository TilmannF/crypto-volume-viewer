//! Streaming extraction destination writer and the progress/cancellation-aware
//! copy core used by the session module.

use crate::cancellation::CancellationToken;
use crate::error::AppError;
use cryptovol_core::EXTRACTION_CHUNK_SIZE;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

/// Errors returned while writing an extracted file to the host filesystem.
#[derive(Debug)]
pub enum WriteError {
    /// Destination already exists and overwrite was not requested.
    DestinationExists(PathBuf),
    /// Destination path resolves to a directory.
    DestinationIsDirectory(PathBuf),
    /// Destination path resolves to a symbolic link.
    DestinationIsSymlink(PathBuf),
    /// Parent directory is missing and parent creation was not requested.
    ParentDirectoryMissing(PathBuf),
    /// Parent directory creation failed.
    CreateParent {
        /// Parent directory path.
        path: PathBuf,
        /// Underlying I/O error.
        source: std::io::Error,
    },
    /// Temporary file could not be created in the destination directory.
    TempFileCreate {
        /// Directory where temp file creation was attempted.
        path: PathBuf,
        /// Underlying I/O error.
        source: std::io::Error,
    },
    /// Atomic rename of temp file to destination failed.
    AtomicPersist {
        /// Destination path.
        path: PathBuf,
        /// Underlying persist error (includes reclaimed temp file).
        source: tempfile::PersistError,
    },
}

impl PartialEq for WriteError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::DestinationExists(a), Self::DestinationExists(b))
            | (Self::DestinationIsDirectory(a), Self::DestinationIsDirectory(b))
            | (Self::DestinationIsSymlink(a), Self::DestinationIsSymlink(b))
            | (Self::ParentDirectoryMissing(a), Self::ParentDirectoryMissing(b)) => a == b,
            (
                Self::CreateParent {
                    path: a,
                    source: source_a,
                },
                Self::CreateParent {
                    path: b,
                    source: source_b,
                },
            ) => a == b && source_a.kind() == source_b.kind(),
            (
                Self::TempFileCreate {
                    path: a,
                    source: source_a,
                },
                Self::TempFileCreate {
                    path: b,
                    source: source_b,
                },
            ) => a == b && source_a.kind() == source_b.kind(),
            (
                Self::AtomicPersist {
                    path: a,
                    source: source_a,
                },
                Self::AtomicPersist {
                    path: b,
                    source: source_b,
                },
            ) => a == b && source_a.error.kind() == source_b.error.kind(),
            _ => false,
        }
    }
}

impl Eq for WriteError {}

impl std::fmt::Display for WriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WriteError::DestinationExists(p) => {
                write!(f, "destination already exists: {}", p.display())
            }
            WriteError::DestinationIsDirectory(p) => {
                write!(f, "destination is a directory: {}", p.display())
            }
            WriteError::DestinationIsSymlink(p) => {
                write!(f, "destination is a symbolic link: {}", p.display())
            }
            WriteError::ParentDirectoryMissing(p) => {
                write!(f, "parent directory does not exist: {}", p.display())
            }
            WriteError::CreateParent { path, source } => {
                write!(
                    f,
                    "could not create parent directory {}: {source}",
                    path.display()
                )
            }
            WriteError::TempFileCreate { path, source } => {
                write!(
                    f,
                    "could not create temp file in {}: {source}",
                    path.display()
                )
            }
            WriteError::AtomicPersist { path, source } => {
                write!(
                    f,
                    "could not persist to destination {}: {}",
                    path.display(),
                    source.error
                )
            }
        }
    }
}

impl std::error::Error for WriteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::CreateParent { source, .. } | Self::TempFileCreate { source, .. } => Some(source),
            Self::AtomicPersist { source, .. } => Some(&source.error),
            Self::DestinationExists(_)
            | Self::DestinationIsDirectory(_)
            | Self::DestinationIsSymlink(_)
            | Self::ParentDirectoryMissing(_) => None,
        }
    }
}

impl From<WriteError> for AppError {
    fn from(err: WriteError) -> Self {
        match err {
            WriteError::DestinationExists(path) => {
                AppError::InvalidInput(format!("destination already exists: {}", path.display()))
            }
            WriteError::DestinationIsDirectory(path) => {
                AppError::InvalidInput(format!("destination is a directory: {}", path.display()))
            }
            WriteError::DestinationIsSymlink(path) => AppError::InvalidInput(format!(
                "destination is a symbolic link: {}",
                path.display()
            )),
            WriteError::ParentDirectoryMissing(path) => AppError::InvalidInput(format!(
                "parent directory does not exist: {}",
                path.display()
            )),
            WriteError::CreateParent { .. }
            | WriteError::TempFileCreate { .. }
            | WriteError::AtomicPersist { .. } => AppError::ExtractionFailed(err.to_string()),
        }
    }
}

/// A streaming destination writer backed by a temporary file in the same
/// directory as the destination.
///
/// Data is written to the temp file via [`std::io::Write`]; call
/// [`StreamingWriter::finish`] to atomically rename it to the final path.
/// Dropping without calling `finish` discards the temp file automatically —
/// the destination path is never touched.
#[derive(Debug)]
pub struct StreamingWriter {
    tmp: NamedTempFile,
    dst: PathBuf,
}

/// Opens a streaming destination writer, enforcing overwrite and parent policy.
///
/// The temp file is created in the same directory as `dst` so that
/// [`StreamingWriter::finish`] can use an atomic rename on the same filesystem.
///
/// # Errors
///
/// Returns [`WriteError`] when the destination or parent policy is violated,
/// a symlink/directory stands at the destination, or temp file creation fails.
pub fn open_streaming_writer(
    dst: &Path,
    overwrite: bool,
    parents: bool,
) -> Result<StreamingWriter, WriteError> {
    let parent = dst.parent().unwrap_or(Path::new(""));
    let parent_exists = parent.as_os_str().is_empty() || parent.exists();

    if !parent_exists {
        if parents {
            std::fs::create_dir_all(parent).map_err(|source| WriteError::CreateParent {
                path: parent.to_path_buf(),
                source,
            })?;
        } else {
            return Err(WriteError::ParentDirectoryMissing(parent.to_path_buf()));
        }
    }

    if let Ok(metadata) = std::fs::symlink_metadata(dst) {
        let file_type = metadata.file_type();
        if file_type.is_symlink() {
            return Err(WriteError::DestinationIsSymlink(dst.to_path_buf()));
        }
        if file_type.is_dir() {
            return Err(WriteError::DestinationIsDirectory(dst.to_path_buf()));
        }
        if !overwrite {
            return Err(WriteError::DestinationExists(dst.to_path_buf()));
        }
    }

    let tmp_dir = if parent.as_os_str().is_empty() {
        Path::new(".")
    } else {
        parent
    };
    let tmp = NamedTempFile::new_in(tmp_dir).map_err(|source| WriteError::TempFileCreate {
        path: tmp_dir.to_path_buf(),
        source,
    })?;

    Ok(StreamingWriter {
        tmp,
        dst: dst.to_path_buf(),
    })
}

impl StreamingWriter {
    /// Atomically persists the written bytes to the destination path.
    ///
    /// On success the destination file contains exactly what was written.
    /// On failure the temp file is dropped (auto-cleaned); the destination is
    /// either absent or retains its previous content.
    ///
    /// # Errors
    ///
    /// Returns [`WriteError::AtomicPersist`] if the rename fails.
    pub fn finish(self) -> Result<(), WriteError> {
        self.tmp
            .persist(&self.dst)
            .map(|_: File| ())
            .map_err(|source| WriteError::AtomicPersist {
                path: self.dst.clone(),
                source,
            })
    }
}

impl Write for StreamingWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.tmp.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.tmp.flush()
    }
}

/// Progress reported while streaming a file out of an opened volume.
///
/// Progress is based on bytes written to the destination, not bytes read
/// from the (decrypted) source. Events never carry decrypted file content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgressEvent {
    /// Extraction of `source_path` to `destination_path` has begun.
    Started {
        /// Container path of the file being extracted.
        source_path: String,
        /// Host destination path.
        destination_path: PathBuf,
        /// Total bytes to write, when known in advance.
        total_bytes: Option<u64>,
    },
    /// At least one chunk has been written to the destination.
    Advanced {
        /// Cumulative bytes written so far.
        bytes_written: u64,
        /// Total bytes to write, when known in advance.
        total_bytes: Option<u64>,
    },
    /// Extraction finished and the destination was fully persisted.
    Finished {
        /// Total bytes written to the destination.
        bytes_written: u64,
    },
}

/// Outcome of a successful extraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtractSummary {
    /// Total bytes written to the destination.
    pub bytes_written: u64,
}

/// Destination and cancellation policy for [`crate::session::VolumeSession::extract_file`].
#[derive(Debug, Clone, Default)]
pub struct ExtractOptions {
    /// Overwrite the destination file if it already exists.
    pub overwrite: bool,
    /// Create missing parent directories of the destination.
    pub parents: bool,
    /// Optional token allowing the caller to cancel extraction in progress.
    pub cancellation_token: Option<CancellationToken>,
}

/// Copies `reader` into `writer` in [`EXTRACTION_CHUNK_SIZE`] chunks,
/// reporting progress and honoring cancellation.
///
/// Takes ownership of `writer` so that cancellation or an I/O error simply
/// drops it (discarding the temp file via [`StreamingWriter`]'s drop
/// behavior) instead of persisting a partial destination. `writer.finish()`
/// is only called after the full source has been copied successfully.
///
/// # Errors
///
/// Returns [`AppError::Cancelled`] if `cancellation_token` is cancelled
/// before the first chunk or between chunks. Returns [`AppError::Io`] for
/// source read failures, and [`AppError::ExtractionFailed`] for destination
/// write or persist failures.
pub fn copy_with_progress_and_cancellation<R: Read>(
    mut reader: R,
    writer: StreamingWriter,
    source_path: &str,
    destination_path: &Path,
    total_bytes: Option<u64>,
    cancellation_token: Option<&CancellationToken>,
    mut progress: impl FnMut(ProgressEvent),
) -> Result<ExtractSummary, AppError> {
    let mut writer = writer;

    if cancellation_token.is_some_and(CancellationToken::is_cancelled) {
        return Err(AppError::Cancelled);
    }

    progress(ProgressEvent::Started {
        source_path: source_path.to_string(),
        destination_path: destination_path.to_path_buf(),
        total_bytes,
    });

    let mut buf = vec![0u8; EXTRACTION_CHUNK_SIZE];
    let mut bytes_written: u64 = 0;

    loop {
        if cancellation_token.is_some_and(CancellationToken::is_cancelled) {
            return Err(AppError::Cancelled);
        }

        let n = reader.read(&mut buf).map_err(AppError::Io)?;
        if n == 0 {
            break;
        }

        writer
            .write_all(&buf[..n])
            .map_err(|source| AppError::ExtractionFailed(source.to_string()))?;
        bytes_written += n as u64;

        progress(ProgressEvent::Advanced {
            bytes_written,
            total_bytes,
        });
    }

    writer.finish()?;
    progress(ProgressEvent::Finished { bytes_written });

    Ok(ExtractSummary { bytes_written })
}

/// Upper bound on bytes written per [`ProgressWriter::write`] call.
///
/// Filesystem readers may hand a whole file to a single `write_all` call
/// when it fits within their own internal read buffer (bounded by
/// [`EXTRACTION_CHUNK_SIZE`]). Re-chunking at this smaller granularity keeps
/// progress reporting and cancellation checks responsive even for files
/// smaller than a filesystem's own read chunk.
const PROGRESS_WRITE_CHUNK_SIZE: usize = 32 * 1024;

/// Wraps a [`StreamingWriter`] so a filesystem's own `read_file_to_writer`
/// (which writes directly rather than exposing a [`Read`] source) still
/// reports [`ProgressEvent`]s and honors cancellation on every write.
pub(crate) struct ProgressWriter<'a> {
    inner: StreamingWriter,
    bytes_written: u64,
    total_bytes: Option<u64>,
    cancellation_token: Option<&'a CancellationToken>,
    progress: &'a mut dyn FnMut(ProgressEvent),
    cancelled: bool,
}

impl Write for ProgressWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self
            .cancellation_token
            .is_some_and(CancellationToken::is_cancelled)
        {
            self.cancelled = true;
            return Err(std::io::Error::other("extraction cancelled"));
        }

        let chunk = buf.len().min(PROGRESS_WRITE_CHUNK_SIZE);
        let n = self.inner.write(&buf[..chunk])?;
        self.bytes_written += n as u64;
        (self.progress)(ProgressEvent::Advanced {
            bytes_written: self.bytes_written,
            total_bytes: self.total_bytes,
        });
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// Drives a filesystem's `read_file_to_writer` through a cancellation- and
/// progress-aware [`StreamingWriter`], reporting the same
/// Started/Advanced/Finished sequence as [`copy_with_progress_and_cancellation`].
///
/// # Errors
///
/// Returns [`AppError::Cancelled`] if `cancellation_token` is cancelled
/// before writing starts or between chunks. Returns `AppError::from(err)`
/// for any other error `read_into` produces, and propagates
/// [`StreamingWriter::finish`] failures as [`AppError::ExtractionFailed`].
pub(crate) fn extract_via_writer<E>(
    writer: StreamingWriter,
    source_path: &str,
    destination_path: &Path,
    total_bytes: Option<u64>,
    cancellation_token: Option<&CancellationToken>,
    mut progress: impl FnMut(ProgressEvent),
    read_into: impl FnOnce(&mut ProgressWriter<'_>) -> Result<u64, E>,
) -> Result<ExtractSummary, AppError>
where
    AppError: From<E>,
{
    if cancellation_token.is_some_and(CancellationToken::is_cancelled) {
        return Err(AppError::Cancelled);
    }

    progress(ProgressEvent::Started {
        source_path: source_path.to_string(),
        destination_path: destination_path.to_path_buf(),
        total_bytes,
    });

    let mut tracked = ProgressWriter {
        inner: writer,
        bytes_written: 0,
        total_bytes,
        cancellation_token,
        progress: &mut progress,
        cancelled: false,
    };

    let result = read_into(&mut tracked);
    let bytes_written = tracked.bytes_written;
    let was_cancelled = tracked.cancelled;
    let inner = tracked.inner;

    match result {
        Ok(_) => {
            inner.finish()?;
            progress(ProgressEvent::Finished { bytes_written });
            Ok(ExtractSummary { bytes_written })
        }
        Err(_) if was_cancelled => Err(AppError::Cancelled),
        Err(err) => Err(AppError::from(err)),
    }
}
