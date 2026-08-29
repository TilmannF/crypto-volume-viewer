import AppBar from "@mui/material/AppBar";
import Box from "@mui/material/Box";
import Toolbar from "@mui/material/Toolbar";
import Typography from "@mui/material/Typography";
import type { ReactNode } from "react";

export type AppShellProps = {
  children: ReactNode;
};

/** App-wide layout: a title bar plus a scrollable content area. No backend calls. */
export function AppShell({ children }: AppShellProps) {
  return (
    <Box sx={{ display: "flex", flexDirection: "column", height: "100vh" }}>
      <AppBar position="static">
        <Toolbar variant="dense" disableGutters sx={{ px: 2 }}>
          <Typography variant="subtitle2" component="h1">
            Crypto Volume Viewer
          </Typography>
          <Typography variant="caption" color="text.secondary" sx={{ ml: 1 }}>
            Beta · Read-only
          </Typography>
        </Toolbar>
      </AppBar>
      <Box sx={{ flex: 1, overflow: "auto", minHeight: 0, p: 1 }}>{children}</Box>
    </Box>
  );
}
