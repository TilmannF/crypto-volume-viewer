import { createTheme } from "@mui/material/styles";

/**
 * Deliberately dense, desktop-utility theme (Total Commander reference).
 * See .work/gui-density-redesign/spec.yaml for the density rationale.
 */
export const theme = createTheme({
  spacing: 4,
  shape: {
    borderRadius: 3,
  },
  typography: {
    fontSize: 13,
  },
  components: {
    MuiButton: {
      defaultProps: {
        size: "small",
      },
      styleOverrides: {
        root: {
          minHeight: 26,
          textTransform: "none",
        },
      },
    },
    MuiIconButton: {
      defaultProps: {
        size: "small",
      },
    },
    MuiTextField: {
      defaultProps: {
        size: "small",
      },
    },
    MuiFormControl: {
      defaultProps: {
        size: "small",
      },
    },
    MuiSelect: {
      defaultProps: {
        size: "small",
      },
    },
    MuiButtonBase: {
      defaultProps: {
        disableRipple: true,
      },
    },
    MuiTableCell: {
      styleOverrides: {
        root: {
          padding: "2px 8px",
          fontSize: 13,
        },
        head: {
          fontWeight: 600,
          whiteSpace: "nowrap",
        },
      },
    },
    MuiToolbar: {
      styleOverrides: {
        dense: {
          minHeight: 34,
        },
      },
    },
    MuiAlert: {
      styleOverrides: {
        root: {
          paddingTop: 0,
          paddingBottom: 0,
          alignItems: "center",
        },
        message: {
          padding: "4px 0",
        },
        icon: {
          padding: "4px 0",
        },
      },
    },
  },
});
