import { selectContainerFile } from "@/shared/api/commands";

export { useOpenContainer } from "@/features/open-container/model/useOpenContainer";
export type { OpenVolumeState } from "@/features/open-container/model/useOpenContainer";

/**
 * Opens a native file picker for selecting a container file. Resolves to
 * `null` if the user cancels.
 */
export function browseContainerFile(): Promise<string | null> {
  return selectContainerFile();
}
