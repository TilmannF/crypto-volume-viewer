use std::path::PathBuf;

/// Error type shared by core block-reader operations.
#[derive(Debug, thiserror::Error)]
pub enum CryptovolError {
    /// A file could not be opened for read-only access.
    #[error("failed to open file {path}: {source}")]
    FileOpen {
        /// Path that failed to open.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// File metadata could not be read.
    #[error("failed to read metadata for {path}: {source}")]
    Metadata {
        /// Path whose metadata failed.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// A read operation failed.
    #[error("failed to read from file: {source}")]
    Read {
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// The requested read range is outside the reader length.
    #[error("read range offset {offset} length {length} is outside file length {file_len}")]
    OutOfBounds {
        /// Requested starting offset.
        offset: u64,
        /// Requested byte count.
        length: usize,
        /// Total readable length.
        file_len: u64,
    },
}
