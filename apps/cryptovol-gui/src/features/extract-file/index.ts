import { selectExtractDestination } from "@/shared/api/commands";

export { useExtractFile } from "@/features/extract-file/model/useExtractFile";

/**
 * Opens a native save-file picker for an extraction destination. Resolves
 * to `null` if the user cancels.
 */
export function browseDestination(defaultFileName?: string): Promise<string | null> {
  return selectExtractDestination(defaultFileName);
}
