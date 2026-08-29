/**
 * The only file in this frontend allowed to import `invoke`/`listen` from
 * `@tauri-apps/api` (see 30-frontend-policy.md section 9). Wraps every
 * Tauri command behind one named, explicitly typed async function, and
 * extraction progress-event subscription behind
 * `subscribeToExtractionEvents`.
 */

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  ExtractCancelledEventDto,
  ExtractFailedEventDto,
  ExtractFileRequestDto,
  ExtractFinishedEventDto,
  ExtractProgressEventDto,
  ExtractStartedDto,
  ExtractStartedEventDto,
  FileEntryDto,
  GuiContainerInfoDto,
  OpenContainerRequestDto,
  OpenContainerResponseDto,
} from "@/shared/api/dto";
import type { GuiError } from "@/shared/api/errors";

/**
 * Narrows a rejected Tauri command call into a `GuiError`. A failing
 * command rejects with the exact `GuiErrorDto` value it returned via
 * `Err(...)` (serialized by Tauri as the rejection payload); this falls
 * back to a generic `internal_error` for anything else (e.g. a
 * transport-level failure), so callers never have to handle `unknown`.
 */
function toGuiError(error: unknown): GuiError {
  if (
    typeof error === "object" &&
    error !== null &&
    "code" in error &&
    "message" in error &&
    typeof (error as { code: unknown }).code === "string" &&
    typeof (error as { message: unknown }).message === "string"
  ) {
    return error as GuiError;
  }
  return { code: "internal_error", message: "An unexpected error occurred." };
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw toGuiError(error);
  }
}

/** Inspects a container's size and header-candidate state without a password. */
export function inspectContainer(path: string): Promise<GuiContainerInfoDto> {
  return call<GuiContainerInfoDto>("inspect_container", { path });
}

/** Opens a container, returning a fresh session id and safe volume info. */
export function openContainer(
  request: OpenContainerRequestDto,
): Promise<OpenContainerResponseDto> {
  return call<OpenContainerResponseDto>("open_container", { request });
}

/** Lists the entries of `path` within the session for `sessionId`. */
export function listDir(sessionId: string, path: string): Promise<FileEntryDto[]> {
  return call<FileEntryDto[]>("list_dir", { sessionId, path });
}

/** Returns metadata for a single entry at `path` within `sessionId`. */
export function stat(sessionId: string, path: string): Promise<FileEntryDto> {
  return call<FileEntryDto>("stat", { sessionId, path });
}

/** Closes the session for `sessionId`, cancelling any active extraction jobs first. */
export function closeSession(sessionId: string): Promise<void> {
  return call<void>("close_session", { sessionId });
}

/** Starts a background extraction job; returns immediately with its id and total bytes. */
export function extractFile(request: ExtractFileRequestDto): Promise<ExtractStartedDto> {
  return call<ExtractStartedDto>("extract_file", { request });
}

/** Cancels the extraction job for `jobId`. */
export function cancelExtract(jobId: string): Promise<void> {
  return call<void>("cancel_extract", { jobId });
}

/** Opens a native file picker for a container path. Resolves to `null` on cancel. */
export function selectContainerFile(): Promise<string | null> {
  return call<string | null>("select_container_file");
}

/** Opens a native save-file picker for an extraction destination. Resolves to `null` on cancel. */
export function selectExtractDestination(defaultFileName?: string): Promise<string | null> {
  return call<string | null>("select_extract_destination", {
    defaultFileName: defaultFileName ?? null,
  });
}

/** Handlers for a single extraction job's lifecycle events. */
export type ExtractionEventHandlers = {
  onStarted?: (event: ExtractStartedEventDto) => void;
  onProgress?: (event: ExtractProgressEventDto) => void;
  onFinished?: (event: ExtractFinishedEventDto) => void;
  onCancelled?: (event: ExtractCancelledEventDto) => void;
  onFailed?: (event: ExtractFailedEventDto) => void;
};

/** A live subscription to extraction events, not yet filtered to one job id. */
export type ExtractionSubscription = {
  /**
   * Binds the job id to filter and dispatch against. Any event received
   * before this is called is buffered (not dropped) and replayed, in
   * arrival order, the instant a matching id is bound. Call this exactly
   * once, as soon as the job id is known.
   */
  bindJobId: (jobId: string) => void;
  /** Stops listening for every `extract://*` event. */
  unsubscribe: () => void;
};

type BufferedExtractionEvent =
  | { kind: "started"; payload: ExtractStartedEventDto }
  | { kind: "progress"; payload: ExtractProgressEventDto }
  | { kind: "finished"; payload: ExtractFinishedEventDto }
  | { kind: "cancelled"; payload: ExtractCancelledEventDto }
  | { kind: "failed"; payload: ExtractFailedEventDto };

function dispatchBufferedEvent(
  handlers: ExtractionEventHandlers,
  entry: BufferedExtractionEvent,
): void {
  switch (entry.kind) {
    case "started":
      handlers.onStarted?.(entry.payload);
      return;
    case "progress":
      handlers.onProgress?.(entry.payload);
      return;
    case "finished":
      handlers.onFinished?.(entry.payload);
      return;
    case "cancelled":
      handlers.onCancelled?.(entry.payload);
      return;
    case "failed":
      handlers.onFailed?.(entry.payload);
      return;
    default: {
      const exhaustive: never = entry;
      return exhaustive;
    }
  }
}

/**
 * Subscribes to every `extract://*` event before the job id that will own
 * them is even known, so a fast backend job can never emit an event before a
 * listener exists (the previous jobId-first signature raced the backend:
 * see `docs/known-issues.md`). Events are ignored for jobs other than the
 * one eventually bound, but never dropped for the right one: anything that
 * arrives before `bindJobId` is buffered and replayed in arrival order once
 * bound. Callers must invoke `unsubscribe` on unmount (see
 * 30-frontend-policy.md section 14).
 */
export async function subscribeToExtractionEvents(
  handlers: ExtractionEventHandlers,
): Promise<ExtractionSubscription> {
  let boundJobId: string | null = null;
  const buffer: BufferedExtractionEvent[] = [];

  function handle(entry: BufferedExtractionEvent, payloadJobId: string): void {
    if (boundJobId === null) {
      buffer.push(entry);
      return;
    }
    if (payloadJobId === boundJobId) dispatchBufferedEvent(handlers, entry);
  }

  const unlistenFns: UnlistenFn[] = await Promise.all([
    listen<ExtractStartedEventDto>("extract://started", (event) => {
      handle({ kind: "started", payload: event.payload }, event.payload.jobId);
    }),
    listen<ExtractProgressEventDto>("extract://progress", (event) => {
      handle({ kind: "progress", payload: event.payload }, event.payload.jobId);
    }),
    listen<ExtractFinishedEventDto>("extract://finished", (event) => {
      handle({ kind: "finished", payload: event.payload }, event.payload.jobId);
    }),
    listen<ExtractCancelledEventDto>("extract://cancelled", (event) => {
      handle({ kind: "cancelled", payload: event.payload }, event.payload.jobId);
    }),
    listen<ExtractFailedEventDto>("extract://failed", (event) => {
      handle({ kind: "failed", payload: event.payload }, event.payload.jobId);
    }),
  ]);

  return {
    bindJobId(jobId: string) {
      boundJobId = jobId;
      const toReplay = buffer.filter((entry) => entry.payload.jobId === jobId);
      buffer.length = 0;
      for (const entry of toReplay) dispatchBufferedEvent(handlers, entry);
    },
    unsubscribe() {
      for (const unlisten of unlistenFns) unlisten();
    },
  };
}
