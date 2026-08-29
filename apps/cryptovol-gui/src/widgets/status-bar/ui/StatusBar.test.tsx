import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { StatusBar } from "./StatusBar";
import { buildFileEntryDto, buildVolumeInfoDto } from "@/shared/testing";

describe("StatusBar", () => {
  it("renders volume facts and no selection panel when nothing is selected", () => {
    const volumeInfo = buildVolumeInfoDto({ filesystem: "fat", kdf: "sha512", readOnly: true });

    render(<StatusBar volumeInfo={volumeInfo} selectedEntry={null} />);

    expect(screen.getByTestId("volume-info-panel")).toBeInTheDocument();
    expect(screen.getByTestId("volume-info-filesystem")).toHaveTextContent(/fat/i);
    expect(screen.getByTestId("volume-info-kdf")).toHaveTextContent(/sha-?512/i);
    expect(screen.getByTestId("volume-info-readonly")).toHaveTextContent(/yes/i);
    expect(screen.queryByTestId("selected-entry-panel")).not.toBeInTheDocument();
  });

  it("renders name, path, and size for a selected file with a known path", () => {
    const volumeInfo = buildVolumeInfoDto();
    const selectedEntry = buildFileEntryDto({
      name: "notes.txt",
      isDir: false,
      path: "/notes.txt",
      size: 49,
    });

    render(<StatusBar volumeInfo={volumeInfo} selectedEntry={selectedEntry} />);

    const panel = screen.getByTestId("selected-entry-panel");
    expect(panel).toBeInTheDocument();
    expect(screen.getByTestId("selected-entry-name")).toHaveTextContent("notes.txt");
    expect(screen.getByTestId("selected-entry-path")).toHaveTextContent("/notes.txt");
    expect(panel.textContent).toContain("49");
  });

  it("shows the directory-extraction-unsupported message for a selected directory", () => {
    const volumeInfo = buildVolumeInfoDto();
    const selectedEntry = buildFileEntryDto({
      name: "Folder With Spaces",
      isDir: true,
      path: null,
    });

    render(<StatusBar volumeInfo={volumeInfo} selectedEntry={selectedEntry} />);

    expect(
      screen.getByText(
        "Directory extraction is not supported in this version. Select a file to extract it.",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByTestId("selected-entry-path")).not.toBeInTheDocument();
  });

  it("shows the no-known-path error for a selected file without a path", () => {
    const volumeInfo = buildVolumeInfoDto();
    const selectedEntry = buildFileEntryDto({
      name: "notes.txt",
      isDir: false,
      path: null,
    });

    render(<StatusBar volumeInfo={volumeInfo} selectedEntry={selectedEntry} />);

    expect(screen.getByTestId("selected-entry-error")).toHaveTextContent(
      "Selected entry has no known path; extraction is unavailable.",
    );
  });
});
