import { closeSession } from "@/shared/api/commands";

/** Closes the session for `sessionId` via the backend. */
export function close(sessionId: string): Promise<void> {
  return closeSession(sessionId);
}
