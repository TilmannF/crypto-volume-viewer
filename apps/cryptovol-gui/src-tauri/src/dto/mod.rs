//! Serde-friendly request/response types for the frontend/backend boundary.
//!
//! DTOs stay separate from `cryptovol_app`'s own types so the GUI can evolve
//! its wire shape independently and so secret-carrying request types (e.g.
//! the open-container password) can opt out of `Debug`/`Clone` explicitly.

pub mod container;
pub mod error;
pub mod extraction;
pub mod file_entry;
pub mod session;

use cryptovol_app::FilesystemKind;

/// Renders a [`FilesystemKind`] as the lowercase wire string shared by every
/// DTO that reports a filesystem kind.
pub(crate) fn filesystem_to_wire_str(filesystem: FilesystemKind) -> &'static str {
    match filesystem {
        FilesystemKind::Fat => "fat",
        FilesystemKind::ExFat => "exfat",
        FilesystemKind::Ntfs => "ntfs",
        FilesystemKind::Unknown => "unknown",
    }
}
