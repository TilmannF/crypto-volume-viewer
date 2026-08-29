import Box from "@mui/material/Box";
import Typography from "@mui/material/Typography";
import { describeVolumeInfo } from "@/entities/volume";
import { toFileEntryView } from "@/entities/file-entry";
import type { FileEntryDto, VolumeInfoDto } from "@/shared/api/dto";

export type StatusBarProps = {
  volumeInfo: VolumeInfoDto;
  selectedEntry: FileEntryDto | null;
};

const DIRECTORY_EXTRACTION_UNSUPPORTED_MESSAGE =
  "Directory extraction is not supported in this version. Select a file to extract it.";
const NO_KNOWN_PATH_MESSAGE = "Selected entry has no known path; extraction is unavailable.";

/** One-line status bar: volume facts on the left, selection info on the right. */
export function StatusBar({ volumeInfo, selectedEntry }: StatusBarProps) {
  const volumeView = describeVolumeInfo(volumeInfo);
  const entryView = selectedEntry ? toFileEntryView(selectedEntry) : null;

  return (
    <Box
      data-testid="status-bar"
      sx={{
        display: "flex",
        alignItems: "center",
        gap: 2,
        borderTop: 1,
        borderColor: "divider",
        px: 1,
        py: 0.5,
        minWidth: 0,
      }}
    >
      <Box data-testid="volume-info-panel" sx={{ minWidth: 0, display: "flex", alignItems: "center" }}>
        <Typography variant="caption" noWrap component="span" sx={{ display: "inline" }}>
          <Typography
            variant="caption"
            component="span"
            data-testid="volume-info-filesystem"
            sx={{ display: "inline" }}
          >
            {volumeView.filesystemLabel}
          </Typography>{" "}
          · {volumeView.cipher} ·{" "}
          <Typography
            variant="caption"
            component="span"
            data-testid="volume-info-kdf"
            sx={{ display: "inline" }}
          >
            {volumeView.kdfLabel}
          </Typography>{" "}
          · PIM: {volumeView.pimLabel} · {volumeView.headerRoleLabel} ·{" "}
          <Typography
            variant="caption"
            component="span"
            data-testid="volume-info-readonly"
            sx={{ display: "inline" }}
          >
            Read-only: {volumeView.readOnly ? "yes" : "no"}
          </Typography>{" "}
          · {volumeView.containerSizeLabel} ·{" "}
          <Typography
            variant="caption"
            component="span"
            noWrap
            title={volumeView.containerPath}
            sx={{ display: "inline-block", overflow: "hidden", textOverflow: "ellipsis" }}
          >
            {volumeView.containerPath}
          </Typography>
        </Typography>
      </Box>

      {entryView && (
        <Box
          data-testid="selected-entry-panel"
          sx={{ ml: "auto", minWidth: 0, flexShrink: 0, display: "flex", gap: 1 }}
        >
          <Typography
            variant="caption"
            noWrap
            title={entryView.name}
            data-testid="selected-entry-name"
          >
            {entryView.name}
          </Typography>
          {entryView.isDir && (
            <Typography
              variant="caption"
              noWrap
              color="text.secondary"
              title={DIRECTORY_EXTRACTION_UNSUPPORTED_MESSAGE}
            >
              {DIRECTORY_EXTRACTION_UNSUPPORTED_MESSAGE}
            </Typography>
          )}
          {!entryView.isDir && entryView.path && (
            <>
              <Typography
                variant="caption"
                noWrap
                title={entryView.path}
                data-testid="selected-entry-path"
              >
                {entryView.path}
              </Typography>
              <Typography variant="caption" noWrap>
                {entryView.sizeLabel}
              </Typography>
            </>
          )}
          {!entryView.isDir && !entryView.path && (
            <Typography
              variant="caption"
              noWrap
              color="warning.main"
              data-testid="selected-entry-error"
            >
              {NO_KNOWN_PATH_MESSAGE}
            </Typography>
          )}
        </Box>
      )}
    </Box>
  );
}
