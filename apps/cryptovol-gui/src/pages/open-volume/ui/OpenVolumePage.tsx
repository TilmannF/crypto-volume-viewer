import { useEffect, useRef, useState, type HTMLAttributes } from "react";
import FolderOpenIcon from "@mui/icons-material/FolderOpen";
import Alert from "@mui/material/Alert";
import Button from "@mui/material/Button";
import FormControl from "@mui/material/FormControl";
import IconButton from "@mui/material/IconButton";
import InputLabel from "@mui/material/InputLabel";
import MenuItem from "@mui/material/MenuItem";
import Select, { type SelectChangeEvent } from "@mui/material/Select";
import Stack from "@mui/material/Stack";
import TextField from "@mui/material/TextField";
import Typography from "@mui/material/Typography";
import { browseContainerFile, useOpenContainer } from "@/features/open-container";
import { KDF_OPTIONS, validatePim } from "@/shared/config/kdf";
import type { KdfHint, VolumeInfoDto } from "@/shared/api/dto";

export type OpenVolumePageProps = {
  onOpened: (sessionId: string, volumeInfo: VolumeInfoDto) => void;
};

/** Sentinel Select value standing in for "no KDF hint" (Auto). */
const AUTO_VALUE = "auto";

/** Open Volume page: container path, password, optional PIM/KDF, and Open. */
export function OpenVolumePage({ onOpened }: OpenVolumePageProps) {
  const { state, open } = useOpenContainer();
  const [containerPath, setContainerPath] = useState("");
  const [password, setPassword] = useState("");
  const [pimInput, setPimInput] = useState("");
  const [kdfHint, setKdfHint] = useState<string>(AUTO_VALUE);
  const passwordInputRef = useRef<HTMLInputElement>(null);

  const pimValidation = validatePim(pimInput);
  const isOpening = state.status === "opening";
  const canSubmit =
    containerPath.trim() !== "" && password !== "" && pimValidation.ok && !isOpening;

  useEffect(() => {
    if (state.status === "opened") {
      setPassword("");
      onOpened(state.sessionId, state.volumeInfo);
    }
  }, [state, onOpened]);

  // Refocuses the password field after a failed open so the user knows
  // exactly where to correct their input, without requiring them to click
  // back into the field themselves.
  useEffect(() => {
    if (state.status === "failed") {
      passwordInputRef.current?.focus();
    }
  }, [state]);

  const handleBrowse = async () => {
    const selected = await browseContainerFile();
    if (selected) {
      setContainerPath(selected);
    }
  };

  const handleOpen = async () => {
    if (!pimValidation.ok) {
      return;
    }
    const hint = kdfHint === AUTO_VALUE ? undefined : (kdfHint as KdfHint);
    await open(containerPath, password, pimValidation.value, hint);
  };

  return (
    <Stack spacing={2} sx={{ maxWidth: 480 }} data-testid="open-volume-page">
      <Typography variant="h6">Open Volume</Typography>

      <Stack direction="row" spacing={1}>
        <TextField
          label="Container path"
          size="small"
          fullWidth
          value={containerPath}
          onChange={(event) => setContainerPath(event.target.value)}
          disabled={isOpening}
          slotProps={{ htmlInput: { "data-testid": "open-container-path-input" } }}
        />
        <IconButton
          aria-label="Browse for container file"
          onClick={() => void handleBrowse()}
          disabled={isOpening}
          data-testid="open-container-browse-button"
        >
          <FolderOpenIcon />
        </IconButton>
      </Stack>

      <TextField
        label="Password"
        type="password"
        size="small"
        fullWidth
        value={password}
        onChange={(event) => setPassword(event.target.value)}
        disabled={isOpening}
        inputRef={passwordInputRef}
        slotProps={{ htmlInput: { "data-testid": "open-container-password-input" } }}
      />

      <TextField
        label="PIM (optional)"
        size="small"
        fullWidth
        value={pimInput}
        onChange={(event) => setPimInput(event.target.value)}
        error={!pimValidation.ok}
        helperText={pimValidation.ok ? "Leave empty for the default" : pimValidation.message}
        disabled={isOpening}
        slotProps={{ htmlInput: { "data-testid": "open-container-pim-input" } }}
      />

      <FormControl size="small" fullWidth disabled={isOpening}>
        <InputLabel id="open-volume-kdf-label">KDF</InputLabel>
        <Select
          labelId="open-volume-kdf-label"
          label="KDF"
          value={kdfHint}
          onChange={(event: SelectChangeEvent) => setKdfHint(event.target.value)}
          // @types/react's HTMLAttributes has no `data-*` index signature,
          // unlike the more permissive slot props used above, so this needs
          // an explicit (still fully-typed, non-`any`) cast.
          SelectDisplayProps={
            { "data-testid": "open-container-kdf-select" } as HTMLAttributes<HTMLDivElement>
          }
        >
          {KDF_OPTIONS.map((option) => (
            <MenuItem key={option.label} value={option.value ?? AUTO_VALUE}>
              {option.label}
            </MenuItem>
          ))}
        </Select>
      </FormControl>

      <Button
        variant="contained"
        onClick={() => void handleOpen()}
        disabled={!canSubmit}
        data-testid="open-container-submit"
      >
        {isOpening ? "Opening…" : "Open"}
      </Button>

      {state.status === "failed" && (
        <Alert severity="error" data-testid="open-container-error">
          {state.error.code === "auth_failed"
            ? "Incorrect password, or unsupported volume parameters."
            : state.error.message}
        </Alert>
      )}
    </Stack>
  );
}
