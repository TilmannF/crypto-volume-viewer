//! Password-free container inspection command.

use crate::dto::container::GuiContainerInfoDto;
use crate::dto::error::GuiErrorDto;

/// Inspects a container file's size and TC/VC header candidate state
/// without requiring a password and without attempting decryption.
///
/// # Errors
///
/// Returns a [`GuiErrorDto`] if the container file cannot be opened or its
/// size cannot be read.
pub fn inspect_container_impl(path: &str) -> Result<GuiContainerInfoDto, GuiErrorDto> {
    cryptovol_app::inspect_container(path)
        .map(GuiContainerInfoDto::from)
        .map_err(GuiErrorDto::from)
}

/// Tauri command wrapper for [`inspect_container_impl`].
#[tauri::command]
pub fn inspect_container(path: String) -> Result<GuiContainerInfoDto, GuiErrorDto> {
    inspect_container_impl(&path)
}
