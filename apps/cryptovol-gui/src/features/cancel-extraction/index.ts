import { cancelExtract } from "@/shared/api/commands";

/**
 * Cancels the extraction job for `jobId` via the backend's real
 * `CancellationToken`. Must never be replaced with merely clearing local
 * UI state (30-frontend-policy.md section 14).
 */
export function cancel(jobId: string): Promise<void> {
  return cancelExtract(jobId);
}
