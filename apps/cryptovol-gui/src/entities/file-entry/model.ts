/**
 * Pure display helpers for `FileEntryDto`. No Tauri imports, no React.
 */

import type { AppTimestampDto, FileAttributesDto, FileEntryDto } from "@/shared/api/dto";

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

/** Renders an `AppTimestampDto` as a stable, locale-independent display string. */
export function formatTimestamp(timestamp: AppTimestampDto): string {
  const pad = (n: number): string => n.toString().padStart(2, "0");
  const date = `${timestamp.year.toString().padStart(4, "0")}-${pad(timestamp.month)}-${pad(timestamp.day)}`;
  const time = `${pad(timestamp.hour)}:${pad(timestamp.minute)}:${pad(timestamp.second)}`;
  return `${date} ${time}`;
}

/** A `FileEntryDto`, rendered for the directory browser's table rows. */
export type FileEntryView = {
  name: string;
  path: string | null;
  isDir: boolean;
  size: number;
  sizeLabel: string;
  modifiedLabel: string | null;
  attributes: FileAttributesDto;
};

/** Builds a display-ready view of `entry`. */
export function toFileEntryView(entry: FileEntryDto): FileEntryView {
  return {
    name: entry.name,
    path: entry.path,
    isDir: entry.isDir,
    size: entry.size,
    sizeLabel: entry.isDir ? "—" : formatByteSize(entry.size),
    modifiedLabel: entry.modified ? formatTimestamp(entry.modified) : null,
    attributes: entry.attributes,
  };
}
