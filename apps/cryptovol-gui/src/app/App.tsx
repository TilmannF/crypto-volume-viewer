import { useState } from "react";
import CssBaseline from "@mui/material/CssBaseline";
import { ThemeProvider } from "@mui/material/styles";
import { theme } from "@/app/theme/theme";
import { OpenVolumePage } from "@/pages/open-volume";
import { VolumeBrowserPage } from "@/pages/volume-browser";
import { AppShell } from "@/widgets/app-shell";
import type { VolumeInfoDto } from "@/shared/api/dto";

type AppPage =
  | { page: "open-volume" }
  | { page: "volume-browser"; sessionId: string; volumeInfo: VolumeInfoDto };

const OPEN_VOLUME_PAGE: AppPage = { page: "open-volume" };

/**
 * Application root. Holds top-level navigation state and composes pages;
 * contains no business logic itself (see 30-frontend-policy.md).
 */
function App() {
  const [current, setCurrent] = useState<AppPage>(OPEN_VOLUME_PAGE);

  return (
    <ThemeProvider theme={theme}>
      <CssBaseline />
      <AppShell>
        {current.page === "open-volume" ? (
          <OpenVolumePage
            onOpened={(sessionId, volumeInfo) =>
              setCurrent({ page: "volume-browser", sessionId, volumeInfo })
            }
          />
        ) : (
          <VolumeBrowserPage
            sessionId={current.sessionId}
            volumeInfo={current.volumeInfo}
            onClosed={() => setCurrent(OPEN_VOLUME_PAGE)}
          />
        )}
      </AppShell>
    </ThemeProvider>
  );
}

export default App;
