import { useEffect, useRef, type KeyboardEvent } from "react";
import ArrowUpwardIcon from "@mui/icons-material/ArrowUpward";
import FolderIcon from "@mui/icons-material/Folder";
import InsertDriveFileIcon from "@mui/icons-material/InsertDriveFile";
import RefreshIcon from "@mui/icons-material/Refresh";
import Box from "@mui/material/Box";
import Button from "@mui/material/Button";
import IconButton from "@mui/material/IconButton";
import Table from "@mui/material/Table";
import TableBody from "@mui/material/TableBody";
import TableCell from "@mui/material/TableCell";
import TableContainer from "@mui/material/TableContainer";
import TableHead from "@mui/material/TableHead";
import TableRow from "@mui/material/TableRow";
import Toolbar from "@mui/material/Toolbar";
import Tooltip from "@mui/material/Tooltip";
import Typography from "@mui/material/Typography";
import { toFileEntryView } from "@/entities/file-entry";
import type { FileEntryDto } from "@/shared/api/dto";

export type DirectoryBrowserProps = {
  currentPath: string;
  entries: FileEntryDto[];
  selectedEntry: FileEntryDto | null;
  loading: boolean;
  canGoUp: boolean;
  onSelect: (entry: FileEntryDto | null) => void;
  onNavigateInto: (entry: FileEntryDto) => void;
  onGoUp: () => void;
  onRefresh: () => void;
  onClose?: () => void;
};

const HANDLED_KEYS = new Set(["ArrowDown", "ArrowUp", "Enter", "Backspace", "Escape"]);

function formatAttributes(attributes: FileEntryDto["attributes"]): string {
  const flags = [
    attributes.readOnly && "R",
    attributes.hidden && "H",
    attributes.system && "S",
    attributes.archive && "A",
  ].filter(Boolean);
  return flags.length > 0 ? flags.join("") : "—";
}

/**
 * Lists the current directory's entries with navigation and selection.
 * Receives all data/callbacks from the composing page; never calls
 * shared/api/commands itself.
 */
export function DirectoryBrowser({
  currentPath,
  entries,
  selectedEntry,
  loading,
  canGoUp,
  onSelect,
  onNavigateInto,
  onGoUp,
  onRefresh,
  onClose,
}: DirectoryBrowserProps) {
  const tableRef = useRef<HTMLTableElement>(null);

  // Refocuses the table whenever a new entries array arrives (initial load
  // and every directory navigation), so the user can immediately use arrow
  // keys without tabbing in first.
  useEffect(() => {
    tableRef.current?.focus();
  }, [entries]);

  function handleKeyDown(event: KeyboardEvent<HTMLTableElement>) {
    if (!HANDLED_KEYS.has(event.key)) {
      return;
    }
    event.preventDefault();

    const currentIndex = selectedEntry
      ? entries.findIndex((entry) => entry.name === selectedEntry.name)
      : -1;

    switch (event.key) {
      case "ArrowDown": {
        const nextIndex = currentIndex === -1 ? 0 : currentIndex + 1;
        if (nextIndex < entries.length) {
          onSelect(entries[nextIndex]);
        }
        break;
      }
      case "ArrowUp": {
        if (currentIndex > 0) {
          onSelect(entries[currentIndex - 1]);
        }
        break;
      }
      case "Enter": {
        if (selectedEntry) {
          if (selectedEntry.isDir) {
            onNavigateInto(selectedEntry);
          } else {
            onSelect(selectedEntry);
          }
        }
        break;
      }
      case "Backspace": {
        if (canGoUp) {
          onGoUp();
        }
        break;
      }
      case "Escape": {
        onSelect(null);
        break;
      }
      default:
        break;
    }
  }

  return (
    <Box
      data-testid="directory-browser"
      sx={{ height: "100%", display: "flex", flexDirection: "column", minWidth: 0 }}
    >
      <Toolbar disableGutters variant="dense" sx={{ gap: 1 }}>
        {onClose && (
          <Tooltip title="Close volume">
            <Button
              onClick={onClose}
              data-testid="volume-browser-back-button"
              aria-label="Close volume"
            >
              Close
            </Button>
          </Tooltip>
        )}
        <Tooltip title="Up one level">
          <span>
            <IconButton
              onClick={onGoUp}
              disabled={!canGoUp || loading}
              aria-label="Up one level"
              size="small"
              data-testid="directory-up-button"
            >
              <ArrowUpwardIcon fontSize="small" />
            </IconButton>
          </span>
        </Tooltip>
        <Tooltip title="Refresh">
          <span>
            <IconButton
              onClick={onRefresh}
              disabled={loading}
              aria-label="Refresh directory listing"
              size="small"
              data-testid="directory-refresh-button"
            >
              <RefreshIcon fontSize="small" />
            </IconButton>
          </span>
        </Tooltip>
        <Typography variant="body2" sx={{ ml: 1 }} data-testid="directory-current-path">
          {currentPath}
        </Typography>
      </Toolbar>

      <TableContainer sx={{ flex: 1, minHeight: 0, overflow: "auto" }}>
        <Table
          ref={tableRef}
          size="small"
          stickyHeader
          sx={{ tableLayout: "fixed" }}
          data-testid="directory-table"
          tabIndex={0}
          onKeyDown={handleKeyDown}
        >
          <TableHead>
            <TableRow>
              <TableCell>Name</TableCell>
              <TableCell align="right" sx={{ width: 90 }}>
                Size
              </TableCell>
              <TableCell sx={{ width: 150 }}>Modified</TableCell>
              <TableCell sx={{ width: 70 }}>Attributes</TableCell>
            </TableRow>
          </TableHead>
          <TableBody>
            {!loading && entries.length === 0 && (
              <TableRow data-testid="directory-empty-row">
                <TableCell colSpan={4} align="center" sx={{ color: "text.secondary" }}>
                  This directory is empty.
                </TableCell>
              </TableRow>
            )}
            {entries.map((entry) => {
              const view = toFileEntryView(entry);
              const isSelected = selectedEntry?.name === entry.name;
              return (
                <TableRow
                  key={entry.name}
                  hover
                  selected={isSelected}
                  onClick={() => onSelect(entry)}
                  onDoubleClick={() => entry.isDir && onNavigateInto(entry)}
                  sx={{ cursor: "pointer" }}
                  data-testid="directory-entry-row"
                  data-entry-name={entry.name}
                  data-entry-type={entry.isDir ? "directory" : "file"}
                >
                  <TableCell sx={{ minWidth: 0, overflow: "hidden" }}>
                    <Box sx={{ display: "flex", alignItems: "center", gap: 1, minWidth: 0 }}>
                      {view.isDir ? (
                        <FolderIcon sx={{ fontSize: 16 }} />
                      ) : (
                        <InsertDriveFileIcon sx={{ fontSize: 16 }} />
                      )}
                      <Typography noWrap title={view.name} sx={{ fontSize: "inherit" }}>
                        {view.name}
                      </Typography>
                    </Box>
                  </TableCell>
                  <TableCell
                    align="right"
                    sx={{ fontVariantNumeric: "tabular-nums", whiteSpace: "nowrap" }}
                  >
                    {view.sizeLabel}
                  </TableCell>
                  <TableCell sx={{ fontVariantNumeric: "tabular-nums", whiteSpace: "nowrap" }}>
                    {view.modifiedLabel ?? "—"}
                  </TableCell>
                  <TableCell>{formatAttributes(view.attributes)}</TableCell>
                </TableRow>
              );
            })}
          </TableBody>
        </Table>
      </TableContainer>
    </Box>
  );
}
