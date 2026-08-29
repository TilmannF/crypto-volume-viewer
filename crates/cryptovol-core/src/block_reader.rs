use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::CryptovolError;

/// Read-only random-access byte reader for logical block devices or files.
pub trait BlockReader {
    /// Returns the total readable length in bytes.
    fn len(&self) -> u64;

    /// Reads exactly `buf.len()` bytes starting at `offset`.
    ///
    /// # Errors
    ///
    /// Returns [`CryptovolError`] when the requested range is outside the
    /// reader length or the underlying read operation fails.
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<(), CryptovolError>;

    /// Returns `true` when the reader has no readable bytes.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// File-backed [`BlockReader`] implementation.
#[derive(Debug)]
pub struct FileBlockReader {
    file: Mutex<File>,
    len: u64,
}

impl FileBlockReader {
    /// Opens `path` read-only and records its current length.
    ///
    /// # Errors
    ///
    /// Returns [`CryptovolError::FileOpen`] if the file cannot be opened, or
    /// [`CryptovolError::Metadata`] if its metadata cannot be read.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, CryptovolError> {
        let path = path.as_ref();
        let owned_path = PathBuf::from(path);
        let file = File::open(path).map_err(|source| CryptovolError::FileOpen {
            path: owned_path.clone(),
            source,
        })?;
        let len = file
            .metadata()
            .map_err(|source| CryptovolError::Metadata {
                path: owned_path,
                source,
            })?
            .len();

        Ok(Self {
            file: Mutex::new(file),
            len,
        })
    }

    fn validate_range(&self, offset: u64, length: usize) -> Result<(), CryptovolError> {
        let length_u64 = u64::try_from(length).map_err(|_| CryptovolError::OutOfBounds {
            offset,
            length,
            file_len: self.len,
        })?;
        let end = offset
            .checked_add(length_u64)
            .ok_or(CryptovolError::OutOfBounds {
                offset,
                length,
                file_len: self.len,
            })?;

        if end > self.len {
            return Err(CryptovolError::OutOfBounds {
                offset,
                length,
                file_len: self.len,
            });
        }

        Ok(())
    }
}

impl<T: BlockReader + ?Sized> BlockReader for &T {
    fn len(&self) -> u64 {
        (**self).len()
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<(), CryptovolError> {
        (**self).read_at(offset, buf)
    }
}

impl BlockReader for FileBlockReader {
    fn len(&self) -> u64 {
        self.len
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<(), CryptovolError> {
        self.validate_range(offset, buf.len())?;

        if buf.is_empty() {
            return Ok(());
        }

        let mut file = self.file.lock().map_err(|_| CryptovolError::Read {
            source: std::io::Error::other("file reader lock poisoned"),
        })?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|source| CryptovolError::Read { source })?;
        file.read_exact(buf)
            .map_err(|source| CryptovolError::Read { source })
    }
}
