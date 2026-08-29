/**
 * Byte-for-byte file comparison for the FAT extraction regression spec:
 * verifies an extracted file's bytes exactly match its ground-truth
 * original under testdata/static/fs-fat-lfn-original/.
 */

import { readFile } from "node:fs/promises";

/** Returns true iff the two files' contents are byte-for-byte identical. */
export async function filesAreByteIdentical(pathA: string, pathB: string): Promise<boolean> {
  const [contentA, contentB] = await Promise.all([readFile(pathA), readFile(pathB)]);
  return contentA.equals(contentB);
}
