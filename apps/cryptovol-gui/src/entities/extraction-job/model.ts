/**
 * Extraction-job state machine (30-frontend-policy.md section 14). No
 * Tauri imports, no React -- `features/extract-file` owns wiring this up to
 * `shared/api/commands.subscribeToExtractionEvents`.
 */

import type {
  ExtractCancelledEventDto,
  ExtractFailedEventDto,
  ExtractFinishedEventDto,
  ExtractProgressEventDto,
  ExtractStartedDto,
  ExtractStartedEventDto,
} from "@/shared/api/dto";
import type { GuiError } from "@/shared/api/errors";

/** Discriminated-union state for a single extraction job. */
export type ExtractionState =
  | { status: "idle" }
  | { status: "running"; jobId: string; bytesWritten: number; totalBytes?: number }
  | { status: "finished"; bytesWritten: number }
  | { status: "cancelled" }
  | { status: "failed"; error: GuiError };

/** The idle state, before any extraction has started. */
export const IDLE_EXTRACTION_STATE: ExtractionState = { status: "idle" };

/** Builds the initial running state from `extract_file`'s immediate response. */
export function startingExtractionState(started: ExtractStartedDto): ExtractionState {
  return {
    status: "running",
    jobId: started.jobId,
    bytesWritten: 0,
    totalBytes: started.totalBytes ?? undefined,
  };
}

/** A single raw `extract://*` event, tagged by kind. */
export type ExtractionLifecycleEvent =
  | { kind: "started"; payload: ExtractStartedEventDto }
  | { kind: "progress"; payload: ExtractProgressEventDto }
  | { kind: "finished"; payload: ExtractFinishedEventDto }
  | { kind: "cancelled"; payload: ExtractCancelledEventDto }
  | { kind: "failed"; payload: ExtractFailedEventDto };

/**
 * Applies a single lifecycle event to `state`, returning the next state.
 * Pure: does not touch the network, storage, or any Tauri API.
 */
export function applyExtractionEvent(
  _state: ExtractionState,
  event: ExtractionLifecycleEvent,
): ExtractionState {
  switch (event.kind) {
    case "started":
      return {
        status: "running",
        jobId: event.payload.jobId,
        bytesWritten: 0,
        totalBytes: event.payload.totalBytes ?? undefined,
      };
    case "progress":
      return {
        status: "running",
        jobId: event.payload.jobId,
        bytesWritten: event.payload.bytesWritten,
        totalBytes: event.payload.totalBytes ?? undefined,
      };
    case "finished":
      return { status: "finished", bytesWritten: event.payload.bytesWritten };
    case "cancelled":
      return { status: "cancelled" };
    case "failed":
      return {
        status: "failed",
        error: { code: event.payload.code, message: event.payload.message },
      };
    default: {
      const exhaustive: never = event;
      return exhaustive;
    }
  }
}
