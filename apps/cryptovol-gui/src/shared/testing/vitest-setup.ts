import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

// vitest.config.ts does not set test.globals, so React Testing Library's
// own auto-cleanup (which only self-registers when it detects a global
// `afterEach`) never fires; without this, DOM from one test leaks into the
// next within the same file.
afterEach(() => {
  cleanup();
});
