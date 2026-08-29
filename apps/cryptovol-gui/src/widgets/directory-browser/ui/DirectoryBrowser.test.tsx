import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { DirectoryBrowser } from "./DirectoryBrowser";
import { buildFileEntryDto } from "@/shared/testing";

const DIR_A = buildFileEntryDto({ name: "Alpha Dir", isDir: true, path: null });
const DIR_B = buildFileEntryDto({ name: "Folder With Spaces", isDir: true, path: null });
const FILE_C = buildFileEntryDto({ name: "notes.txt", isDir: false, path: "/notes.txt" });
const ENTRIES = [DIR_A, DIR_B, FILE_C];

function renderTable(
  overrides: {
    selectedEntry?: (typeof ENTRIES)[number] | null;
    canGoUp?: boolean;
    entries?: (typeof ENTRIES)[number][];
    loading?: boolean;
  } = {},
) {
  // Some tests call renderTable() more than once per `it` block (to assert
  // behavior across several selectedEntry/canGoUp states); vitest-setup.ts's
  // afterEach(cleanup) only runs between `it` blocks, not within one.
  cleanup();
  const handlers = {
    onSelect: vi.fn(),
    onNavigateInto: vi.fn(),
    onGoUp: vi.fn(),
    onRefresh: vi.fn(),
  };
  render(
    <DirectoryBrowser
      currentPath="/"
      entries={overrides.entries ?? ENTRIES}
      selectedEntry={overrides.selectedEntry ?? null}
      loading={overrides.loading ?? false}
      canGoUp={overrides.canGoUp ?? true}
      onSelect={handlers.onSelect}
      onNavigateInto={handlers.onNavigateInto}
      onGoUp={handlers.onGoUp}
      onRefresh={handlers.onRefresh}
    />,
  );
  return handlers;
}

function getTable(): HTMLElement {
  return screen.getByTestId("directory-table");
}

describe("DirectoryBrowser keyboard navigation", () => {
  it("ArrowDown selects the first row when nothing is selected", () => {
    const { onSelect } = renderTable({ selectedEntry: null });
    fireEvent.keyDown(getTable(), { key: "ArrowDown" });
    expect(onSelect).toHaveBeenCalledTimes(1);
    expect(onSelect).toHaveBeenCalledWith(DIR_A);
  });

  it("ArrowDown/ArrowUp move the selection between rows without wrapping", () => {
    const first = renderTable({ selectedEntry: DIR_A });
    fireEvent.keyDown(getTable(), { key: "ArrowDown" });
    expect(first.onSelect).toHaveBeenCalledWith(DIR_B);

    const second = renderTable({ selectedEntry: DIR_B });
    fireEvent.keyDown(getTable(), { key: "ArrowUp" });
    expect(second.onSelect).toHaveBeenCalledWith(DIR_A);

    const last = renderTable({ selectedEntry: FILE_C });
    fireEvent.keyDown(getTable(), { key: "ArrowDown" });
    expect(last.onSelect).not.toHaveBeenCalled();
  });

  it("Enter on a focused directory navigates into it", () => {
    const { onNavigateInto } = renderTable({ selectedEntry: DIR_B });
    fireEvent.keyDown(getTable(), { key: "Enter" });
    expect(onNavigateInto).toHaveBeenCalledTimes(1);
    expect(onNavigateInto).toHaveBeenCalledWith(DIR_B);
  });

  it("Enter on a focused file confirms selection without navigating", () => {
    const { onNavigateInto, onSelect } = renderTable({ selectedEntry: FILE_C });
    fireEvent.keyDown(getTable(), { key: "Enter" });
    expect(onNavigateInto).not.toHaveBeenCalled();
    expect(onSelect).toHaveBeenCalledWith(FILE_C);
  });

  it("Backspace navigates up only when canGoUp is true", () => {
    const canGo = renderTable({ canGoUp: true });
    fireEvent.keyDown(getTable(), { key: "Backspace" });
    expect(canGo.onGoUp).toHaveBeenCalledTimes(1);

    const cannotGo = renderTable({ canGoUp: false });
    fireEvent.keyDown(getTable(), { key: "Backspace" });
    expect(cannotGo.onGoUp).not.toHaveBeenCalled();
  });

  it("Escape clears the selection", () => {
    const { onSelect } = renderTable({ selectedEntry: FILE_C });
    fireEvent.keyDown(getTable(), { key: "Escape" });
    expect(onSelect).toHaveBeenCalledWith(null);
  });
});

describe("DirectoryBrowser empty and loading states", () => {
  it("shows an empty-directory message when there are no entries and it is not loading", () => {
    renderTable({ entries: [], loading: false });
    expect(screen.getByTestId("directory-empty-row")).toHaveTextContent(
      "This directory is empty.",
    );
  });

  it("does not show the empty-directory message while still loading", () => {
    renderTable({ entries: [], loading: true });
    expect(screen.queryByTestId("directory-empty-row")).not.toBeInTheDocument();
  });
});
