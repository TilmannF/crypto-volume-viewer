//! DTO for password-free container inspection.

use cryptovol_app::{ContainerHeaderInspection, ContainerInfo};
use serde::Serialize;

/// Safe, non-secret metadata about a container file, gathered without a
/// password and without attempting decryption.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiContainerInfoDto {
    /// Path to the inspected container file.
    pub container_path: String,
    /// Container file size in bytes.
    pub container_size_bytes: u64,
    /// `"too_small"` if the file can't hold a header candidate, otherwise
    /// `"candidates"`.
    pub header_state: String,
    /// Minimum file size required for one candidate header, when the
    /// container is too small.
    pub required_minimum_bytes: Option<u64>,
}

impl From<ContainerInfo> for GuiContainerInfoDto {
    fn from(info: ContainerInfo) -> Self {
        let (header_state, required_minimum_bytes) = match info.header_inspection {
            ContainerHeaderInspection::TooSmall { required_minimum } => {
                ("too_small".to_string(), Some(required_minimum))
            }
            ContainerHeaderInspection::Candidates { .. } => ("candidates".to_string(), None),
        };

        Self {
            container_path: info.container_path.display().to_string(),
            container_size_bytes: info.container_size_bytes,
            header_state,
            required_minimum_bytes,
        }
    }
}
