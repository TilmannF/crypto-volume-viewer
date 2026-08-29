/**
 * Regression coverage for the GUI extraction event race (docs/known-issues.md):
 * a fast/small extraction job can emit every `extract://*` event before the
 * frontend has registered a listener for it, and Tauri's `emit` is
 * fire-and-forget (no replay for late subscribers). Unlike every other test
 * in this frontend, this file exercises `subscribeToExtractionEvents`'s real
 * implementation rather than mocking `@/shared/api/commands` wholesale --
 * the bug lived inside that function, below the boundary the rest of the
 * frontend test suite mocks at (see docs/gui-testing.md).
 *
 * `@tauri-apps/api/event`'s `listen` is mocked so the test can invoke a
 * captured event callback directly, in an order it controls -- this
 * reproduces the race deterministically, with no timers or wall-clock waits.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import { listen } from "@tauri-apps/api/event";
import { subscribeToExtractionEvents } from "@/shared/api/commands";
import {
  buildExtractCancelledEventDto,
  buildExtractFinishedEventDto,
  buildExtractProgressEventDto,
} from "@/shared/testing";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));

type ListenMock = {
  mock: { calls: Array<[string, (event: { payload: unknown }) => void]> };
};

/** Invokes the callback `listen` captured for `eventName`, simulating a real Tauri emit. */
function emit(eventName: string, payload: unknown): void {
  const registration = (listen as unknown as ListenMock).mock.calls.find(
    ([name]) => name === eventName,
  );
  if (!registration) throw new Error(`no listener registered for ${eventName}`);
  registration[1]({ payload });
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe("subscribeToExtractionEvents buffering (extraction event race regression)", () => {
  it("buffers an event emitted before bindJobId and replays it once the matching job id is bound", async () => {
    const onFinished = vi.fn();
    const subscription = await subscribeToExtractionEvents({ onFinished });

    emit("extract://finished", buildExtractFinishedEventDto({ jobId: "job-1", bytesWritten: 49 }));
    expect(onFinished).not.toHaveBeenCalled();

    subscription.bindJobId("job-1");

    expect(onFinished).toHaveBeenCalledTimes(1);
    expect(onFinished).toHaveBeenCalledWith(
      expect.objectContaining({ jobId: "job-1", bytesWritten: 49 }),
    );
  });

  it("discards a buffered event for a different job id once a non-matching id is bound", async () => {
    const onFinished = vi.fn();
    const subscription = await subscribeToExtractionEvents({ onFinished });

    emit("extract://finished", buildExtractFinishedEventDto({ jobId: "job-stale" }));
    subscription.bindJobId("job-1");

    expect(onFinished).not.toHaveBeenCalled();
  });

  it("dispatches an event arriving after bindJobId immediately, without buffering", async () => {
    const onProgress = vi.fn();
    const subscription = await subscribeToExtractionEvents({ onProgress });

    subscription.bindJobId("job-1");
    emit("extract://progress", buildExtractProgressEventDto({ jobId: "job-1", bytesWritten: 64 }));

    expect(onProgress).toHaveBeenCalledTimes(1);
    expect(onProgress).toHaveBeenCalledWith(
      expect.objectContaining({ jobId: "job-1", bytesWritten: 64 }),
    );
  });

  it("buffers and replays a cancelled event through the same path", async () => {
    const onCancelled = vi.fn();
    const subscription = await subscribeToExtractionEvents({ onCancelled });

    emit("extract://cancelled", buildExtractCancelledEventDto({ jobId: "job-1" }));
    expect(onCancelled).not.toHaveBeenCalled();

    subscription.bindJobId("job-1");

    expect(onCancelled).toHaveBeenCalledTimes(1);
  });
});
