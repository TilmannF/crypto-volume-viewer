# Dependency Licenses

This is a lightweight inventory of how to check dependency licenses, not a full audit. It exists so a technical user (or a future contributor) can quickly see the license posture of the main frameworks this project builds on, and knows how to re-check the full dependency tree before any public release.

This document covers third-party dependencies only. The project's own license is Apache-2.0 — see [`LICENSE`](../LICENSE) and [license-decision.md](license-decision.md).

## Rust dependency license inventory

No license-audit tool is added as a mandatory build dependency. Two ways to inspect Rust dependency licenses, from lightest to more detailed:

**Dependency-free** (works with only `cargo` and `python3`/`jq`, already available in this project's toolchain):

```bash
cargo metadata --format-version 1 --all-features | python3 -c "
import json, sys
d = json.load(sys.stdin)
for pkg in sorted(d['packages'], key=lambda p: p['name']):
    if pkg.get('license'):
        print(f\"{pkg['name']} {pkg['version']}: {pkg['license']}\")
"
```

(or pipe through `jq '.packages[] | select(.license) | \"\(.name) \(.version): \(.license)\"'` if `jq` is preferred over `python3`.) Run from any crate's directory with `--all-features` to include the `e2e-webdriver`-gated dependencies too.

**Optional dev tool** for a more readable report: [`cargo-license`](https://crates.io/crates/cargo-license) (`cargo install cargo-license`, then `cargo license` from the workspace root). Not installed or required by this repository; a developer can install it locally if they want a nicer summary.

## Frontend dependency license inventory

From `apps/cryptovol-gui`, run via `npx` (not installed as a project dependency):

```bash
npx license-checker --summary
```

For a per-package breakdown instead of a summary: `npx license-checker`.

## Known key GUI dependencies

Verified directly against each installed package's own metadata (`node_modules/*/package.json`'s `license` field, or `cargo metadata`'s `license` field for the Rust crate), not assumed:

| Dependency | License |
|---|---|
| Tauri (Rust crate `tauri`, `tauri-plugin-dialog`) | Apache-2.0 OR MIT |
| Tauri (JS `@tauri-apps/api`, `@tauri-apps/cli`) | Apache-2.0 OR MIT |
| React | MIT |
| MUI (`@mui/material`) | MIT |
| WebdriverIO (`webdriverio`, `@wdio/cli`, `@wdio/tauri-service`) | MIT |
| `tauri-plugin-wdio` (E2E-only, `e2e-webdriver` feature) | MIT OR Apache-2.0 |
| `tauri-plugin-wdio-webdriver` (E2E-only, `e2e-webdriver` feature) | MIT |

All of the above are permissive (MIT and/or Apache-2.0), consistent with this project's own dependency tree generally (see [license-decision.md](license-decision.md) for how that informs this project's own license options).

## Before public distribution

Re-run both inventory commands above and review any dependency added since this document was last updated -- new dependencies are not automatically covered by this snapshot. This is especially important for any dependency outside the permissive MIT/Apache-2.0 family, which none of the above are today, but which a future addition might be.
