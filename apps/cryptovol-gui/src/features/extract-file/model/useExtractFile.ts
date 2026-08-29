/**
 * Starts a single-file extraction and drives `ExtractionState` from the
 * backend's `extract://*` events.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { extractFile, subscribeToExtractionEvents } from "@/shared/api/commands";
import type { FileEntryDto } from "@/shared/api/dto";
import type { GuiError } from "@/shared/api/errors";
import {
  applyExtractionEvent,
  IDLE_EXTRACTION_STATE,
  startingExtractionState,
  type ExtractionState,
} from "@/entities/extraction-job";

const DIRECTORY_EXTRACTION_UNSUPPORTED: GuiError = {
  code: "directory_extraction_unsupported",
  message: "Cannot extract a directory. Select a file instead.",
};

export type UseExtractFileResult = {
  state: ExtractionState;
  /**
   * Starts extracting `sourceEntry` to `destinationPath`. Rejects a
   * directory entry locally (no round-trip) with the same
   * `directory_extraction_unsupported` shape the backend itself returns.
   */
  start: (sessionId: string, sourceEntry: FileEntryDto, destinationPath: string) => Promise<void>;
  /** Unsubscribes from any active job's events and resets to idle. */
  reset: () => void;
};

export function useExtractFile(): UseExtractFileResult {
  const [state, setState] = useState<ExtractionState>(IDLE_EXTRACTION_STATE);
  const unsubscribeRef = useRef<(() => void) | null>(null);

  const unsubscribe = useCallback(() => {
    unsubscribeRef.current?.();
    unsubscribeRef.current = null;
  }, []);

  useEffect(() => unsubscribe, [unsubscribe]);

  const start = useCallback(
    async (sessionId: string, sourceEntry: FileEntryDto, destinationPath: string) => {
      if (sourceEntry.isDir) {
        setState({ status: "failed", error: DIRECTORY_EXTRACTION_UNSUPPORTED });
        return;
      }
      if (!sourceEntry.path) {
        setState({
          status: "failed",
          error: { code: "invalid_input", message: "Selected entry has no known path." },
        });
        return;
      }

      unsubscribe();

      try {
        // Subscribed before `extractFile` is even called: on the backend,
        // `extract_file` synchronously spawns the copy thread before
        // returning, so a fast/small job can otherwise finish (and emit
        // every lifecycle event) before a listener exists. Registering
        // listeners first closes that race regardless of file size or IPC
        // speed -- see docs/known-issues.md.
        const subscription = await subscribeToExtractionEvents({
          onStarted: (event) =>
            setState((prev) => applyExtractionEvent(prev, { kind: "started", payload: event })),
          onProgress: (event) =>
            setState((prev) => applyExtractionEvent(prev, { kind: "progress", payload: event })),
          onFinished: (event) =>
            setState((prev) => applyExtractionEvent(prev, { kind: "finished", payload: event })),
          onCancelled: (event) =>
            setState((prev) => applyExtractionEvent(prev, { kind: "cancelled", payload: event })),
          onFailed: (event) =>
            setState((prev) => applyExtractionEvent(prev, { kind: "failed", payload: event })),
        });
        unsubscribeRef.current = subscription.unsubscribe;

        const started = await extractFile({
          sessionId,
          sourcePath: sourceEntry.path,
          destinationPath,
          overwrite: false,
          parents: false,
        });
        setState(startingExtractionState(started));
        subscription.bindJobId(started.jobId);
      } catch (error) {
        unsubscribe();
        setState({ status: "failed", error: error as GuiError });
      }
    },
    [unsubscribe],
  );

  const reset = useCallback(() => {
    unsubscribe();
    setState(IDLE_EXTRACTION_STATE);
  }, [unsubscribe]);

  return { state, start, reset };
}
