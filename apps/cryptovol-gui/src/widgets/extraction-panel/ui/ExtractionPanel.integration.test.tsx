import { act, render, screen } from "@testing-library/react";
import userEvent, { type UserEvent } from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { VolumeBrowserPage } from "@/pages/volume-browser";
import {
  cancelExtract,
  extractFile,
  listDir,
  stat,
  subscribeToExtractionEvents,
  type ExtractionEventHandlers,
} from "@/shared/api/commands";
import {
  buildExtractFailedEventDto,
  buildExtractFinishedEventDto,
  buildExtractProgressEventDto,
  buildExtractStartedDto,
  buildFileEntryDto,
  buildVolumeInfoDto,
} from "@/shared/testing";

vi.mock("@/shared/api/commands");

const FILE_ENTRY = buildFileEntryDto({ name: "notes.txt", isDir: false, path: "/notes.txt" });

/** Renders VolumeBrowserPage with one selectable file already selected. */
async function openWithSelectedFile(): Promise<UserEvent> {
  vi.mocked(listDir).mockResolvedValue([FILE_ENTRY]);
  vi.mocked(stat).mockResolvedValue(FILE_ENTRY);
  const user = userEvent.setup();
  render(<VolumeBrowserPage sessionId="s1" volumeInfo={buildVolumeInfoDto()} onClosed={vi.fn()} />);
  await user.click(await screen.findByTestId("directory-entry-row"));
  await screen.findByTestId("selected-entry-panel");
  return user;
}

/** Configures subscribeToExtractionEvents to capture, rather than call, its handlers. */
function captureExtractionHandlers(): { current: ExtractionEventHandlers | null } {
  const captured: { current: ExtractionEventHandlers | null } = { current: null };
  vi.mocked(subscribeToExtractionEvents).mockImplementation(async (handlers) => {
    captured.current = handlers;
    return { bindJobId: () => {}, unsubscribe: () => {} };
  });
  return captured;
}

async function startExtraction(user: UserEvent): Promise<void> {
  await user.type(screen.getByTestId("extract-destination-input"), "/tmp/notes.txt");
  await user.click(screen.getByTestId("extract-submit"));
}

describe("Extraction start/progress/cancel wiring", () => {
  it("calls extractFile with the expected request DTO on submit", async () => {
    const user = await openWithSelectedFile();
    vi.mocked(extractFile).mockResolvedValue(
      buildExtractStartedDto({ jobId: "job-1", totalBytes: 100 }),
    );
    captureExtractionHandlers();

    await startExtraction(user);

    expect(extractFile).toHaveBeenCalledWith({
      sessionId: "s1",
      sourcePath: "/notes.txt",
      destinationPath: "/tmp/notes.txt",
      overwrite: false,
      parents: false,
    });
  });

  it("subscribes to extraction events before invoking extractFile, so no event can be missed", async () => {
    // Regression for the GUI extraction event race (docs/known-issues.md):
    // a fast backend job can emit and drop every extract://* event before a
    // listener exists if extractFile is called first. Fails against the
    // pre-fix ordering, where extractFile was awaited before subscribing.
    const user = await openWithSelectedFile();
    vi.mocked(extractFile).mockResolvedValue(
      buildExtractStartedDto({ jobId: "job-1", totalBytes: 100 }),
    );
    captureExtractionHandlers();

    await startExtraction(user);

    const subscribeOrder = vi.mocked(subscribeToExtractionEvents).mock.invocationCallOrder[0];
    const extractOrder = vi.mocked(extractFile).mock.invocationCallOrder[0];
    expect(subscribeOrder).toBeLessThan(extractOrder);
  });

  it("reflects a progress event as byte counts only, with no raw content", async () => {
    const user = await openWithSelectedFile();
    vi.mocked(extractFile).mockResolvedValue(
      buildExtractStartedDto({ jobId: "job-1", totalBytes: 100 }),
    );
    const handlers = captureExtractionHandlers();

    await startExtraction(user);
    await screen.findByTestId("extract-progress");

    act(() => {
      handlers.current?.onProgress?.(
        buildExtractProgressEventDto({ jobId: "job-1", bytesWritten: 50, totalBytes: 100 }),
      );
    });

    expect(screen.getByTestId("extract-progress")).toBeInTheDocument();
    const bytesText = screen.getByTestId("extract-progress-bytes").textContent ?? "";
    expect(bytesText).toMatch(/^[0-9A-Za-z.,/ ]+$/);
  });

  it("calls cancelExtract with the running job's id", async () => {
    const user = await openWithSelectedFile();
    vi.mocked(extractFile).mockResolvedValue(
      buildExtractStartedDto({ jobId: "job-1", totalBytes: 100 }),
    );
    captureExtractionHandlers();

    await startExtraction(user);
    await user.click(await screen.findByTestId("extract-cancel"));

    expect(cancelExtract).toHaveBeenCalledWith("job-1");
  });

  it("shows extract-success on a finished event", async () => {
    const user = await openWithSelectedFile();
    vi.mocked(extractFile).mockResolvedValue(
      buildExtractStartedDto({ jobId: "job-1", totalBytes: 100 }),
    );
    const handlers = captureExtractionHandlers();

    await startExtraction(user);
    act(() => {
      handlers.current?.onFinished?.(
        buildExtractFinishedEventDto({ jobId: "job-1", bytesWritten: 100 }),
      );
    });

    expect(await screen.findByTestId("extract-success")).toBeInTheDocument();
  });

  it("shows a sanitized extract-error on a failed event, with no stack-trace text", async () => {
    const user = await openWithSelectedFile();
    vi.mocked(extractFile).mockResolvedValue(
      buildExtractStartedDto({ jobId: "job-1", totalBytes: 100 }),
    );
    const handlers = captureExtractionHandlers();

    await startExtraction(user);
    act(() => {
      handlers.current?.onFailed?.(
        buildExtractFailedEventDto({
          jobId: "job-1",
          code: "io_error",
          message: "Could not write file.",
        }),
      );
    });

    const errorElement = await screen.findByTestId("extract-error");
    expect(errorElement).toHaveTextContent("Could not write file.");
    expect(errorElement.textContent ?? "").not.toMatch(/at Object\.|\.rs:/);
  });
});
