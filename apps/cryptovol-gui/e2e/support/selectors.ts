/**
 * Single source of truth for every stable data-testid the E2E specs query,
 * mirroring the selector list in apps/cryptovol-gui/src and documented in
 * docs/gui-testing.md. Keep in sync with the frontend when selectors change.
 */
export const SEL = {
  openVolumePage: "open-volume-page",
  openContainerPathInput: "open-container-path-input",
  openContainerBrowseButton: "open-container-browse-button",
  openContainerPasswordInput: "open-container-password-input",
  openContainerPimInput: "open-container-pim-input",
  openContainerKdfSelect: "open-container-kdf-select",
  openContainerSubmit: "open-container-submit",
  openContainerError: "open-container-error",

  volumeBrowserPage: "volume-browser-page",
  volumeBrowserBackButton: "volume-browser-back-button",
  volumeInfoPanel: "volume-info-panel",
  volumeInfoFilesystem: "volume-info-filesystem",
  volumeInfoKdf: "volume-info-kdf",
  volumeInfoReadonly: "volume-info-readonly",

  directoryBrowser: "directory-browser",
  directoryCurrentPath: "directory-current-path",
  directoryUpButton: "directory-up-button",
  directoryRefreshButton: "directory-refresh-button",
  directoryEntryRow: "directory-entry-row",

  selectedEntryPanel: "selected-entry-panel",
  selectedEntryName: "selected-entry-name",
  selectedEntryPath: "selected-entry-path",
  selectedEntryError: "selected-entry-error",

  extractionPanel: "extraction-panel",
  extractDestinationInput: "extract-destination-input",
  extractDestinationBrowseButton: "extract-destination-browse-button",
  extractSubmit: "extract-submit",
  extractCancel: "extract-cancel",
  extractProgress: "extract-progress",
  extractProgressBytes: "extract-progress-bytes",
  extractSuccess: "extract-success",
  extractError: "extract-error",

  globalErrorAlert: "global-error-alert",
} as const;
