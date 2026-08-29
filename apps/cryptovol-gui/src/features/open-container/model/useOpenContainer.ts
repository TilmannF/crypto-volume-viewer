/**
 * Opens a container through `shared/api/commands.openContainer`.
 *
 * The password is only ever a call parameter, never assigned to this
 * hook's own state, so there is nothing for the hook itself to retain or
 * clear once `open()` resolves (see 30-frontend-policy.md section 5).
 */

import { useCallback, useState } from "react";
import { openContainer } from "@/shared/api/commands";
import type { KdfHint, VolumeInfoDto } from "@/shared/api/dto";
import type { GuiError } from "@/shared/api/errors";

/** Discriminated state for the Open Volume form's submit flow. */
export type OpenVolumeState =
  | { status: "idle" }
  | { status: "opening" }
  | { status: "opened"; sessionId: string; volumeInfo: VolumeInfoDto }
  | { status: "failed"; error: GuiError };

export type UseOpenContainerResult = {
  state: OpenVolumeState;
  /** Opens `containerPath` with `password` and optional PIM/KDF hint. */
  open: (
    containerPath: string,
    password: string,
    pim?: number,
    kdfHint?: KdfHint,
  ) => Promise<void>;
  /** Resets back to the idle state (e.g. before retrying after a failure). */
  reset: () => void;
};

export function useOpenContainer(): UseOpenContainerResult {
  const [state, setState] = useState<OpenVolumeState>({ status: "idle" });

  const open = useCallback(
    async (containerPath: string, password: string, pim?: number, kdfHint?: KdfHint) => {
      setState({ status: "opening" });
      try {
        const response = await openContainer({
          containerPath,
          password,
          pim: pim ?? null,
          kdfHint: kdfHint ?? null,
        });
        setState({
          status: "opened",
          sessionId: response.sessionId,
          volumeInfo: response.volumeInfo,
        });
      } catch (error) {
        // `error.code === "auth_failed"` lets the UI show a distinct wrong-
        // password message rather than a generic failure.
        setState({ status: "failed", error: error as GuiError });
      }
    },
    [],
  );

  const reset = useCallback(() => setState({ status: "idle" }), []);

  return { state, open, reset };
}
