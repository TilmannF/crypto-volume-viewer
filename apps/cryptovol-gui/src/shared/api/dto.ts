/**
 * TypeScript mirrors of the Rust DTOs exchanged with the Tauri backend
 * (apps/cryptovol-gui/src-tauri/src/dto/*.rs and src/events.rs). Field
 * names match the backend's `#[serde(rename_all = "camelCase")]` output
 * exactly. This file only defines shapes -- all values flow through
 * shared/api/commands.ts.
 */

/** Wire values for the KDF hint, matching cryptovol-cli's --kdf tokens. */
export type KdfHint = "sha512" | "sha256" | "whirlpool" | "blake2s" | "streebog";

/** Lowercase wire values for a detected/opened filesystem kind. */
export type FilesystemWire = "fat" | "exfat" | "ntfs" | "unknown";

/** Stable, sanitized error returned by every Tauri command. */
export interface GuiErrorDto {
  code: string;
  message: string;
}

/** Safe, non-secret metadata about a container file (no password needed). */
export interface GuiContainerInfoDto {
  containerPath: string;
  containerSizeBytes: number;
  /** "too_small" or "candidates". */
  headerState: string;
  requiredMinimumBytes: number | null;
}

/**
 * Request to open a container. Password lives only in local component
 * state; see 30-frontend-policy.md section 5.
 */
export interface OpenContainerRequestDto {
  containerPath: string;
  password: string;
  pim: number | null;
  kdfHint: KdfHint | null;
}

/** Response returned after successfully opening a container. */
export interface OpenContainerResponseDto {
  sessionId: string;
  volumeInfo: VolumeInfoDto;
}

/** Safe, non-secret metadata about an opened volume, rendered for display. */
export interface VolumeInfoDto {
  containerPath: string;
  containerSizeBytes: number;
  backend: string;
  cipher: string;
  kdf: KdfHint;
  /** "default" or "custom:<n>". */
  pim: string;
  headerRole: "primary" | "backup";
  filesystem: FilesystemWire;
  readOnly: boolean;
}

/** Filesystem attribute flags for a single entry. */
export interface FileAttributesDto {
  readOnly: boolean;
  hidden: boolean;
  system: boolean;
  directory: boolean;
  archive: boolean;
}

/** A calendar timestamp for display. */
export interface AppTimestampDto {
  year: number;
  month: number;
  day: number;
  hour: number;
  minute: number;
  second: number;
}

/** A single directory/file entry as shown to the frontend. */
export interface FileEntryDto {
  name: string;
  path: string | null;
  isDir: boolean;
  size: number;
  attributes: FileAttributesDto;
  created: AppTimestampDto | null;
  modified: AppTimestampDto | null;
  accessed: AppTimestampDto | null;
  filesystem: FilesystemWire;
}

/** Request to extract one file from an opened volume to a host destination. */
export interface ExtractFileRequestDto {
  sessionId: string;
  sourcePath: string;
  destinationPath: string;
  overwrite: boolean;
  parents: boolean;
}

/** Returned immediately after extract_file starts a background job. */
export interface ExtractStartedDto {
  jobId: string;
  sessionId: string;
  sourcePath: string;
  destinationPath: string;
  totalBytes: number | null;
}

/** Payload for the `extract://started` event. */
export interface ExtractStartedEventDto {
  jobId: string;
  sessionId: string;
  sourcePath: string;
  destinationPath: string;
  totalBytes: number | null;
}

/** Payload for the `extract://progress` event. */
export interface ExtractProgressEventDto {
  jobId: string;
  sessionId: string;
  bytesWritten: number;
  totalBytes: number | null;
}

/** Payload for the `extract://finished` event. */
export interface ExtractFinishedEventDto {
  jobId: string;
  sessionId: string;
  bytesWritten: number;
}

/** Payload for the `extract://cancelled` event. */
export interface ExtractCancelledEventDto {
  jobId: string;
  sessionId: string;
}

/** Payload for the `extract://failed` event. */
export interface ExtractFailedEventDto {
  jobId: string;
  sessionId: string;
  code: string;
  message: string;
}
