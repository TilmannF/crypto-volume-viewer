/**
 * Manual Vitest mock for `../commands.ts` (Vitest's `__mocks__` convention:
 * `vi.mock("@/shared/api/commands")` in a test file picks this up
 * automatically). Every export is a bare `vi.fn()` with no real Tauri
 * `invoke`/`listen` behind it; tests configure return values per case via
 * `vi.mocked(...)`.
 */

import { vi } from "vitest";
import type * as Commands from "../commands";

export const inspectContainer: typeof Commands.inspectContainer = vi.fn();
export const openContainer: typeof Commands.openContainer = vi.fn();
export const listDir: typeof Commands.listDir = vi.fn();
export const stat: typeof Commands.stat = vi.fn();
export const closeSession: typeof Commands.closeSession = vi.fn();
export const extractFile: typeof Commands.extractFile = vi.fn();
export const cancelExtract: typeof Commands.cancelExtract = vi.fn();
export const selectContainerFile: typeof Commands.selectContainerFile = vi.fn();
export const selectExtractDestination: typeof Commands.selectExtractDestination = vi.fn();
export const subscribeToExtractionEvents: typeof Commands.subscribeToExtractionEvents = vi.fn();
