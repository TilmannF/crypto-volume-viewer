/**
 * Typed frontend error model mirroring `GuiErrorDto`. The backend's
 * `message` field is already a short, sanitized, user-facing string; this
 * module adds a stable per-code title for UI headings (e.g. an Alert's
 * title) and a safe fallback for any code the frontend doesn't recognize.
 */

/** Frontend error model. Mirrors `GuiErrorDto` from dto.ts. */
export type GuiError = {
  code: string;
  message: string;
};

const ERROR_TITLES: Record<string, string> = {
  auth_failed: "Authentication failed",
  unsupported_format: "Unsupported format",
  filesystem_not_recognized: "Filesystem not recognized",
  path_not_found: "Not found",
  directory_extraction_unsupported: "Cannot extract a directory",
  unsupported_feature: "Unsupported feature",
  cancelled: "Cancelled",
  io_error: "Read/write error",
  invalid_input: "Invalid input",
  session_not_found: "Session no longer open",
  job_not_found: "Extraction job not found",
  internal_error: "Something went wrong",
};

const DEFAULT_ERROR_TITLE = "Something went wrong";

/** Returns a short, stable title for `error.code`, for use as e.g. an Alert heading. */
export function errorTitle(error: GuiError): string {
  return ERROR_TITLES[error.code] ?? DEFAULT_ERROR_TITLE;
}
