import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import App from "@/app/App";
import { listDir, openContainer } from "@/shared/api/commands";
import { buildOpenContainerResponseDto, buildVolumeInfoDto } from "@/shared/testing";

vi.mock("@/shared/api/commands");

describe("App", () => {
  it("renders the app title", () => {
    render(<App />);
    expect(screen.getByText("Crypto Volume Viewer")).toBeInTheDocument();
  });

  it("transitions to the Volume Browser with safe volume info after a successful open", async () => {
    const user = userEvent.setup();
    vi.mocked(openContainer).mockResolvedValue(
      buildOpenContainerResponseDto({
        sessionId: "s1",
        volumeInfo: buildVolumeInfoDto({ filesystem: "fat", kdf: "sha512", readOnly: true }),
      }),
    );
    vi.mocked(listDir).mockResolvedValue([]);

    const { container } = render(<App />);

    await user.type(screen.getByTestId("open-container-path-input"), "/tmp/example.hc");
    await user.type(screen.getByTestId("open-container-password-input"), "secret-pw-123");
    await user.click(screen.getByTestId("open-container-submit"));

    expect(await screen.findByTestId("volume-browser-page")).toBeInTheDocument();
    expect(screen.queryByTestId("open-volume-page")).not.toBeInTheDocument();

    expect(screen.getByTestId("volume-info-panel")).toBeInTheDocument();
    expect(screen.getByTestId("volume-info-filesystem")).toHaveTextContent(/fat/i);
    expect(screen.getByTestId("volume-info-kdf")).toHaveTextContent(/sha-?512/i);
    expect(screen.getByTestId("volume-info-readonly")).toHaveTextContent(/yes|true/i);

    expect(container.textContent).not.toContain("secret-pw-123");
  });
});
