import Alert from "@mui/material/Alert";
import Box from "@mui/material/Box";
import Button from "@mui/material/Button";
import LinearProgress from "@mui/material/LinearProgress";
import Stack from "@mui/material/Stack";
import TextField from "@mui/material/TextField";
import Typography from "@mui/material/Typography";
import { formatByteSize } from "@/entities/file-entry";
import type { ExtractionState } from "@/entities/extraction-job";

export type ExtractionPanelProps = {
  destinationPath: string;
  onDestinationPathChange: (value: string) => void;
  onBrowseDestination: () => void;
  canStart: boolean;
  onStart: () => void;
  onCancel: () => void;
  state: ExtractionState;
};

/**
 * Destination picker, start/cancel controls, and progress/result display
 * for a single extraction job. Receives all state/actions from the
 * composing page (backed by features/extract-file and
 * features/cancel-extraction); never imports invoke or shared/api/commands
 * itself.
 */
export function ExtractionPanel({
  destinationPath,
  onDestinationPathChange,
  onBrowseDestination,
  canStart,
  onStart,
  onCancel,
  state,
}: ExtractionPanelProps) {
  const isRunning = state.status === "running";

  return (
    <Stack spacing={1} data-testid="extraction-panel">
      <Stack direction="row" spacing={1} alignItems="center">
        <TextField
          label="Destination path"
          size="small"
          fullWidth
          value={destinationPath}
          onChange={(event) => onDestinationPathChange(event.target.value)}
          disabled={isRunning}
          slotProps={{ htmlInput: { "data-testid": "extract-destination-input" } }}
        />
        <Button
          onClick={onBrowseDestination}
          disabled={isRunning}
          data-testid="extract-destination-browse-button"
        >
          Browse...
        </Button>
        <Button
          variant="contained"
          onClick={onStart}
          disabled={!canStart || isRunning}
          data-testid="extract-submit"
        >
          Extract
        </Button>
        <Button onClick={onCancel} disabled={!isRunning} data-testid="extract-cancel">
          Cancel
        </Button>
      </Stack>

      {state.status === "running" && (
        <Box data-testid="extract-progress">
          <LinearProgress
            variant={state.totalBytes ? "determinate" : "indeterminate"}
            value={
              state.totalBytes
                ? Math.min(100, (state.bytesWritten / state.totalBytes) * 100)
                : undefined
            }
            sx={{ height: 3 }}
          />
          <Typography variant="caption" data-testid="extract-progress-bytes">
            {formatByteSize(state.bytesWritten)}
            {state.totalBytes ? ` / ${formatByteSize(state.totalBytes)}` : ""}
          </Typography>
        </Box>
      )}

      {state.status === "finished" && (
        <Alert severity="success" data-testid="extract-success">
          Extraction finished ({formatByteSize(state.bytesWritten)}).
        </Alert>
      )}
      {state.status === "cancelled" && <Alert severity="warning">Extraction cancelled.</Alert>}
      {state.status === "failed" && (
        <Alert severity="error" data-testid="extract-error">
          {state.error.message}
        </Alert>
      )}
    </Stack>
  );
}
