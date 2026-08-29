# Contributing

Thanks for looking. This is a small, read-only encrypted-volume explorer. It is [written by AI](docs/ai.md) under human direction. Contributions can come from humans or models. The bar is the same.

## Before you write code

1. Read [README.md](README.md) and [docs/security.md](docs/security.md).
2. `AGENTS.md` and `policies/` are the product rules. Read-only, no mount, no FUSE, no write support, no cracking.
3. Open an issue first if the change is more than a small fix.

## Will not be accepted

- Password recovery, brute force, wordlists, or “help me open this container I forgot”
- Hidden-volume hunting
- Write/mount/FUSE/kernel features
- Secrets, signing certs, or notarization keys in the tree
- Drive-by refactors with no tests

## How to work

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets
```

From `apps/cryptovol-gui`:

```bash
npm ci
npm run typecheck
npm test
```

E2E (`npm run test:e2e`) is local/macOS. Not required for every docs PR.

Fixture tests under `testdata/static/` are `#[ignore]` unless you set the env vars in [docs/release-checklist.md](docs/release-checklist.md). CI sets them.

**Clone is large (~120 MB of committed test containers).** That is intentional. See the README.

## Pull requests

- One purpose per PR
- Tests for behavior changes
- No `unwrap`/`expect` in library code
- Do not bump version or cut a release unless asked
