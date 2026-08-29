import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { AppShell } from "./AppShell";

describe("AppShell", () => {
  it("shows a beta status hint without disturbing the title text", () => {
    render(
      <AppShell>
        <div>content</div>
      </AppShell>,
    );

    expect(screen.getByText(/beta/i)).toBeInTheDocument();
    // getByText itself throws if there is more than one match, so this
    // also proves the title still resolves to exactly one element.
    expect(screen.getByText("Crypto Volume Viewer")).toBeInTheDocument();
  });
});
