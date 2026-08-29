# License Decision

## Decision

**Resolved: Apache License, Version 2.0 (Apache-2.0).** Decided by the project owner on 2026-07-05.

- Copyright holder: Tilmann Felgner
- Copyright notice: `Copyright 2026 Tilmann Felgner`
- Author: Tilmann Felgner <cryptovol@flgnr.com>

This decision is enacted in:

1. [`LICENSE`](../LICENSE) — the full Apache-2.0 text, with the copyright notice above prepended.
2. `Cargo.toml` (`[workspace.package]`) and every crate's `[package]` section — `license.workspace = true` / `authors.workspace = true`.
3. [`apps/cryptovol-gui/package.json`](../apps/cryptovol-gui/package.json) — `"license"` and `"author"` fields.
4. [`README.md`](../README.md) — Beta Status bullet and dedicated License section.

No further license decision is open. This document exists as a historical record of the choice, not as a live discussion.

## Why Apache-2.0

Apache-2.0 is a permissive license, similar in spirit to MIT but with an explicit patent grant and patent-retaliation clause. It keeps the project's own license in the same permissive family as its dependency tree, which is already predominantly MIT/Apache-2.0 (see [dependency-licenses.md](dependency-licenses.md)) — a common, low-friction choice in the Rust ecosystem. Other options (MIT, dual MIT OR Apache-2.0, AGPL-3.0-or-later, source-available/custom) were considered earlier in the project's history but are no longer under consideration.
