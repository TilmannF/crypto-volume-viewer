import React from "react";
import ReactDOM from "react-dom/client";
import App from "@/app/App";

/**
 * Loads the E2E-only WebdriverIO Tauri companion plugin before rendering,
 * when the build was produced with `VITE_E2E_WEBDRIVER=1` (see
 * `apps/cryptovol-gui/e2e/wdio.conf.ts`'s `buildAppForE2e()`). Dynamic so
 * it's excluded from the production bundle's static import graph; it must
 * never ship in a normal/production build. Auto-initializes on import,
 * setting up `window.__wdio_original_core__` that
 * `@wdio/tauri-service`'s window-focus/state polling depends on to avoid
 * its slow fallback path.
 */
async function loadE2eSupportIfNeeded(): Promise<void> {
  if (import.meta.env.VITE_E2E_WEBDRIVER) {
    await import("@wdio/tauri-plugin");
  }
}

async function main(): Promise<void> {
  await loadE2eSupportIfNeeded();

  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
}

void main();
