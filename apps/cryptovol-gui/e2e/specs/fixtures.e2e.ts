/**
 * Fixture-based GUI flows against the real built app: open, browse,
 * select, and extract from the committed static TC/VC-compatible
 * containers (see docs/test-containers.md).
 */

import { browser, expect } from "@wdio/globals";
import path from "node:path";
import { waitForTestId } from "../support/app.js";
import { filesAreByteIdentical } from "../support/filesystem.js";
import {
  cleanupExtractionTempDir,
  createExtractionTempDir,
  FAT_GROUND_TRUTH_DIR,
  FIXTURES,
  TEST_PASSWORD,
} from "../support/paths.js";
import { SEL } from "../support/selectors.js";

/** Opens `fixturePath` with `password` and an explicit SHA-512 KDF hint. */
async function openContainer(fixturePath: string, password: string): Promise<void> {
  await (await waitForTestId(SEL.openContainerPathInput)).setValue(fixturePath);
  await (await waitForTestId(SEL.openContainerPasswordInput)).setValue(password);
  // A native click focuses MUI's Select trigger but does not open its
  // popup here; Enter does (matching MUI's own keyboard-accessible
  // behavior). See the "Wrong password" spec for the same pattern.
  await (await waitForTestId(SEL.openContainerKdfSelect)).click();
  await browser.keys(["Enter"]);
  await (await browser.$("//*[@role='option'][text()='SHA-512']")).click();
  await (await waitForTestId(SEL.openContainerSubmit)).click();
}

/**
 * Dispatches a real "dblclick" MouseEvent directly (React's delegated
 * listener picks up any bubbling native event, including a
 * programmatically dispatched one). This WebView's synthetic clicks
 * always report `event.detail: 0` and never produce a native "dblclick"
 * no matter how many single clicks are sent in succession -- confirmed by
 * instrumenting click/dblclick listeners directly -- so
 * `element.doubleClick()` cannot trigger DirectoryBrowser's
 * onDoubleClick-based navigate-into.
 */
async function dispatchDoubleClick(element: WebdriverIO.Element): Promise<void> {
  await browser.execute((el: HTMLElement) => {
    el.dispatchEvent(new MouseEvent("dblclick", { bubbles: true, cancelable: true, detail: 2 }));
  }, element);
}

/** Finds a directory-entry-row by its data-entry-name/data-entry-type attributes. */
async function waitForRow(
  name: string,
  type: "directory" | "file",
  timeoutMs = 30_000,
): Promise<WebdriverIO.Element> {
  const element = browser.$(
    `[data-testid="directory-entry-row"][data-entry-name="${name}"][data-entry-type="${type}"]`,
  );
  await element.waitForDisplayed({ timeout: timeoutMs });
  return element;
}

/**
 * Returns the app to the Open Volume page if it's currently past it.
 * @wdio/tauri-service's embedded provider reuses one app process across
 * every spec file in a run (it does not restart the app per spec), so
 * navigation state otherwise leaks into whatever spec/test runs next.
 */
async function returnToOpenVolumeIfNeeded(): Promise<void> {
  const backButton = browser.$(`[data-testid="${SEL.volumeBrowserBackButton}"]`);
  if (await backButton.isExisting()) {
    await backButton.click();
    await waitForTestId(SEL.openVolumePage, 30_000);
  }
}

/**
 * Navigates into "Folder With Spaces", selects "Rocket Science <rocket>
 * For Beginners.txt", extracts it to a fresh temp destination, and
 * asserts the bytes match the shared ground-truth original. Shared by
 * the FAT/exFAT/NTFS extraction regression specs -- all three static
 * fixtures document the same file tree and the same ground-truth
 * directory (see docs/test-containers.md).
 */
async function extractRocketFileAndVerify(): Promise<void> {
  await dispatchDoubleClick(await waitForRow("Folder With Spaces", "directory"));
  const currentPath = await (await waitForTestId(SEL.directoryCurrentPath)).getText();
  expect(currentPath).toContain("Folder With Spaces");

  const fileName = "Rocket Science \u{1F680} For Beginners.txt";
  await (await waitForRow(fileName, "file")).click();

  const selectedPath = await (await waitForTestId(SEL.selectedEntryPath)).getText();
  expect(selectedPath.length).toBeGreaterThan(0);

  const tempDir = await createExtractionTempDir();
  try {
    const destinationPath = path.join(tempDir, fileName);
    await (await waitForTestId(SEL.extractDestinationInput)).setValue(destinationPath);

    await expect(await waitForTestId(SEL.extractSubmit)).toBeEnabled();
    await (await waitForTestId(SEL.extractSubmit)).click();
    await expect(await waitForTestId(SEL.extractSuccess, 60_000)).toBeDisplayed();

    const groundTruthPath = path.join(FAT_GROUND_TRUTH_DIR, "Folder With Spaces", fileName);
    const identical = await filesAreByteIdentical(destinationPath, groundTruthPath);
    expect(identical).toBe(true);
  } finally {
    await cleanupExtractionTempDir(tempDir);
  }
}

describe("FAT open, browse, select, extract", () => {
  afterEach(returnToOpenVolumeIfNeeded);

  // This is the E2E equivalent of the gui-mvp-spike FAT
  // stat_via_parent_listing bug (docs/gui-mvp.md): FAT's stat fallback
  // once left FileEntry.path empty, silently blocking extraction for
  // every FAT file. This test fails if selecting the FAT file ever
  // leaves selectedEntry.path empty or extraction disabled.
  it("fatFixtureSelectionExtractionRegression", async () => {
    await openContainer(FIXTURES.fat, TEST_PASSWORD);

    await expect(await waitForTestId(SEL.volumeBrowserPage, 60_000)).toBeDisplayed();
    await expect(await waitForTestId(SEL.volumeInfoPanel)).toBeDisplayed();

    const filesystemText = await (await waitForTestId(SEL.volumeInfoFilesystem)).getText();
    expect(filesystemText.toLowerCase()).toContain("fat");

    await extractRocketFileAndVerify();
  });

  it("keeps extraction unavailable and starts no extraction when a directory is selected", async () => {
    await openContainer(FIXTURES.fat, TEST_PASSWORD);
    await expect(await waitForTestId(SEL.volumeBrowserPage, 60_000)).toBeDisplayed();

    await (await waitForRow("Folder With Spaces", "directory")).click();

    // A destination path alone must not be enough to enable extraction --
    // only an isDir: false, path-bearing selection should.
    const tempDir = await createExtractionTempDir();
    try {
      await (await waitForTestId(SEL.extractDestinationInput)).setValue(
        path.join(tempDir, "Folder With Spaces"),
      );

      await expect(browser.$(`[data-testid="${SEL.extractSubmit}"]`)).not.toBeEnabled();

      // Give any accidental extraction start a bounded window to appear.
      await browser.pause(2_000);
      expect(await browser.$(`[data-testid="${SEL.extractProgress}"]`).isExisting()).toBe(false);
      expect(await browser.$(`[data-testid="${SEL.extractSuccess}"]`).isExisting()).toBe(false);
    } finally {
      await cleanupExtractionTempDir(tempDir);
    }
  });
});

describe("exFAT open, browse, select, extract", () => {
  afterEach(returnToOpenVolumeIfNeeded);

  it("opens the exFAT fixture and lists its root Unicode/LFN entries", async () => {
    await openContainer(FIXTURES.exfat, TEST_PASSWORD);

    await expect(await waitForTestId(SEL.volumeBrowserPage, 60_000)).toBeDisplayed();

    const filesystemText = await (await waitForTestId(SEL.volumeInfoFilesystem)).getText();
    expect(filesystemText.toLowerCase()).toContain("exfat");

    // Per docs/test-containers.md's documented exFAT fixture contents.
    await waitForRow("Emoji Rocket \u{1F680} Test.txt", "file");
    await waitForRow("Folder With Spaces", "directory");
  });

  // The E2E equivalent of the exFAT half of the stat()-never-returns-a-path
  // bug (docs/gui-mvp.md): VolumeSession::stat() left FileEntry.path empty
  // for exFAT (and NTFS), silently blocking extraction. This test fails if
  // selecting an exFAT file ever leaves selectedEntry.path empty or
  // extraction disabled.
  it("exfatFixtureSelectionExtractionRegression", async () => {
    await openContainer(FIXTURES.exfat, TEST_PASSWORD);

    await expect(await waitForTestId(SEL.volumeBrowserPage, 60_000)).toBeDisplayed();
    await expect(await waitForTestId(SEL.volumeInfoPanel)).toBeDisplayed();

    const filesystemText = await (await waitForTestId(SEL.volumeInfoFilesystem)).getText();
    expect(filesystemText.toLowerCase()).toContain("exfat");

    await extractRocketFileAndVerify();
  });
});

describe("NTFS open, browse, select, extract", () => {
  afterEach(returnToOpenVolumeIfNeeded);

  it("opens the NTFS fixture and lists its root Unicode/LFN entries", async () => {
    await openContainer(FIXTURES.ntfs, TEST_PASSWORD);

    await expect(await waitForTestId(SEL.volumeBrowserPage, 60_000)).toBeDisplayed();

    const filesystemText = await (await waitForTestId(SEL.volumeInfoFilesystem)).getText();
    expect(filesystemText.toLowerCase()).toContain("ntfs");

    // Per docs/test-containers.md's documented NTFS fixture contents.
    // waitForRow's combined name+type selector is itself the assertion
    // that "Folder With Spaces" has data-entry-type="directory".
    await waitForRow("Emoji Rocket \u{1F680} Test.txt", "file");
    await waitForRow("Folder With Spaces", "directory");
  });

  // The E2E equivalent of the NTFS half of the stat()-never-returns-a-path
  // bug (docs/gui-mvp.md) -- this is the exact bug the user originally
  // reported: browsing several subdirectories deep worked fine, but
  // selecting a file for extraction showed "Selected entry has no known
  // path; extraction is unavailable." This test fails if selecting an
  // NTFS file ever leaves selectedEntry.path empty or extraction disabled.
  it("ntfsFixtureSelectionExtractionRegression", async () => {
    await openContainer(FIXTURES.ntfs, TEST_PASSWORD);

    await expect(await waitForTestId(SEL.volumeBrowserPage, 60_000)).toBeDisplayed();
    await expect(await waitForTestId(SEL.volumeInfoPanel)).toBeDisplayed();

    const filesystemText = await (await waitForTestId(SEL.volumeInfoFilesystem)).getText();
    expect(filesystemText.toLowerCase()).toContain("ntfs");

    await extractRocketFileAndVerify();
  });
});
