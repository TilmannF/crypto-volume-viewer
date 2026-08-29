import { fileURLToPath, URL } from "node:url";
import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

// Separate from vite.config.ts on purpose: that file's server/build settings
// are tailored for `tauri dev`/`tauri build` and must not leak into the test
// runner (see docs/gui-testing.md).
export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  test: {
    environment: "jsdom",
    setupFiles: ["./src/shared/testing/vitest-setup.ts"],
  },
});
