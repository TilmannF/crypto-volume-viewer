use crate::{BlockReader, CryptovolError};

/// Common interface for encrypted container format backends.
pub trait CryptoVolumeBackend {
    /// Concrete inspection result returned by this backend.
    type Inspection;

    /// Stable backend name used in diagnostics and output.
    fn name(&self) -> &'static str;

    /// Inspects format-specific metadata without requiring secrets.
    ///
    /// # Errors
    ///
    /// Returns [`CryptovolError`] when the underlying reader fails while
    /// collecting inspection data.
    fn inspect(&self, reader: &dyn BlockReader) -> Result<Self::Inspection, CryptovolError>;
}
