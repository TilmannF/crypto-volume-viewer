//! Volume opening and the owned `VolumeSession` API.

use crate::error::{require_recognized_filesystem, AppError};
use crate::extract::{extract_via_writer, open_streaming_writer};
use crate::model::{FileEntry, FilesystemKind};
use crate::{CancellationToken, ExtractOptions, ExtractSummary, ProgressEvent};
use cryptovol_core::{BlockReader, FileBlockReader};
use cryptovol_fs_exfat::ExfatFileSystem;
use cryptovol_fs_fat::FatFileSystem;
use cryptovol_fs_ntfs::NtfsFileSystem;
use cryptovol_tcvc::{
    open_with_options, probe_filesystem, FilesystemProbeCandidate, HeaderCandidateRole, PimState,
    TcvcKdf, TcvcMatchedOpenedVolume, TcvcOpenOptions,
};
use secrecy::{ExposeSecret, SecretString};
use std::path::{Path, PathBuf};

/// Request to open a TC/VC volume.
///
/// The password is consumed by [`open_volume`] and is never stored on the
/// returned [`VolumeSession`].
pub struct OpenVolumeRequest {
    /// Path to the container file.
    pub container_path: PathBuf,
    /// Volume password.
    pub password: SecretString,
    /// Personal Iterations Multiplier; `None` uses the VeraCrypt default.
    pub pim: Option<u32>,
    /// KDF/hash hint; `None` autoprobes all supported KDFs.
    pub kdf_hint: Option<TcvcKdf>,
}

/// An opened TC/VC volume.
///
/// Owns the container's [`FileBlockReader`] and the matched, opened TC/VC
/// volume state. The password used to open it is never retained; operations
/// that need a decrypted data reader construct one locally from the owned
/// reader and opened-volume state rather than storing it on the session.
pub struct VolumeSession {
    container_path: PathBuf,
    reader: FileBlockReader,
    opened: TcvcMatchedOpenedVolume,
}

impl std::fmt::Debug for VolumeSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VolumeSession")
            .field("container_path", &self.container_path)
            .finish_non_exhaustive()
    }
}

/// Opens a TC/VC volume with the given password, PIM, and KDF hint.
///
/// # Errors
///
/// Returns [`AppError`] if the container file cannot be opened, the
/// password is incorrect, or the volume's profile is unsupported.
pub fn open_volume(request: OpenVolumeRequest) -> Result<VolumeSession, AppError> {
    let reader = FileBlockReader::open(&request.container_path)?;

    // `TcvcOpenOptions` zeroizes its own `password` field on drop, so no
    // manual cleanup is needed once `options` goes out of scope below.
    let options = TcvcOpenOptions {
        password: request.password.expose_secret().as_bytes().to_vec(),
        pim: request.pim,
        kdf_hint: request.kdf_hint,
    };

    let opened = open_with_options(&reader, &options)?;

    Ok(VolumeSession {
        container_path: request.container_path,
        reader,
        opened,
    })
}

/// Safe, non-secret metadata about an opened TC/VC volume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolumeInfo {
    /// Path to the container file.
    pub container_path: PathBuf,
    /// Container file size in bytes.
    pub container_size_bytes: u64,
    /// Crypto backend name (currently always `"tcvc"`).
    pub backend: &'static str,
    /// Cipher in use (currently always `"AES-XTS"`, the only supported cipher).
    pub cipher: &'static str,
    /// KDF/hash that matched when the volume was opened.
    pub kdf: TcvcKdf,
    /// Whether the default or a custom PIM was used to open the volume.
    pub pim: PimState,
    /// Which header candidate (primary or backup) matched.
    pub header_role: HeaderCandidateRole,
    /// Detected filesystem kind, or [`FilesystemKind::Unknown`] if the
    /// decrypted data area could not be probed.
    pub filesystem: FilesystemKind,
    /// Whether the volume is exposed read-only.
    pub read_only: bool,
    /// Byte offset of the decrypted data region within the container, when
    /// known. `None` is reserved for future backends that may not expose
    /// this concept; TC/VC volumes always report `Some`.
    pub decrypted_data_offset: Option<u64>,
    /// Length in bytes of the decrypted data region, when known. `None` is
    /// reserved for future backends that may not expose this concept; TC/VC
    /// volumes always report `Some`.
    pub decrypted_data_len: Option<u64>,
}

impl VolumeSession {
    /// Returns safe, non-secret metadata about this opened volume.
    #[must_use]
    pub fn volume_info(&self) -> VolumeInfo {
        let metadata = self.opened.metadata();
        let matched = self.opened.matched_profile();

        let filesystem = self
            .opened
            .data_reader(&self.reader)
            .ok()
            .and_then(|data_reader| probe_filesystem(&data_reader).ok())
            .map(FilesystemKind::from)
            .unwrap_or(FilesystemKind::Unknown);

        VolumeInfo {
            container_path: self.container_path.clone(),
            container_size_bytes: self.reader.len(),
            backend: metadata.backend,
            cipher: "AES-XTS",
            kdf: matched.kdf,
            pim: matched.pim,
            header_role: matched.header_role,
            filesystem,
            read_only: metadata.read_only,
            decrypted_data_offset: Some(metadata.data_offset),
            decrypted_data_len: Some(metadata.data_length),
        }
    }

    /// Lists the entries of the directory at `path`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::FilesystemNotRecognized`] if the volume's
    /// filesystem could not be identified, [`AppError::PathNotFound`] if
    /// `path` does not exist, or another [`AppError`] variant for other
    /// filesystem-specific failures.
    pub fn list_dir(&self, path: &str) -> Result<Vec<FileEntry>, AppError> {
        let data_reader = self.opened.data_reader(&self.reader)?;
        let candidate = require_recognized_filesystem(probe_filesystem(&data_reader)?)?;
        let filesystem = FilesystemKind::from(candidate);

        match candidate {
            FilesystemProbeCandidate::FatLike => {
                let fs = FatFileSystem::open(&data_reader)?;
                let entries = fs.list_dir(path)?;
                Ok(entries
                    .into_iter()
                    .map(|entry| FileEntry::from((entry, filesystem)))
                    .collect())
            }
            FilesystemProbeCandidate::ExFat => {
                let fs = ExfatFileSystem::open(&data_reader)?;
                let entries = fs.list_dir(path)?;
                Ok(entries
                    .into_iter()
                    .map(|entry| FileEntry::from((entry, filesystem)))
                    .collect())
            }
            FilesystemProbeCandidate::Ntfs => {
                let fs = NtfsFileSystem::open(&data_reader)?;
                let entries = fs.list_dir(path)?;
                Ok(entries
                    .into_iter()
                    .map(|entry| FileEntry::from((entry, filesystem)))
                    .collect())
            }
            FilesystemProbeCandidate::Unknown => {
                unreachable!("require_recognized_filesystem rejects Unknown")
            }
        }
    }

    /// Returns metadata for a single file or directory at `path`.
    ///
    /// exFAT and NTFS use their native stat operation. FAT has none, so
    /// this falls back to listing the parent directory and matching the
    /// requested entry by name.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::PathNotFound`] if `path` does not exist, or
    /// another [`AppError`] variant for other filesystem-specific failures.
    pub fn stat(&self, path: &str) -> Result<FileEntry, AppError> {
        let data_reader = self.opened.data_reader(&self.reader)?;
        let candidate = require_recognized_filesystem(probe_filesystem(&data_reader)?)?;
        let filesystem = FilesystemKind::from(candidate);

        match candidate {
            FilesystemProbeCandidate::FatLike => self.stat_via_parent_listing(path),
            FilesystemProbeCandidate::ExFat => {
                let fs = ExfatFileSystem::open(&data_reader)?;
                let entry = fs.stat(path)?;
                Ok(FileEntry {
                    path: Some(path.to_string()),
                    ..FileEntry::from((entry, filesystem))
                })
            }
            FilesystemProbeCandidate::Ntfs => {
                let fs = NtfsFileSystem::open(&data_reader)?;
                let entry = fs.stat(path)?;
                Ok(FileEntry {
                    path: Some(path.to_string()),
                    ..FileEntry::from((entry, filesystem))
                })
            }
            FilesystemProbeCandidate::Unknown => {
                unreachable!("require_recognized_filesystem rejects Unknown")
            }
        }
    }

    /// Extracts the file at `source_path` to `destination_path` on the host
    /// filesystem, reporting [`ProgressEvent`]s and honoring
    /// `options.cancellation_token`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::PathNotFound`] if `source_path` does not exist,
    /// [`AppError::DirectoryExtractionUnsupported`] if it names a directory,
    /// [`AppError::Cancelled`] if cancelled before or during the copy, or
    /// another [`AppError`] variant for other filesystem or destination
    /// write failures.
    pub fn extract_file(
        &self,
        source_path: &str,
        destination_path: impl AsRef<Path>,
        options: ExtractOptions,
        mut progress: impl FnMut(ProgressEvent),
    ) -> Result<ExtractSummary, AppError> {
        let destination_path = destination_path.as_ref();
        let cancellation_token = options.cancellation_token.as_ref();

        if cancellation_token.is_some_and(CancellationToken::is_cancelled) {
            return Err(AppError::Cancelled);
        }

        let entry = self.stat(source_path)?;
        if entry.is_dir {
            return Err(AppError::DirectoryExtractionUnsupported(
                source_path.to_string(),
            ));
        }
        let total_bytes = Some(entry.size);

        let writer = open_streaming_writer(destination_path, options.overwrite, options.parents)?;

        let data_reader = self.opened.data_reader(&self.reader)?;
        let candidate = require_recognized_filesystem(probe_filesystem(&data_reader)?)?;

        match candidate {
            FilesystemProbeCandidate::FatLike => {
                let fs = FatFileSystem::open(&data_reader)?;
                extract_via_writer(
                    writer,
                    source_path,
                    destination_path,
                    total_bytes,
                    cancellation_token,
                    &mut progress,
                    |w| fs.read_file_to_writer(source_path, w),
                )
            }
            FilesystemProbeCandidate::ExFat => {
                let fs = ExfatFileSystem::open(&data_reader)?;
                extract_via_writer(
                    writer,
                    source_path,
                    destination_path,
                    total_bytes,
                    cancellation_token,
                    &mut progress,
                    |w| fs.read_file_to_writer(source_path, w),
                )
            }
            FilesystemProbeCandidate::Ntfs => {
                let fs = NtfsFileSystem::open(&data_reader)?;
                extract_via_writer(
                    writer,
                    source_path,
                    destination_path,
                    total_bytes,
                    cancellation_token,
                    &mut progress,
                    |w| fs.read_file_to_writer(source_path, w),
                )
            }
            FilesystemProbeCandidate::Unknown => {
                unreachable!("require_recognized_filesystem rejects Unknown")
            }
        }
    }

    /// FAT has no native stat: list the parent directory and match `path`'s
    /// final component by name.
    fn stat_via_parent_listing(&self, path: &str) -> Result<FileEntry, AppError> {
        let (parent, name) = split_parent_and_name(path);
        let entries = self.list_dir(parent)?;
        entries
            .into_iter()
            .find(|entry| entry.name == name)
            .map(|entry| FileEntry {
                path: Some(path.to_string()),
                ..entry
            })
            .ok_or_else(|| AppError::PathNotFound(path.to_string()))
    }
}

/// Splits an absolute container path into its parent directory and final
/// component name. A path with no `/` is treated as a root-level name.
fn split_parent_and_name(path: &str) -> (&str, &str) {
    match path.rsplit_once('/') {
        Some((parent, name)) => (parent, name),
        None => ("", path),
    }
}
