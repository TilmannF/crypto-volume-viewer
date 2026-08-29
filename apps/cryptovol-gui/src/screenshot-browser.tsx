/**
 * Dev-only composition of the real volume-browser widgets with fixture-like
 * names, used to capture README screenshots. Not part of the production
 * Tauri bundle (vite build still uses index.html only).
 */
import { StrictMode, useState } from "react";
import { createRoot } from "react-dom/client";
import CssBaseline from "@mui/material/CssBaseline";
import Box from "@mui/material/Box";
import { ThemeProvider } from "@mui/material/styles";
import { theme } from "@/app/theme/theme";
import { AppShell } from "@/widgets/app-shell";
import { DirectoryBrowser } from "@/widgets/directory-browser";
import { ExtractionPanel } from "@/widgets/extraction-panel";
import { StatusBar } from "@/widgets/status-bar";
import { IDLE_EXTRACTION_STATE } from "@/entities/extraction-job";
import type { FileEntryDto, VolumeInfoDto } from "@/shared/api/dto";

const ts = {
  year: 2025,
  month: 9,
  day: 7,
  hour: 14,
  minute: 32,
  second: 0,
};

const attrs = {
  readOnly: false,
  hidden: false,
  system: false,
  directory: false,
  archive: true,
};

const entries: FileEntryDto[] = [
  {
    name: "Folder With Spaces",
    path: "/Folder With Spaces",
    isDir: true,
    size: 0,
    attributes: { ...attrs, directory: true, archive: false },
    created: ts,
    modified: ts,
    accessed: ts,
    filesystem: "fat",
  },
  {
    name: "Project Notes Final.txt",
    path: "/Project Notes Final.txt",
    isDir: false,
    size: 49,
    attributes: attrs,
    created: ts,
    modified: ts,
    accessed: ts,
    filesystem: "fat",
  },
  {
    name: "Emoji Rocket 🚀 Test.txt",
    path: "/Emoji Rocket 🚀 Test.txt",
    isDir: false,
    size: 128,
    attributes: attrs,
    created: ts,
    modified: ts,
    accessed: ts,
    filesystem: "fat",
  },
  {
    name: "Unicode Umlaut äöü ÄÖÜ ß.txt",
    path: "/Unicode Umlaut äöü ÄÖÜ ß.txt",
    isDir: false,
    size: 256,
    attributes: attrs,
    created: ts,
    modified: ts,
    accessed: ts,
    filesystem: "fat",
  },
  {
    name: "Sydney Sweeney at the 2025 Toronto International Film Festival.jpg",
    path: "/Sydney Sweeney at the 2025 Toronto International Film Festival.jpg",
    isDir: false,
    size: 89489,
    attributes: attrs,
    created: ts,
    modified: ts,
    accessed: ts,
    filesystem: "fat",
  },
];

const volumeInfo: VolumeInfoDto = {
  containerPath: "/Users/you/backup.hc",
  containerSizeBytes: 20971520,
  backend: "tcvc",
  cipher: "aes-xts",
  kdf: "sha512",
  pim: "default",
  headerRole: "primary",
  filesystem: "fat",
  readOnly: true,
};

function ScreenshotBrowser() {
  const [selected, setSelected] = useState<FileEntryDto | null>(entries[1]);
  return (
    <ThemeProvider theme={theme}>
      <CssBaseline />
      <AppShell>
        <Box
          sx={{ height: "100%", minHeight: 0, display: "flex", flexDirection: "column", gap: 1 }}
        >
          <Box sx={{ flex: 1, minHeight: 0 }}>
            <DirectoryBrowser
              currentPath="/"
              entries={entries}
              selectedEntry={selected}
              loading={false}
              canGoUp={false}
              onSelect={setSelected}
              onNavigateInto={() => undefined}
              onGoUp={() => undefined}
              onRefresh={() => undefined}
              onClose={() => undefined}
            />
          </Box>
          <ExtractionPanel
            destinationPath="/Users/you/Desktop/Project Notes Final.txt"
            onDestinationPathChange={() => undefined}
            onBrowseDestination={() => undefined}
            canStart={Boolean(selected && !selected.isDir)}
            onStart={() => undefined}
            onCancel={() => undefined}
            state={IDLE_EXTRACTION_STATE}
          />
          <StatusBar volumeInfo={volumeInfo} selectedEntry={selected} />
        </Box>
      </AppShell>
    </ThemeProvider>
  );
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ScreenshotBrowser />
  </StrictMode>,
);
