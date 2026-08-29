/**
 * Small WebdriverIO helpers shared across E2E specs: a data-testid
 * selector builder and bounded-timeout waits for a testid to
 * appear/disappear.
 */

import { $ } from "@wdio/globals";
import type { ChainablePromiseElement } from "webdriverio";

const DEFAULT_TIMEOUT_MS = 15_000;

/** Builds a CSS attribute selector for a stable data-testid. */
export function byTestId(testId: string): string {
  return `[data-testid="${testId}"]`;
}

/** Waits for and returns the element with `data-testid="testId"`. */
export async function waitForTestId(
  testId: string,
  timeoutMs = DEFAULT_TIMEOUT_MS,
): Promise<ChainablePromiseElement> {
  const element = $(byTestId(testId));
  await element.waitForDisplayed({ timeout: timeoutMs });
  return element;
}

/** Waits until no element with `data-testid="testId"` is displayed. */
export async function waitForTestIdGone(
  testId: string,
  timeoutMs = DEFAULT_TIMEOUT_MS,
): Promise<void> {
  const element = $(byTestId(testId));
  await element.waitForDisplayed({ timeout: timeoutMs, reverse: true });
}
