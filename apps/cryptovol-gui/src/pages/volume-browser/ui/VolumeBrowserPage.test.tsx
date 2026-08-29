import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { VolumeBrowserPage } from "./VolumeBrowserPage";
import { listDir, stat } from "@/shared/api/commands";
import { buildFileEntryDto, buildVolumeInfoDto } from "@/shared/testing";

vi.mock("@/shared/api/commands");

const DIRECTORY_ENTRY = buildFileEntryDto({
  name: "Folder With Spaces",
  isDir: true,
  path: null,
  size: 0,
});
const FILE_ENTRY = buildFileEntryDto({ name: "notes.txt", isDir: false, path: null });

function findRow(name: string): HTMLElement {
  const row = screen
    .getAllByTestId("directory-entry-row")
    .find((element) => element.getAttribute("data-entry-name") === name);
  if (!row) {
    throw new Error(`No directory-entry-row found for "${name}"`);
  }
  return row;
}

async function renderOpened() {
  vi.mocked(listDir).mockResolvedValue([DIRECTORY_ENTRY, FILE_ENTRY]);
  render(
    <VolumeBrowserPage sessionId="s1" volumeInfo={buildVolumeInfoDto()} onClosed={vi.fn()} />,
  );
  await screen.findAllByTestId("directory-entry-row");
}

describe("VolumeBrowserPage selection and extraction gating", () => {
  it("selecting a directory shows the extraction-unsupported message and keeps extraction unavailable", async () => {
    const user = userEvent.setup();
    vi.mocked(stat).mockResolvedValue(DIRECTORY_ENTRY);
    await renderOpened();

    await user.click(findRow("Folder With Spaces"));

    expect(
      await screen.findByText(
        "Directory extraction is not supported in this version. Select a file to extract it.",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByTestId("extract-submit")).not.toBeEnabled();
  });

  it("selectingFatFileWithStatPathEnablesExtraction", async () => {
    const user = userEvent.setup();
    vi.mocked(stat).mockResolvedValue(
      buildFileEntryDto({ name: "notes.txt", isDir: false, path: "/notes.txt" }),
    );
    await renderOpened();

    await user.click(findRow("notes.txt"));

    expect(await screen.findByTestId("selected-entry-panel")).toBeInTheDocument();
    expect(screen.getByTestId("selected-entry-name")).toHaveTextContent("notes.txt");
    expect(screen.getByTestId("selected-entry-path")).toHaveTextContent("/notes.txt");

    await user.type(screen.getByTestId("extract-destination-input"), "/tmp/notes.txt");
    expect(screen.getByTestId("extract-submit")).toBeEnabled();
  });

  it("selecting a file with no known path keeps extraction disabled and shows a clear message", async () => {
    const user = userEvent.setup();
    vi.mocked(stat).mockResolvedValue(
      buildFileEntryDto({ name: "notes.txt", isDir: false, path: null }),
    );
    await renderOpened();

    await user.click(findRow("notes.txt"));

    expect(await screen.findByTestId("selected-entry-error")).toBeInTheDocument();

    await user.type(screen.getByTestId("extract-destination-input"), "/tmp/notes.txt");
    expect(screen.getByTestId("extract-submit")).toBeDisabled();
  });
});

describe("VolumeBrowserPage focus management", () => {
  it("focuses the directory table after entries load", async () => {
    await renderOpened();

    expect(document.activeElement).toBe(screen.getByTestId("directory-table"));
  });

  it("refocuses the directory table after navigating into a directory", async () => {
    const user = userEvent.setup();
    await renderOpened();

    const subDirEntry = buildFileEntryDto({
      name: "Rocket Science.txt",
      isDir: false,
      path: null,
    });
    vi.mocked(listDir).mockResolvedValueOnce([subDirEntry]);
    await user.dblClick(findRow("Folder With Spaces"));

    await screen.findByText("Rocket Science.txt");
    expect(document.activeElement).toBe(screen.getByTestId("directory-table"));
  });
});
