import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { OpenVolumePage } from "./OpenVolumePage";
import { openContainer } from "@/shared/api/commands";
import { buildGuiError } from "@/shared/testing";

vi.mock("@/shared/api/commands");

const KDF_LABELS = ["Auto", "SHA-512", "SHA-256", "Whirlpool", "BLAKE2s-256", "Streebog"];

describe("OpenVolumePage", () => {
  it("renders all required fields, with a masked password input", () => {
    render(<OpenVolumePage onOpened={vi.fn()} />);

    expect(screen.getByTestId("open-volume-page")).toBeInTheDocument();
    expect(screen.getByTestId("open-container-path-input")).toBeInTheDocument();
    expect(screen.getByTestId("open-container-password-input")).toBeInTheDocument();
    expect(screen.getByTestId("open-container-pim-input")).toBeInTheDocument();
    expect(screen.getByTestId("open-container-kdf-select")).toBeInTheDocument();
    expect(screen.getByTestId("open-container-submit")).toBeInTheDocument();
    expect(screen.getByTestId("open-container-password-input")).toHaveAttribute("type", "password");
  });

  it("accepts an empty PIM and rejects a non-numeric PIM", async () => {
    const user = userEvent.setup();
    render(<OpenVolumePage onOpened={vi.fn()} />);

    const pimInput = screen.getByTestId("open-container-pim-input");
    await user.type(pimInput, "abc");
    expect(screen.getByText("PIM must be a whole, non-negative number.")).toBeInTheDocument();
    expect(screen.getByTestId("open-container-submit")).toBeDisabled();

    await user.clear(pimInput);
    expect(
      screen.queryByText("PIM must be a whole, non-negative number."),
    ).not.toBeInTheDocument();
  });

  it("lists Auto, SHA-512, SHA-256, Whirlpool, BLAKE2s-256, and Streebog as KDF options", async () => {
    const user = userEvent.setup();
    render(<OpenVolumePage onOpened={vi.fn()} />);

    await user.click(screen.getByTestId("open-container-kdf-select"));
    const options = await screen.findAllByRole("option");
    expect(options.map((option) => option.textContent)).toEqual(KDF_LABELS);
  });

  it("wrongPasswordShowsSanitizedErrorAndNeverRendersPassword", async () => {
    const user = userEvent.setup();
    vi.mocked(openContainer).mockRejectedValue(buildGuiError("auth_failed", "auth_failed"));

    const { container } = render(<OpenVolumePage onOpened={vi.fn()} />);

    await user.type(screen.getByTestId("open-container-path-input"), "/tmp/example.hc");
    await user.type(screen.getByTestId("open-container-password-input"), "definitely-wrong-pw");
    await user.click(screen.getByTestId("open-container-submit"));

    expect(await screen.findByTestId("open-container-error")).toHaveTextContent(
      "Incorrect password, or unsupported volume parameters.",
    );
    // The password input keeps its masked (type="password") value after a
    // failed attempt so the user can retry without retyping; only a
    // *successful* open clears it (AC-005). What AC-004 actually requires is
    // that the password is never rendered as visible text anywhere.
    expect(container.textContent).not.toContain("definitely-wrong-pw");
  });

  it("refocuses the password field after a failed open", async () => {
    const user = userEvent.setup();
    vi.mocked(openContainer).mockRejectedValue(buildGuiError("auth_failed", "auth_failed"));

    render(<OpenVolumePage onOpened={vi.fn()} />);

    await user.type(screen.getByTestId("open-container-path-input"), "/tmp/example.hc");
    await user.type(screen.getByTestId("open-container-password-input"), "definitely-wrong-pw");
    await user.click(screen.getByTestId("open-container-submit"));

    await screen.findByTestId("open-container-error");
    expect(document.activeElement).toBe(screen.getByTestId("open-container-password-input"));
  });
});
