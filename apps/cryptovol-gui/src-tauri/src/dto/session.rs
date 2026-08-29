//! DTOs and validation helpers for opening a container and describing an
//! opened session's safe metadata.

use crate::dto::error::{GuiErrorDto, INVALID_INPUT};
use crate::dto::filesystem_to_wire_str;
use cryptovol_app::{HeaderCandidateRole, PimState, TcvcKdf, VolumeInfo};
use serde::{Deserialize, Serialize};

/// Request to open a container.
///
/// Deliberately does not derive `Debug`, `Clone`, or `Copy` so the password
/// can't be accidentally logged, duplicated, or retained beyond this single
/// command call.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenContainerRequestDto {
    /// Path to the container file.
    pub container_path: String,
    /// Volume password, as entered by the user.
    pub password: String,
    /// Optional Personal Iterations Multiplier; `None` uses the default.
    pub pim: Option<u32>,
    /// Optional KDF/hash hint; `None` autoprobes all supported KDFs.
    pub kdf_hint: Option<String>,
}

/// Response returned after successfully opening a container.
///
/// Never includes the password.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenContainerResponseDto {
    /// Opaque session id for subsequent `list_dir`/`stat`/`extract_file` calls.
    pub session_id: String,
    /// Safe, non-secret metadata about the opened volume.
    pub volume_info: VolumeInfoDto,
}

/// Safe, non-secret metadata about an opened volume, rendered for display.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeInfoDto {
    /// Path to the container file.
    pub container_path: String,
    /// Container file size in bytes.
    pub container_size_bytes: u64,
    /// Crypto backend name (currently always `"tcvc"`).
    pub backend: String,
    /// Cipher in use (currently always `"AES-XTS"`).
    pub cipher: String,
    /// KDF/hash that matched, as a lowercase wire string (e.g. `"sha512"`).
    pub kdf: String,
    /// `"default"` or `"custom:<n>"`.
    pub pim: String,
    /// `"primary"` or `"backup"`.
    pub header_role: String,
    /// Detected filesystem kind, as a lowercase wire string.
    pub filesystem: String,
    /// Whether the volume is exposed read-only (always `true`).
    pub read_only: bool,
}

impl From<VolumeInfo> for VolumeInfoDto {
    fn from(info: VolumeInfo) -> Self {
        Self {
            container_path: info.container_path.display().to_string(),
            container_size_bytes: info.container_size_bytes,
            backend: info.backend.to_string(),
            cipher: info.cipher.to_string(),
            kdf: kdf_to_wire_str(info.kdf).to_string(),
            pim: pim_to_display_string(info.pim),
            header_role: header_role_to_wire_str(info.header_role).to_string(),
            filesystem: filesystem_to_wire_str(info.filesystem).to_string(),
            read_only: info.read_only,
        }
    }
}

fn kdf_to_wire_str(kdf: TcvcKdf) -> &'static str {
    match kdf {
        TcvcKdf::Sha512 => "sha512",
        TcvcKdf::Sha256 => "sha256",
        TcvcKdf::Whirlpool => "whirlpool",
        TcvcKdf::Blake2s256 => "blake2s",
        TcvcKdf::Streebog => "streebog",
    }
}

fn pim_to_display_string(pim: PimState) -> String {
    match pim {
        PimState::Default => "default".to_string(),
        PimState::Custom(value) => format!("custom:{value}"),
    }
}

fn header_role_to_wire_str(role: HeaderCandidateRole) -> &'static str {
    match role {
        HeaderCandidateRole::Primary => "primary",
        HeaderCandidateRole::Backup => "backup",
    }
}

/// Parses an optional KDF-hint wire string into a [`TcvcKdf`].
///
/// Accepts the same token set as `cryptovol-cli`'s `--kdf` flag (`"sha512"`,
/// `"sha256"`, `"whirlpool"`, `"blake2s"`, `"streebog"`); `None` means Auto
/// (autoprobe every supported KDF).
///
/// # Errors
///
/// Returns a [`GuiErrorDto`] with code [`INVALID_INPUT`] for any other value.
pub fn parse_kdf_hint(hint: Option<&str>) -> Result<Option<TcvcKdf>, GuiErrorDto> {
    match hint {
        None => Ok(None),
        Some("sha512") => Ok(Some(TcvcKdf::Sha512)),
        Some("sha256") => Ok(Some(TcvcKdf::Sha256)),
        Some("whirlpool") => Ok(Some(TcvcKdf::Whirlpool)),
        Some("blake2s") => Ok(Some(TcvcKdf::Blake2s256)),
        Some("streebog") => Ok(Some(TcvcKdf::Streebog)),
        Some(other) => Err(GuiErrorDto::new(
            INVALID_INPUT,
            format!(
                "unsupported KDF: {other}; supported values: sha512, sha256, whirlpool, blake2s, streebog"
            ),
        )),
    }
}

/// Validates a PIM value.
///
/// Every `u32` (including `0`, which `cryptovol_tcvc` treats as "use the
/// default") is currently accepted; `cryptovol_app`/`cryptovol_tcvc` remain
/// the actual authority on PIM validity during `open_volume`.
///
/// # Errors
///
/// Currently infallible; returns `Result` so future validation can be added
/// without changing the call sites.
pub fn parse_pim(pim: Option<u32>) -> Result<Option<u32>, GuiErrorDto> {
    Ok(pim)
}
