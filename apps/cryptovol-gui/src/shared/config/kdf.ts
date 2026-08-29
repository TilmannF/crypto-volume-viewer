/**
 * KDF option list for the Open Volume form's KDF Select, and PIM input
 * validation. The frontend only does obvious sanity checks here -- the
 * backend (parse_kdf_hint/parse_pim in src-tauri/src/dto/session.rs) is the
 * actual authority and must be trusted as the sole gate.
 */

import type { KdfHint } from "@/shared/api/dto";

/** One selectable KDF option: its display label and wire value (undefined for Auto). */
export type KdfOption = {
  label: string;
  value: KdfHint | undefined;
};

/** KDF choices for the Open Volume form, in display order. */
export const KDF_OPTIONS: readonly KdfOption[] = [
  { label: "Auto", value: undefined },
  { label: "SHA-512", value: "sha512" },
  { label: "SHA-256", value: "sha256" },
  { label: "Whirlpool", value: "whirlpool" },
  { label: "BLAKE2s-256", value: "blake2s" },
  { label: "Streebog", value: "streebog" },
] as const;

/** Result of validating a PIM form field's raw text input. */
export type PimValidationResult =
  | { ok: true; value: number | undefined }
  | { ok: false; message: string };

/**
 * Validates a PIM form field's raw text input ahead of calling the backend.
 * An empty string means "use the default". Only catches obvious input
 * errors (non-numeric, negative, non-integer); the backend performs the
 * real validation.
 */
export function validatePim(input: string): PimValidationResult {
  const trimmed = input.trim();
  if (trimmed === "") {
    return { ok: true, value: undefined };
  }

  if (!/^\d+$/.test(trimmed)) {
    return { ok: false, message: "PIM must be a whole, non-negative number." };
  }

  const value = Number(trimmed);
  if (!Number.isSafeInteger(value)) {
    return { ok: false, message: "PIM is too large." };
  }

  return { ok: true, value };
}
