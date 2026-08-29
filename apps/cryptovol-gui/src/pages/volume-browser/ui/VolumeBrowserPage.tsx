import { useEffect, useState } from "react";
import Alert from "@mui/material/Alert";
import Box from "@mui/material/Box";
import { useDirectoryBrowser } from "@/features/browse-directory";
import { cancel } from "@/features/cancel-extraction";
import { close } from "@/features/close-session";
import { browseDestination, useExtractFile } from "@/features/extract-file";
import { DirectoryBrowser } from "@/widgets/directory-browser";
import { ExtractionPanel } from "@/widgets/extraction-panel";
import { StatusBar } from "@/widgets/status-bar";
import type { VolumeInfoDto } from "@/shared/api/dto";

const ROOT_PATH = "/";

export type VolumeBrowserPageProps = {
  sessionId: string;
  volumeInfo: VolumeInfoDto;
  /** Called after the session has been closed on the backend. */
  onClosed: () => void;
};

/** Volume Browser page: volume info, directory browsing, and extraction. */
export function VolumeBrowserPage({ sessionId, volumeInfo, onClosed }: VolumeBrowserPageProps) {
  const browser = useDirectoryBrowser();
  const extraction = useExtractFile();
  const [destinationPath, setDestinationPath] = useState("");

  useEffect(() => {
    void browser.loadRoot(sessionId);
    // Runs once per opened session; browser's action callbacks are stable
    // (useCallback), so this intentionally does not depend on `browser`.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionId]);

  const handleBack = async () => {
    await close(sessionId);
    onClosed();
  };

  const handleBrowseDestination = async () => {
    const selected = await browseDestination(browser.state.selectedEntry?.name);
    if (selected) {
      setDestinationPath(selected);
    }
  };

  const selectedEntry = browser.state.selectedEntry;
  const canStartExtraction =
    Boolean(selectedEntry) &&
    !selectedEntry?.isDir &&
    Boolean(selectedEntry?.path) &&
    destinationPath.trim() !== "";

  return (
    <Box
      data-testid="volume-browser-page"
      sx={{ height: "100%", minHeight: 0, display: "flex", flexDirection: "column", gap: 1 }}
    >
      <Box sx={{ flex: 1, minHeight: 0 }}>
        <DirectoryBrowser
          currentPath={browser.state.currentPath}
          entries={browser.state.entries}
          selectedEntry={browser.state.selectedEntry}
          loading={browser.state.loading}
          canGoUp={browser.state.currentPath !== ROOT_PATH}
          onSelect={(entry) => void browser.select(sessionId, entry)}
          onNavigateInto={(entry) => void browser.navigateInto(sessionId, entry.name)}
          onGoUp={() => void browser.goUp(sessionId)}
          onRefresh={() => void browser.refresh(sessionId)}
          onClose={() => void handleBack()}
        />
      </Box>

      {browser.state.error && (
        <Alert severity="error" data-testid="global-error-alert">
          {browser.state.error.message}
        </Alert>
      )}

      <ExtractionPanel
        destinationPath={destinationPath}
        onDestinationPathChange={setDestinationPath}
        onBrowseDestination={() => void handleBrowseDestination()}
        canStart={canStartExtraction}
        onStart={() => {
          if (selectedEntry) {
            void extraction.start(sessionId, selectedEntry, destinationPath);
          }
        }}
        onCancel={() => {
          if (extraction.state.status === "running") {
            void cancel(extraction.state.jobId);
          }
        }}
        state={extraction.state}
      />

      <StatusBar volumeInfo={volumeInfo} selectedEntry={selectedEntry} />
    </Box>
  );
}
