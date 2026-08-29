/**
 * Directory browsing (list/navigate/refresh/select) through
 * `shared/api/commands.listDir`/`.stat`.
 */

import { useCallback, useState } from "react";
import { listDir, stat } from "@/shared/api/commands";
import type { FileEntryDto } from "@/shared/api/dto";
import type { GuiError } from "@/shared/api/errors";

const ROOT_PATH = "/";

function parentPath(path: string): string {
  if (path === ROOT_PATH || path === "") {
    return ROOT_PATH;
  }
  const trimmed = path.endsWith("/") ? path.slice(0, -1) : path;
  const lastSlash = trimmed.lastIndexOf("/");
  return lastSlash <= 0 ? ROOT_PATH : trimmed.slice(0, lastSlash);
}

function joinPath(parent: string, name: string): string {
  return parent === ROOT_PATH ? `${ROOT_PATH}${name}` : `${parent}/${name}`;
}

/** State for the directory browser widget. */
export type DirectoryBrowserState = {
  currentPath: string;
  entries: FileEntryDto[];
  selectedEntry: FileEntryDto | null;
  loading: boolean;
  error: GuiError | null;
};

export type UseDirectoryBrowserResult = {
  state: DirectoryBrowserState;
  loadRoot: (sessionId: string) => Promise<void>;
  navigateInto: (sessionId: string, name: string) => Promise<void>;
  goUp: (sessionId: string) => Promise<void>;
  refresh: (sessionId: string) => Promise<void>;
  /** Selects `entry` and fetches its full metadata via `stat` (AC-007). */
  select: (sessionId: string, entry: FileEntryDto | null) => Promise<void>;
};

const INITIAL_STATE: DirectoryBrowserState = {
  currentPath: ROOT_PATH,
  entries: [],
  selectedEntry: null,
  loading: false,
  error: null,
};

export function useDirectoryBrowser(): UseDirectoryBrowserResult {
  const [state, setState] = useState<DirectoryBrowserState>(INITIAL_STATE);

  const loadPath = useCallback(async (sessionId: string, path: string) => {
    setState((prev) => ({ ...prev, loading: true, error: null }));
    try {
      const entries = await listDir(sessionId, path);
      setState({ currentPath: path, entries, selectedEntry: null, loading: false, error: null });
    } catch (error) {
      setState((prev) => ({ ...prev, loading: false, error: error as GuiError }));
    }
  }, []);

  const loadRoot = useCallback(
    (sessionId: string) => loadPath(sessionId, ROOT_PATH),
    [loadPath],
  );

  const navigateInto = useCallback(
    (sessionId: string, name: string) => loadPath(sessionId, joinPath(state.currentPath, name)),
    [loadPath, state.currentPath],
  );

  const goUp = useCallback(
    (sessionId: string) => loadPath(sessionId, parentPath(state.currentPath)),
    [loadPath, state.currentPath],
  );

  const refresh = useCallback(
    (sessionId: string) => loadPath(sessionId, state.currentPath),
    [loadPath, state.currentPath],
  );

  const select = useCallback(
    async (sessionId: string, entry: FileEntryDto | null) => {
      if (!entry) {
        setState((prev) => ({ ...prev, selectedEntry: null }));
        return;
      }
      try {
        const detailed = await stat(sessionId, joinPath(state.currentPath, entry.name));
        setState((prev) => ({ ...prev, selectedEntry: detailed }));
      } catch {
        // Fall back to the listing's own entry if stat fails; the user can
        // still see basic info, and extraction will surface any real error.
        setState((prev) => ({ ...prev, selectedEntry: entry }));
      }
    },
    [state.currentPath],
  );

  return { state, loadRoot, navigateInto, goUp, refresh, select };
}
