/**
 * Pure display helpers for `VolumeInfoDto`. No Tauri imports, no React.
 */

import type { FilesystemWire, KdfHint, VolumeInfoDto } from "@/shared/api/dto";

const KDF_LABELS: Record<KdfHint, string> = {
  sha512: "SHA-512",
  sha256: "SHA-256",
  whirlpool: "Whirlpool",
  blake2s: "BLAKE2s-256",
  streebog: "Streebog",
};

const FILESYSTEM_LABELS: Record<FilesystemWire, string> = {
  fat: "FAT",
  exfat: "exFAT",
  ntfs: "NTFS",
  unknown: "Unknown",
};

/** Renders a KDF wire value as its display label (e.g. "sha512" -> "SHA-512"). */
export function formatKdf(kdf: KdfHint): string {
  return KDF_LABELS[kdf];
}

/** Renders a filesystem wire value as its display label. */
export function formatFilesystem(filesystem: FilesystemWire): string {
  return FILESYSTEM_LABELS[filesystem];
}

/** Renders the PIM display string ("default" / "custom:<n>") for display. */
export function formatPim(pim: string): string {
  if (pim === "default") {
    return "Default";
  }
  const match = /^custom:(\d+)$/.exec(pim);
  return match ? `Custom (${Number(match[1]).toLocaleString()})` : pim;
}

/** Renders the header role for display. */
export function formatHeaderRole(headerRole: VolumeInfoDto["headerRole"]): string {
  return headerRole === "primary" ? "Primary" : "Backup";
}

/** Renders a byte count as a human-readable size (e.g. 1536 -> "1.5 KB"). */
export function formatByteSize(bytes: number): string {
  const units = ["bytes", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }
  const precision = unitIndex === 0 ? 0 : 1;
  return `${value.toFixed(precision)} ${units[unitIndex]}`;
}

/** A `VolumeInfoDto`, fully rendered for the Volume Browser page's info panel. */
export type VolumeInfoView = {
  containerPath: string;
  containerSizeLabel: string;
  filesystemLabel: string;
  cipher: string;
  kdfLabel: string;
  pimLabel: string;
  headerRoleLabel: string;
  readOnly: boolean;
};

/** Builds a display-ready view of `info`. */
export function describeVolumeInfo(info: VolumeInfoDto): VolumeInfoView {
  return {
    containerPath: info.containerPath,
    containerSizeLabel: formatByteSize(info.containerSizeBytes),
    filesystemLabel: formatFilesystem(info.filesystem),
    cipher: info.cipher,
    kdfLabel: formatKdf(info.kdf),
    pimLabel: formatPim(info.pim),
    headerRoleLabel: formatHeaderRole(info.headerRole),
    readOnly: info.readOnly,
  };
}
