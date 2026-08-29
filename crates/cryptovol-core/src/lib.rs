//! Core domain abstractions shared by `cryptovol` crates.
//!
//! This crate contains read-only block reader traits, file-backed block access,
//! common errors, and generic encrypted-volume backend contracts. It does not
//! perform command-line parsing, password prompting, or format-specific crypto.

mod block_reader;
mod error;
mod volume;

pub use block_reader::{BlockReader, FileBlockReader};
pub use error::CryptovolError;
pub use volume::CryptoVolumeBackend;

/// Fixed chunk size used when streaming file data from an encrypted container
/// to a destination writer. Bounds peak RAM to roughly this size plus parser
/// metadata, regardless of source file or container size.
pub const EXTRACTION_CHUNK_SIZE: usize = 256 * 1024;

/// Returns the public product name used in user-facing output.
pub fn product_name() -> &'static str {
    "Crypto Volume Viewer"
}
