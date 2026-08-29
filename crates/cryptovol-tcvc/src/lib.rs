//! TC/VC-compatible encrypted container support for the narrow MVP profile.
//!
//! The current implementation supports only file-hosted normal containers that
//! use AES-XTS, SHA-512 header KDF parameters, 512-byte sectors, and no hidden
//! volume data. It exposes header-candidate inspection, password-based opening,
//! decrypted read-only data access, and conservative first-sector filesystem
//! probing.

#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::panic,
        reason = "unit tests use direct synthetic fixture assertions"
    )
)]

use aes::cipher::KeyInit;
use aes::Aes256;
use blake2::Blake2s256;
use cryptovol_core::{BlockReader, CryptoVolumeBackend, CryptovolError};
// Blake2s uses Lazy buffering, not EagerHash, so it can't use Hmac or pbkdf2_hmac.
// Use the lower-level pbkdf2 function with SimpleHmac<Blake2s256> instead.
use pbkdf2::{hmac::SimpleHmac, pbkdf2 as pbkdf2_raw, pbkdf2_hmac};
use sha2::{Sha256, Sha512};
use std::fmt;
use streebog::Streebog512;
use whirlpool::Whirlpool;
use xts_mode::{get_tweak_default, Xts128};
use zeroize::Zeroize;

/// Stable backend name for TrueCrypt/VeraCrypt-compatible containers.
pub const BACKEND_NAME: &str = "tcvc";
/// Current narrow supported profile identifier.
pub const SUPPORTED_PROFILE: &str = "tcvc-aes-sha512-basic";
/// Size in bytes of each TC/VC header candidate.
pub const TCVC_HEADER_CANDIDATE_LEN: u64 = 512;
const TCVC_HEADER_CANDIDATE_LEN_USIZE: usize = 512;
const HEADER_SALT_LEN: usize = 64;
const ENCRYPTED_HEADER_OFFSET: usize = 64;
const HEADER_KEY_LEN: usize = 64;
const DATA_KEY_LEN: usize = 64;
const AES_KEY_LEN: usize = 32;
const VERA_MAGIC_OFFSET: usize = 64;
const VERA_MAGIC: &[u8; 4] = b"VERA";
const HEADER_KEY_AREA_CRC_OFFSET: usize = 72;
const HIDDEN_VOLUME_SIZE_OFFSET: usize = 92;
const VOLUME_SIZE_OFFSET: usize = 100;
const DATA_OFFSET_OFFSET: usize = 108;
const DATA_LENGTH_OFFSET: usize = 116;
const HEADER_FLAGS_OFFSET: usize = 124;
const SECTOR_SIZE_OFFSET: usize = 128;
const HEADER_CRC_OFFSET: usize = 252;
const KEY_AREA_OFFSET: usize = 256;
const SUPPORTED_SECTOR_SIZE: u32 = 512;
const SUPPORTED_SECTOR_SIZE_USIZE: usize = 512;
const VERA_PIM0_SHA512_ITERATIONS: u32 = 500_000;
const UNSUPPORTED_SYSTEM_OR_IN_PLACE_FLAGS: u32 = 0x0000_0001;

/// Available PBKDF2-HMAC KDF/hash profiles for TC/VC header key derivation.
///
/// The iteration count formula for non-system/file containers is identical
/// across all profiles: `PIM=0` → 500,000 iterations; `PIM>0` → `15000 + (PIM * 1000)`.
/// Source: <https://veracrypt.io/en/Personal%20Iterations%20Multiplier%20(PIM).html>
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TcvcKdf {
    /// SHA-512 PBKDF2-HMAC header KDF.
    Sha512,
    /// SHA-256 PBKDF2-HMAC header KDF.
    Sha256,
    /// Whirlpool PBKDF2-HMAC header KDF.
    Whirlpool,
    /// BLAKE2s-256 PBKDF2-HMAC header KDF.
    Blake2s256,
    /// Streebog-512 (GOST R 34.11-2012) PBKDF2-HMAC header KDF.
    Streebog,
}

impl TcvcKdf {
    /// Human-readable display name for CLI output and documentation.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Sha512 => "SHA-512",
            Self::Sha256 => "SHA-256",
            Self::Whirlpool => "Whirlpool",
            Self::Blake2s256 => "BLAKE2s-256",
            Self::Streebog => "Streebog",
        }
    }
}

/// Computes the PBKDF2 iteration count for the given KDF and optional PIM value.
///
/// Formula (confirmed from official VeraCrypt documentation):
/// - `pim=None` or `pim=Some(0)`: 500,000 (VeraCrypt default for non-system file containers)
/// - `pim=Some(n)` where n>0: `15000 + (n * 1000)` (uniform across all PBKDF2-HMAC hash algorithms)
///
/// # Errors
///
/// Returns [`TcvcOpenError::InvalidPim`] if the PIM value overflows the iteration count.
pub fn compute_iterations(kdf: TcvcKdf, pim: Option<u32>) -> Result<u32, TcvcOpenError> {
    let _ = kdf; // formula is uniform across all PBKDF2-HMAC algorithms for non-system containers
    match pim {
        None | Some(0) => Ok(VERA_PIM0_SHA512_ITERATIONS),
        Some(n) => {
            let product = n.checked_mul(1_000).ok_or(TcvcOpenError::InvalidPim {
                reason: "PIM * 1000 overflows u32",
            })?;
            15_000u32
                .checked_add(product)
                .ok_or(TcvcOpenError::InvalidPim {
                    reason: "15000 + (PIM * 1000) overflows u32",
                })
        }
    }
}

/// Options for opening a TC/VC volume with multi-KDF autoprobing and custom PIM support.
pub struct TcvcOpenOptions {
    /// Raw password bytes; zeroized on drop.
    pub password: Vec<u8>,
    /// Personal Iterations Multiplier. `None` or `Some(0)` means VeraCrypt default (500,000 iterations).
    pub pim: Option<u32>,
    /// Optional KDF hint; `None` means autoprobe all supported KDFs in deterministic order.
    pub kdf_hint: Option<TcvcKdf>,
}

impl fmt::Debug for TcvcOpenOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TcvcOpenOptions")
            .field("password", &"[redacted]")
            .field("pim", &self.pim)
            .field("kdf_hint", &self.kdf_hint)
            .finish()
    }
}

impl Drop for TcvcOpenOptions {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}

/// Records whether a container was opened with the VeraCrypt default PIM or a custom value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PimState {
    /// Default PIM (500,000 iterations for non-system file-hosted containers).
    Default,
    /// Explicit custom PIM value supplied by the caller.
    Custom(u32),
}

impl PimState {
    /// Constructs a `PimState` from an optional PIM input.
    pub fn from_option(pim: Option<u32>) -> Self {
        match pim {
            None | Some(0) => Self::Default,
            Some(n) => Self::Custom(n),
        }
    }

    /// Returns `true` if this represents the VeraCrypt default PIM.
    pub fn is_default(self) -> bool {
        matches!(self, Self::Default)
    }

    /// Returns the explicit PIM value, or `None` for the default.
    pub fn value(self) -> Option<u32> {
        match self {
            Self::Custom(n) => Some(n),
            Self::Default => None,
        }
    }

    /// Returns a human-readable string for CLI output: `"default"` or the numeric value.
    pub fn display(self) -> String {
        match self {
            Self::Default => "default".to_owned(),
            Self::Custom(n) => n.to_string(),
        }
    }
}

/// Backend handle for TC/VC-compatible container inspection.
#[derive(Debug, Default, Clone, Copy)]
pub struct TcvcBackend;

impl TcvcBackend {
    /// Creates a TC/VC backend handle.
    pub fn new() -> Self {
        Self
    }
}

/// Result of inspecting TC/VC header candidate locations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TcvcInspection {
    /// The file is too small to contain a single header candidate.
    TooSmall {
        /// Actual file size in bytes.
        file_size: u64,
        /// Minimum file size required for one candidate header.
        required_minimum: u64,
    },
    /// Primary and backup candidate locations were identified.
    Candidates {
        /// Primary header candidate at the start of the container.
        primary: HeaderCandidate,
        /// Backup header candidate state.
        backup: HeaderCandidateState,
    },
}

/// Metadata about a readable header candidate location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderCandidate {
    /// Candidate role.
    pub role: HeaderCandidateRole,
    /// Candidate byte offset in the container.
    pub offset: u64,
    /// Candidate length in bytes.
    pub length: u64,
    /// Whether the candidate bytes were readable.
    pub read_status: CandidateReadStatus,
}

/// Header candidate role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderCandidateRole {
    /// Primary header at offset 0.
    Primary,
    /// Backup header at the end of the container.
    Backup,
}

/// Backup header candidate state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeaderCandidateState {
    /// A distinct backup candidate exists.
    Candidate {
        /// Candidate role.
        role: HeaderCandidateRole,
        /// Candidate byte offset in the container.
        offset: u64,
        /// Candidate length in bytes.
        length: u64,
        /// Whether the candidate bytes were readable.
        read_status: CandidateReadStatus,
    },
    /// The backup candidate would overlap the primary candidate.
    OverlapsPrimary,
}

/// Result of attempting to read a header candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateReadStatus {
    /// Candidate bytes were read successfully.
    Readable,
    /// Candidate bytes could not be read.
    ReadFailed {
        /// Sanitized read error text.
        error: String,
    },
}

/// Safe non-secret metadata returned after opening a supported profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TcvcOpenInfo {
    /// Backend name.
    pub backend: &'static str,
    /// Supported profile identifier.
    pub profile: &'static str,
    /// Whether the opened volume is exposed as read-only.
    pub read_only: bool,
    /// Volume size reported by the decrypted header.
    pub volume_size: u64,
    /// Physical offset of the encrypted data area.
    pub data_offset: u64,
    /// Logical decrypted data length.
    pub data_length: u64,
    /// Header-reported sector size.
    pub sector_size: u32,
}

/// Conservative first-sector filesystem probe result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilesystemProbeCandidate {
    /// The first sector has FAT-like markers and geometry.
    FatLike,
    /// The first sector carries the exFAT OEM name (`"EXFAT   "`).
    ExFat,
    /// The first sector carries the NTFS OEM name (`"NTFS    "`).
    Ntfs,
    /// No supported filesystem candidate was recognized.
    Unknown,
}

/// Opened TC/VC-compatible volume for the current supported profile.
pub struct TcvcOpenedVolume {
    metadata: TcvcOpenInfo,
    data_key: [u8; DATA_KEY_LEN],
}

impl TcvcOpenedVolume {
    /// Returns safe non-secret metadata about the opened volume.
    pub fn metadata(&self) -> &TcvcOpenInfo {
        &self.metadata
    }

    /// Creates a read-only decrypted data reader over the opened volume.
    ///
    /// # Errors
    ///
    /// Returns [`TcvcOpenError`] when the decrypted header metadata describes a
    /// data area outside the encrypted container or outside the supported
    /// profile constraints.
    pub fn data_reader<'a>(
        &'a self,
        encrypted_reader: &'a dyn BlockReader,
    ) -> Result<TcvcDataReader<'a>, TcvcOpenError> {
        validate_data_area(&self.metadata, encrypted_reader)?;

        Ok(TcvcDataReader {
            encrypted_reader,
            data_offset: self.metadata.data_offset,
            data_length: self.metadata.data_length,
            data_key: &self.data_key,
        })
    }
}

impl fmt::Debug for TcvcOpenedVolume {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TcvcOpenedVolume")
            .field("metadata", &self.metadata)
            .finish_non_exhaustive()
    }
}

impl Drop for TcvcOpenedVolume {
    fn drop(&mut self) {
        self.data_key.zeroize();
    }
}

/// Identifies which KDF and PIM combination successfully opened the header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TcvcMatchedProfile {
    /// The KDF/hash algorithm that matched.
    pub kdf: TcvcKdf,
    /// Whether the default or a custom PIM was used.
    pub pim: PimState,
    /// Which header candidate (primary or backup) matched.
    pub header_role: HeaderCandidateRole,
}

impl TcvcMatchedProfile {
    /// Returns `true` if the default PIM was used to open the header.
    pub fn pim_is_default(&self) -> bool {
        self.pim.is_default()
    }

    /// Returns the explicit PIM value, or `None` if the default PIM was used.
    pub fn pim_value(&self) -> Option<u32> {
        self.pim.value()
    }
}

/// Opened TC/VC volume together with the KDF and PIM profile that matched.
pub struct TcvcMatchedOpenedVolume {
    inner: TcvcOpenedVolume,
    matched: TcvcMatchedProfile,
}

impl TcvcMatchedOpenedVolume {
    /// Returns the KDF/PIM profile that successfully opened the header.
    pub fn matched_profile(&self) -> &TcvcMatchedProfile {
        &self.matched
    }

    /// Returns safe non-secret metadata about the opened volume.
    pub fn metadata(&self) -> &TcvcOpenInfo {
        self.inner.metadata()
    }

    /// Creates a read-only decrypted data reader over the opened volume.
    ///
    /// # Errors
    ///
    /// Returns [`TcvcOpenError`] when the data area is outside the container
    /// or outside the supported profile constraints.
    pub fn data_reader<'a>(
        &'a self,
        encrypted_reader: &'a dyn BlockReader,
    ) -> Result<TcvcDataReader<'a>, TcvcOpenError> {
        self.inner.data_reader(encrypted_reader)
    }
}

impl fmt::Debug for TcvcMatchedOpenedVolume {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TcvcMatchedOpenedVolume")
            .field("matched", &self.matched)
            .finish_non_exhaustive()
    }
}

/// Read-only decrypted data-area reader for an opened TC/VC volume.
pub struct TcvcDataReader<'a> {
    encrypted_reader: &'a dyn BlockReader,
    data_offset: u64,
    data_length: u64,
    data_key: &'a [u8; DATA_KEY_LEN],
}

impl fmt::Debug for TcvcDataReader<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TcvcDataReader")
            .field("data_offset", &self.data_offset)
            .field("data_length", &self.data_length)
            .finish_non_exhaustive()
    }
}

/// Errors returned while opening or using a supported TC/VC profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TcvcOpenError {
    /// Authentication failed or the container uses unsupported parameters.
    AuthenticationOrUnsupported,
    /// The decrypted header describes a profile outside current support.
    UnsupportedProfile {
        /// Non-secret reason for rejecting the profile.
        reason: &'static str,
    },
    /// Header or data reads failed.
    ReadFailed {
        /// Sanitized read error text.
        error: String,
    },
    /// The provided PIM value is invalid (e.g. would overflow the iteration count).
    InvalidPim {
        /// Non-secret reason for rejecting the PIM value.
        reason: &'static str,
    },
}

impl fmt::Display for TcvcOpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AuthenticationOrUnsupported => {
                f.write_str("authentication failed or unsupported parameters")
            }
            Self::UnsupportedProfile { reason } => {
                write!(f, "unsupported TC/VC profile: {reason}")
            }
            Self::ReadFailed { .. } => f.write_str("failed to read TC/VC header candidate"),
            Self::InvalidPim { reason } => write!(f, "invalid PIM: {reason}"),
        }
    }
}

impl std::error::Error for TcvcOpenError {}

/// Inspects primary and backup TC/VC header candidate locations.
///
/// # Errors
///
/// Returns [`CryptovolError`] only for reader-level failures that prevent
/// inspection setup. Candidate-specific read failures are represented inside
/// [`CandidateReadStatus`].
pub fn inspect_header_candidates(
    reader: &dyn BlockReader,
) -> Result<TcvcInspection, CryptovolError> {
    let file_size = reader.len();

    if file_size < TCVC_HEADER_CANDIDATE_LEN {
        return Ok(TcvcInspection::TooSmall {
            file_size,
            required_minimum: TCVC_HEADER_CANDIDATE_LEN,
        });
    }

    let primary = HeaderCandidate {
        role: HeaderCandidateRole::Primary,
        offset: 0,
        length: TCVC_HEADER_CANDIDATE_LEN,
        read_status: read_candidate(reader, 0),
    };

    let backup = if file_size == TCVC_HEADER_CANDIDATE_LEN {
        HeaderCandidateState::OverlapsPrimary
    } else {
        let offset = file_size - TCVC_HEADER_CANDIDATE_LEN;
        HeaderCandidateState::Candidate {
            role: HeaderCandidateRole::Backup,
            offset,
            length: TCVC_HEADER_CANDIDATE_LEN,
            read_status: read_candidate(reader, offset),
        }
    };

    Ok(TcvcInspection::Candidates { primary, backup })
}

/// Opens a supported AES-XTS/SHA-512 header and returns non-secret metadata.
///
/// # Errors
///
/// Returns [`TcvcOpenError`] when authentication fails, parameters are outside
/// the supported profile, or the header cannot be read.
pub fn open_aes_sha512_header(
    reader: &dyn BlockReader,
    password: &[u8],
) -> Result<TcvcOpenInfo, TcvcOpenError> {
    open_aes_sha512_volume(reader, password).map(|volume| volume.metadata().clone())
}

/// Opens a supported AES-XTS/SHA-512 volume.
///
/// # Errors
///
/// Returns [`TcvcOpenError`] when authentication fails, parameters are outside
/// the supported profile, or required container bytes cannot be read.
pub fn open_aes_sha512_volume(
    reader: &dyn BlockReader,
    password: &[u8],
) -> Result<TcvcOpenedVolume, TcvcOpenError> {
    let mut encrypted_header = [0u8; TCVC_HEADER_CANDIDATE_LEN_USIZE];
    reader
        .read_at(0, &mut encrypted_header)
        .map_err(|error| TcvcOpenError::ReadFailed {
            error: error.to_string(),
        })?;

    let mut header_key = [0u8; HEADER_KEY_LEN];
    pbkdf2_hmac::<Sha512>(
        password,
        &encrypted_header[..HEADER_SALT_LEN],
        VERA_PIM0_SHA512_ITERATIONS,
        &mut header_key,
    );

    let mut decrypted_header = [0u8; TCVC_HEADER_CANDIDATE_LEN_USIZE];
    decrypted_header[ENCRYPTED_HEADER_OFFSET..]
        .copy_from_slice(&encrypted_header[ENCRYPTED_HEADER_OFFSET..]);
    decrypt_xts_area(
        &header_key,
        &mut decrypted_header[ENCRYPTED_HEADER_OFFSET..],
        0,
    );

    let result = validate_decrypted_header(&decrypted_header).map(|metadata| {
        let mut data_key = [0u8; DATA_KEY_LEN];
        data_key
            .copy_from_slice(&decrypted_header[KEY_AREA_OFFSET..KEY_AREA_OFFSET + DATA_KEY_LEN]);

        TcvcOpenedVolume { metadata, data_key }
    });

    decrypted_header.zeroize();
    header_key.zeroize();
    encrypted_header.zeroize();

    result
}

/// Opens a TC/VC-compatible volume using KDF autoprobing and optional PIM support.
///
/// Tries supported KDF/hash profiles in deterministic order (SHA-512, SHA-256) unless
/// a hint is provided via [`TcvcOpenOptions::kdf_hint`]. Checks the primary header
/// candidate first, then the backup header if the file is large enough. Returns the
/// matched profile and opened volume on success.
///
/// # Errors
///
/// Returns [`TcvcOpenError::InvalidPim`] when the PIM value overflows the iteration
/// count. Returns [`TcvcOpenError::ReadFailed`] when the container cannot be read.
/// Returns [`TcvcOpenError::AuthenticationOrUnsupported`] when no KDF/candidate pair
/// matches.
pub fn open_with_options(
    reader: &dyn BlockReader,
    options: &TcvcOpenOptions,
) -> Result<TcvcMatchedOpenedVolume, TcvcOpenError> {
    let iterations = compute_iterations(options.kdf_hint.unwrap_or(TcvcKdf::Sha512), options.pim)?;

    let kdfs: Vec<TcvcKdf> = match options.kdf_hint {
        Some(hint) => vec![hint],
        None => vec![
            TcvcKdf::Sha512,
            TcvcKdf::Sha256,
            TcvcKdf::Whirlpool,
            TcvcKdf::Blake2s256,
            TcvcKdf::Streebog,
        ],
    };

    let file_size = reader.len();
    let backup_offset = file_size
        .checked_sub(TCVC_HEADER_CANDIDATE_LEN)
        .filter(|&off| off > 0);

    let mut candidate_offsets = [(0u64, HeaderCandidateRole::Primary); 2];
    let candidate_count = if let Some(boff) = backup_offset {
        candidate_offsets[1] = (boff, HeaderCandidateRole::Backup);
        2
    } else {
        1
    };

    for &(offset, role) in &candidate_offsets[..candidate_count] {
        let mut encrypted_header = [0u8; TCVC_HEADER_CANDIDATE_LEN_USIZE];
        reader
            .read_at(offset, &mut encrypted_header)
            .map_err(|error| TcvcOpenError::ReadFailed {
                error: error.to_string(),
            })?;

        for &kdf in &kdfs {
            let mut header_key = [0u8; HEADER_KEY_LEN];
            match kdf {
                TcvcKdf::Sha512 => pbkdf2_hmac::<Sha512>(
                    &options.password,
                    &encrypted_header[..HEADER_SALT_LEN],
                    iterations,
                    &mut header_key,
                ),
                TcvcKdf::Sha256 => pbkdf2_hmac::<Sha256>(
                    &options.password,
                    &encrypted_header[..HEADER_SALT_LEN],
                    iterations,
                    &mut header_key,
                ),
                TcvcKdf::Whirlpool => pbkdf2_hmac::<Whirlpool>(
                    &options.password,
                    &encrypted_header[..HEADER_SALT_LEN],
                    iterations,
                    &mut header_key,
                ),
                TcvcKdf::Blake2s256 => {
                    pbkdf2_raw::<SimpleHmac<Blake2s256>>(
                        &options.password,
                        &encrypted_header[..HEADER_SALT_LEN],
                        iterations,
                        &mut header_key,
                    )
                    .map_err(|_| TcvcOpenError::UnsupportedProfile {
                        reason: "BLAKE2s-256 output length mismatch",
                    })?;
                }
                TcvcKdf::Streebog => pbkdf2_hmac::<Streebog512>(
                    &options.password,
                    &encrypted_header[..HEADER_SALT_LEN],
                    iterations,
                    &mut header_key,
                ),
            }

            let mut decrypted_header = [0u8; TCVC_HEADER_CANDIDATE_LEN_USIZE];
            decrypted_header[ENCRYPTED_HEADER_OFFSET..]
                .copy_from_slice(&encrypted_header[ENCRYPTED_HEADER_OFFSET..]);
            decrypt_xts_area(
                &header_key,
                &mut decrypted_header[ENCRYPTED_HEADER_OFFSET..],
                0,
            );

            let result = validate_decrypted_header(&decrypted_header).map(|metadata| {
                let mut data_key = [0u8; DATA_KEY_LEN];
                data_key.copy_from_slice(
                    &decrypted_header[KEY_AREA_OFFSET..KEY_AREA_OFFSET + DATA_KEY_LEN],
                );
                TcvcMatchedOpenedVolume {
                    inner: TcvcOpenedVolume { metadata, data_key },
                    matched: TcvcMatchedProfile {
                        kdf,
                        pim: PimState::from_option(options.pim),
                        header_role: role,
                    },
                }
            });

            decrypted_header.zeroize();
            header_key.zeroize();

            match result {
                Ok(vol) => {
                    encrypted_header.zeroize();
                    return Ok(vol);
                }
                Err(
                    TcvcOpenError::AuthenticationOrUnsupported
                    | TcvcOpenError::UnsupportedProfile { .. },
                ) => continue,
                Err(e) => {
                    encrypted_header.zeroize();
                    return Err(e);
                }
            }
        }

        encrypted_header.zeroize();
    }

    Err(TcvcOpenError::AuthenticationOrUnsupported)
}

/// Validates a decrypted TC/VC header for the current supported profile.
///
/// # Errors
///
/// Returns [`TcvcOpenError`] when integrity checks fail or header metadata is
/// unsupported.
pub fn validate_decrypted_header(header: &[u8; 512]) -> Result<TcvcOpenInfo, TcvcOpenError> {
    if &header[VERA_MAGIC_OFFSET..VERA_MAGIC_OFFSET + VERA_MAGIC.len()] != VERA_MAGIC {
        return Err(TcvcOpenError::AuthenticationOrUnsupported);
    }

    let expected_key_area_crc = read_u32_be(header, HEADER_KEY_AREA_CRC_OFFSET);
    if crc32(&header[KEY_AREA_OFFSET..]) != expected_key_area_crc {
        return Err(TcvcOpenError::AuthenticationOrUnsupported);
    }

    let expected_header_crc = read_u32_be(header, HEADER_CRC_OFFSET);
    if crc32(&header[VERA_MAGIC_OFFSET..HEADER_CRC_OFFSET]) != expected_header_crc {
        return Err(TcvcOpenError::AuthenticationOrUnsupported);
    }

    if read_u64_be(header, HIDDEN_VOLUME_SIZE_OFFSET) != 0 {
        return Err(TcvcOpenError::UnsupportedProfile {
            reason: "hidden volumes are not supported",
        });
    }

    let flags = read_u32_be(header, HEADER_FLAGS_OFFSET);
    if flags & UNSUPPORTED_SYSTEM_OR_IN_PLACE_FLAGS != 0 {
        return Err(TcvcOpenError::UnsupportedProfile {
            reason: "system or in-place encrypted volumes are not supported",
        });
    }

    Ok(TcvcOpenInfo {
        backend: BACKEND_NAME,
        profile: SUPPORTED_PROFILE,
        read_only: true,
        volume_size: read_u64_be(header, VOLUME_SIZE_OFFSET),
        data_offset: read_u64_be(header, DATA_OFFSET_OFFSET),
        data_length: read_u64_be(header, DATA_LENGTH_OFFSET),
        sector_size: read_u32_be(header, SECTOR_SIZE_OFFSET),
    })
}

/// Probes the first decrypted sector for a conservative filesystem candidate.
///
/// # Errors
///
/// Returns [`CryptovolError`] when the first sector cannot be read.
pub fn probe_filesystem(
    reader: &dyn BlockReader,
) -> Result<FilesystemProbeCandidate, CryptovolError> {
    let mut sector = [0u8; SUPPORTED_SECTOR_SIZE_USIZE];
    reader.read_at(0, &mut sector)?;

    let candidate = if &sector[3..11] == b"EXFAT   " {
        FilesystemProbeCandidate::ExFat
    } else if &sector[3..11] == b"NTFS    " {
        FilesystemProbeCandidate::Ntfs
    } else if is_fat_like_boot_sector(&sector) {
        FilesystemProbeCandidate::FatLike
    } else {
        FilesystemProbeCandidate::Unknown
    };

    sector.zeroize();

    Ok(candidate)
}

impl BlockReader for TcvcDataReader<'_> {
    fn len(&self) -> u64 {
        self.data_length
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<(), CryptovolError> {
        validate_logical_range(offset, buf.len(), self.data_length)?;

        if buf.is_empty() {
            return Ok(());
        }

        let sector_size = u64::from(SUPPORTED_SECTOR_SIZE);
        let first_sector = offset / sector_size;
        let sector_offset =
            usize::try_from(offset % sector_size).map_err(|_| CryptovolError::OutOfBounds {
                offset,
                length: buf.len(),
                file_len: self.data_length,
            })?;
        let length_u64 = u64::try_from(buf.len()).map_err(|_| CryptovolError::OutOfBounds {
            offset,
            length: buf.len(),
            file_len: self.data_length,
        })?;
        let end = offset
            .checked_add(length_u64)
            .ok_or(CryptovolError::OutOfBounds {
                offset,
                length: buf.len(),
                file_len: self.data_length,
            })?;
        let sector_end = end.div_ceil(sector_size);
        let sector_count = sector_end - first_sector;
        let sector_buf_len_u64 =
            sector_count
                .checked_mul(sector_size)
                .ok_or(CryptovolError::OutOfBounds {
                    offset,
                    length: buf.len(),
                    file_len: self.data_length,
                })?;
        let sector_buf_len =
            usize::try_from(sector_buf_len_u64).map_err(|_| CryptovolError::OutOfBounds {
                offset,
                length: buf.len(),
                file_len: self.data_length,
            })?;
        let mut sector_buf = vec![0u8; sector_buf_len];
        let physical_offset = self
            .data_offset
            .checked_add(first_sector.checked_mul(sector_size).ok_or(
                CryptovolError::OutOfBounds {
                    offset,
                    length: buf.len(),
                    file_len: self.data_length,
                },
            )?)
            .ok_or(CryptovolError::OutOfBounds {
                offset,
                length: buf.len(),
                file_len: self.data_length,
            })?;

        self.encrypted_reader
            .read_at(physical_offset, &mut sector_buf)?;
        decrypt_xts_area(
            self.data_key,
            &mut sector_buf,
            physical_offset / sector_size,
        );

        let copy_end = sector_offset
            .checked_add(buf.len())
            .ok_or(CryptovolError::OutOfBounds {
                offset,
                length: buf.len(),
                file_len: self.data_length,
            })?;
        buf.copy_from_slice(&sector_buf[sector_offset..copy_end]);
        sector_buf.zeroize();

        Ok(())
    }
}

fn is_fat_like_boot_sector(sector: &[u8; 512]) -> bool {
    has_boot_signature(sector)
        && (has_exfat_oem_marker(sector)
            || has_fat_type_marker(sector)
            || (has_x86_boot_jump(sector)
                && (has_fat_oem_marker(sector) || has_fat_bpb_geometry(sector))))
}

fn has_boot_signature(sector: &[u8; 512]) -> bool {
    sector[510] == 0x55 && sector[511] == 0xaa
}

fn has_x86_boot_jump(sector: &[u8; 512]) -> bool {
    matches!(sector[0], 0xeb | 0xe9)
}

fn has_fat_oem_marker(sector: &[u8; 512]) -> bool {
    let oem = &sector[3..11];

    oem.starts_with(b"MSDOS")
        || oem.starts_with(b"MSWIN")
        || oem.starts_with(b"mkfs.fat")
        || oem.starts_with(b"FRDOS")
}

fn has_exfat_oem_marker(sector: &[u8; 512]) -> bool {
    &sector[3..11] == b"EXFAT   "
}

fn has_fat_type_marker(sector: &[u8; 512]) -> bool {
    sector[54..62].windows(3).any(|window| window == b"FAT")
        || sector[82..90].windows(3).any(|window| window == b"FAT")
}

fn has_fat_bpb_geometry(sector: &[u8; 512]) -> bool {
    let bytes_per_sector = u16::from_le_bytes([sector[11], sector[12]]);
    let sectors_per_cluster = sector[13];

    matches!(bytes_per_sector, 512 | 1024 | 2048 | 4096)
        && sectors_per_cluster.is_power_of_two()
        && sectors_per_cluster > 0
}

fn validate_logical_range(
    offset: u64,
    length: usize,
    data_length: u64,
) -> Result<(), CryptovolError> {
    let length_u64 = u64::try_from(length).map_err(|_| CryptovolError::OutOfBounds {
        offset,
        length,
        file_len: data_length,
    })?;
    let end = offset
        .checked_add(length_u64)
        .ok_or(CryptovolError::OutOfBounds {
            offset,
            length,
            file_len: data_length,
        })?;

    if end > data_length {
        return Err(CryptovolError::OutOfBounds {
            offset,
            length,
            file_len: data_length,
        });
    }

    Ok(())
}

fn validate_data_area(
    metadata: &TcvcOpenInfo,
    encrypted_reader: &dyn BlockReader,
) -> Result<(), TcvcOpenError> {
    if metadata.sector_size != SUPPORTED_SECTOR_SIZE {
        return Err(TcvcOpenError::UnsupportedProfile {
            reason: "only 512-byte sectors are supported",
        });
    }

    if metadata.data_length == 0 {
        return Err(TcvcOpenError::UnsupportedProfile {
            reason: "empty data areas are not supported",
        });
    }

    if metadata.data_offset % u64::from(SUPPORTED_SECTOR_SIZE) != 0
        || metadata.data_length % u64::from(SUPPORTED_SECTOR_SIZE) != 0
    {
        return Err(TcvcOpenError::UnsupportedProfile {
            reason: "data area is not sector aligned",
        });
    }

    let data_end = metadata
        .data_offset
        .checked_add(metadata.data_length)
        .ok_or(TcvcOpenError::UnsupportedProfile {
            reason: "data area range overflows",
        })?;

    if data_end > encrypted_reader.len() {
        return Err(TcvcOpenError::UnsupportedProfile {
            reason: "data area extends beyond container",
        });
    }

    Ok(())
}

fn decrypt_xts_area(key: &[u8; DATA_KEY_LEN], payload: &mut [u8], first_sector_index: u64) {
    let xts = xts_from_key(key);

    xts.decrypt_area(
        payload,
        SUPPORTED_SECTOR_SIZE_USIZE,
        u128::from(first_sector_index),
        get_tweak_default,
    );
}

fn xts_from_key(key: &[u8; DATA_KEY_LEN]) -> Xts128<Aes256> {
    let mut key_1 = [0u8; AES_KEY_LEN];
    let mut key_2 = [0u8; AES_KEY_LEN];
    key_1.copy_from_slice(&key[..AES_KEY_LEN]);
    key_2.copy_from_slice(&key[AES_KEY_LEN..DATA_KEY_LEN]);

    let cipher_1 = Aes256::new(&key_1.into());
    let cipher_2 = Aes256::new(&key_2.into());
    key_1.zeroize();
    key_2.zeroize();

    Xts128::<Aes256>::new(cipher_1, cipher_2)
}

fn crc32(bytes: &[u8]) -> u32 {
    crc32fast::hash(bytes)
}

fn read_u32_be(buf: &[u8], offset: usize) -> u32 {
    let mut bytes = [0u8; 4];
    bytes.copy_from_slice(&buf[offset..offset + 4]);
    u32::from_be_bytes(bytes)
}

fn read_u64_be(buf: &[u8], offset: usize) -> u64 {
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&buf[offset..offset + 8]);
    u64::from_be_bytes(bytes)
}

fn read_candidate(reader: &dyn BlockReader, offset: u64) -> CandidateReadStatus {
    let mut candidate = [0u8; TCVC_HEADER_CANDIDATE_LEN_USIZE];

    match reader.read_at(offset, &mut candidate) {
        Ok(()) => CandidateReadStatus::Readable,
        Err(error) => CandidateReadStatus::ReadFailed {
            error: error.to_string(),
        },
    }
}

impl CryptoVolumeBackend for TcvcBackend {
    type Inspection = TcvcInspection;

    fn name(&self) -> &'static str {
        BACKEND_NAME
    }

    fn inspect(&self, reader: &dyn BlockReader) -> Result<Self::Inspection, CryptovolError> {
        inspect_header_candidates(reader)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EmptyReader;

    impl BlockReader for EmptyReader {
        fn len(&self) -> u64 {
            0
        }

        fn read_at(&self, _offset: u64, _buf: &mut [u8]) -> Result<(), CryptovolError> {
            Ok(())
        }
    }

    #[test]
    fn backend_exposes_tcvc_name() {
        let backend = TcvcBackend::new();

        assert_eq!(backend.name(), "tcvc");
    }

    #[test]
    fn backend_returns_core_inspection_metadata() {
        let backend = TcvcBackend::new();
        let reader = EmptyReader;

        let inspection = backend.inspect(&reader).expect("inspection should succeed");

        assert_eq!(
            inspection,
            TcvcInspection::TooSmall {
                file_size: 0,
                required_minimum: TCVC_HEADER_CANDIDATE_LEN,
            }
        );
    }
}
