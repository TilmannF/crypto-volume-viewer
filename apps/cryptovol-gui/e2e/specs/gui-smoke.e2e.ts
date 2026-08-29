/**
 * E2E1 (App Starts): the real built cryptovol-gui launches and its Open
 * Volume page is immediately usable. See docs/gui-testing.md for how this
 * harness builds/launches the app.
 */

import { browser, expect } from "@wdio/globals";
import { waitForTestId } from "../support/app.js";
import { FIXTURES } from "../support/paths.js";
import { SEL } from "../support/selectors.js";

describe("App starts", () => {
  it("shows the Open Volume page with a path input, password input, and open button", async () => {
    await expect(await waitForTestId(SEL.openVolumePage)).toBeDisplayed();
    await expect(await waitForTestId(SEL.openContainerPathInput)).toBeDisplayed();
    await expect(await waitForTestId(SEL.openContainerPasswordInput)).toBeDisplayed();
    await expect(await waitForTestId(SEL.openContainerSubmit)).toBeDisplayed();
  });
});

describe("Wrong password", () => {
  it("shows a sanitized auth error and never renders the entered password", async () => {
    const wrongPassword = "definitely-wrong-password";

    await (await waitForTestId(SEL.openContainerPathInput)).setValue(FIXTURES.fat);
    await (await waitForTestId(SEL.openContainerPasswordInput)).setValue(wrongPassword);

    // An explicit KDF hint (matching the fixture) avoids the multi-minute
    // exhaustive KDF autoprobe that "Auto" would trigger on a wrong
    // password (every KDF/header-candidate combination has to be tried
    // before giving up). A native click focuses MUI's Select trigger but
    // does not open its popup here; Enter does (matching MUI's own
    // keyboard-accessible behavior).
    await (await waitForTestId(SEL.openContainerKdfSelect)).click();
    await browser.keys(["Enter"]);
    await (await browser.$("//*[@role='option'][text()='SHA-512']")).click();

    await (await waitForTestId(SEL.openContainerSubmit)).click();

    await expect(await waitForTestId(SEL.openContainerError, 30_000)).toBeDisplayed();

    const bodyText = await browser.$("body").getText();
    expect(bodyText).not.toContain(wrongPassword);
  });
});

/**
 * E2E7 (extraction cancellation) is intentionally not implemented here.
 *
 * Decision: a deterministic click-extract-then-click-cancel E2E race is
 * not practical without adding an artificial delay to production code,
 * which this milestone forbids. Measured directly against the real app:
 * extracting the largest file across every committed static fixture
 * (testdata/static/fs-fat-lfn-original/Sydney Sweeney at the 2025 Toronto
 * International Film Festival.jpg, 89,489 bytes -- nothing larger exists
 * in any testdata/static/*.hc container) completed in ~0.26s end to end
 * (click to destination file on disk). Separately, this WebDriver harness
 * pays a real ~5s tax on essentially every command (see the "Wrong
 * password" spec's comment above and docs/gui-testing.md), so the
 * earliest a `cancel` command could physically be issued after `extract`
 * is several seconds after the click -- by which point an 87KB
 * extraction has been done for seconds already. There is no fixture
 * large enough, and no way to slow the harness down, to make this race
 * land mid-extraction reliably.
 *
 * Cancellation is instead covered at the layers where it can actually be
 * made deterministic:
 * - Frontend wiring (AC-009): ExtractionPanel.integration.test.tsx
 *   asserts clicking extract-cancel calls cancelExtract(jobId) with the
 *   running job's id.
 * - cryptovol-app: crates/cryptovol-app/tests/cancellation_token.rs
 *   (token semantics) and progress_and_cancellation.rs /
 *   extract_file_fixtures.rs (cancellation_before_copy_starts_returns_
 *   cancelled_with_no_destination, cancellation_mid_copy_stops_early_and_
 *   leaves_no_destination, cancellation_mid_extraction_leaves_no_
 *   destination_file -- exactly AC-018's "no unexpected destination
 *   file" concern, exercised deterministically via an injected
 *   cancellation point rather than a timing race).
 * - Tauri command layer: apps/cryptovol-gui/src-tauri/tests/job_registry.rs
 *   (cancelling a known/unknown job), extract_job.rs
 *   (run_extraction_job_cancelled_mid_copy_emits_cancelled_not_finished,
 *   cancel_extract_impl_on_unknown_job_returns_job_not_found), and
 *   session_registry.rs (close_session_cancels_active_jobs_then_removes_
 *   the_session).
 */
