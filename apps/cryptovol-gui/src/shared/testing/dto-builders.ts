/**
 * DTO builders for frontend integration tests: small functions returning
 * realistic values matching `shared/api/dto.ts`/`shared/api/errors.ts`
 * exactly, each taking a `Partial<...>` overrides object merged over sane
 * defaults so a test only has to specify the fields it cares about.
 */

import type {
  ExtractCancelledEventDto,
  ExtractFailedEventDto,
  ExtractFinishedEventDto,
  ExtractProgressEventDto,
  ExtractStartedDto,
  FileAttributesDto,
  FileEntryDto,
  OpenContainerResponseDto,
  VolumeInfoDto,
} from "@/shared/api/dto";
import type { GuiError } from "@/shared/api/errors";

const DEFAULT_ATTRIBUTES: FileAttributesDto = {
  readOnly: false,
  hidden: false,
  system: false,
  directory: false,
  archive: false,
};

export function buildVolumeInfoDto(overrides: Partial<VolumeInfoDto> = {}): VolumeInfoDto {
  return {
    containerPath: "/tmp/example.hc",
    containerSizeBytes: 20_971_520,
    backend: "tcvc",
    cipher: "aes-xts",
    kdf: "sha512",
    pim: "default",
    headerRole: "primary",
    filesystem: "fat",
    readOnly: true,
    ...overrides,
  };
}

export function buildFileEntryDto(overrides: Partial<FileEntryDto> = {}): FileEntryDto {
  return {
    name: "example.txt",
    path: "/example.txt",
    isDir: false,
    size: 128,
    attributes: DEFAULT_ATTRIBUTES,
    created: null,
    modified: null,
    accessed: null,
    filesystem: "fat",
    ...overrides,
  };
}

export function buildOpenContainerResponseDto(
  overrides: Partial<OpenContainerResponseDto> = {},
): OpenContainerResponseDto {
  return {
    sessionId: "session-1",
    volumeInfo: buildVolumeInfoDto(),
    ...overrides,
  };
}

export function buildExtractStartedDto(overrides: Partial<ExtractStartedDto> = {}): ExtractStartedDto {
  return {
    jobId: "job-1",
    sessionId: "session-1",
    sourcePath: "/example.txt",
    destinationPath: "/tmp/example.txt",
    totalBytes: 128,
    ...overrides,
  };
}

export function buildExtractProgressEventDto(
  overrides: Partial<ExtractProgressEventDto> = {},
): ExtractProgressEventDto {
  return {
    jobId: "job-1",
    sessionId: "session-1",
    bytesWritten: 0,
    totalBytes: 128,
    ...overrides,
  };
}

export function buildExtractFinishedEventDto(
  overrides: Partial<ExtractFinishedEventDto> = {},
): ExtractFinishedEventDto {
  return {
    jobId: "job-1",
    sessionId: "session-1",
    bytesWritten: 128,
    ...overrides,
  };
}

export function buildExtractCancelledEventDto(
  overrides: Partial<ExtractCancelledEventDto> = {},
): ExtractCancelledEventDto {
  return {
    jobId: "job-1",
    sessionId: "session-1",
    ...overrides,
  };
}

export function buildExtractFailedEventDto(
  overrides: Partial<ExtractFailedEventDto> = {},
): ExtractFailedEventDto {
  return {
    jobId: "job-1",
    sessionId: "session-1",
    code: "io_error",
    message: "Could not write file.",
    ...overrides,
  };
}

export function buildGuiError(code: string, message: string): GuiError {
  return { code, message };
}
