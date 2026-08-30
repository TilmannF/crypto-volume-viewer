# 50 GitHub and CI Policy

This policy defines repository, git-history, GitHub Actions, and release-channel rules for `cryptovol`.

It is subordinate to `00-engineering-policy.md`. When rules conflict, this file wins for GitHub, git history, CI workflows, Dependabot, tags, and repository visibility.

Rust, frontend, and Tauri policies still govern what the CI *commands* check. This file governs *how* those checks run on GitHub and *how* history is published.

Normative `MUST` / `MUST NOT` / `SHOULD` / `SHOULD NOT` / `MAY` follow `00-engineering-policy.md`.

This policy is written for humans and AI agents. Agents MUST follow it even when GitHub settings have not caught up yet.

## 1. Goals

GitHub and CI MUST optimize for:

1. Correctness of `main`.
2. Hang-free, terminating checks.
3. Least privilege (tokens, secrets, workflow permissions).
4. Reproducible, reviewable workflow definitions.
5. A public history that explains *why*, not agent thrash.
6. Explicit human consent for irreversible forge actions.

CI MUST NOT be used as a password prompt, a KDF autoprobe farm, a notarization machine, or a place to store signing keys.

## 2. Scope

This policy applies to:

```text
.github/
.gitattributes
.gitignore
```

and to git operations against `origin`, GitHub repository settings, tags, GitHub Releases, and Actions.

It does not replace:

* `docs/release-checklist.md` (how a human cuts a build)
* `docs/packaging-macos.md` / `docs/packaging-appstore.md` (how a human signs)
* `docs/test-containers.md` (fixture contracts)

Those docs stay the how-to. This file is the MUST/MUST NOT.

## 3. Canonical repository

The canonical source repository is public at a stable HTTPS URL.

Agents MUST NOT change repository visibility, rename the canonical remote, or stand up a second “official” repository.

Changing visibility is an exceptional operational act (incident, legal). It requires a human. It is not part of normal development.

## 4. Default branch and pull requests

The default branch is `main`.

Changes that land on `origin/main` MUST go through a pull request.

Agents MUST NOT `git push` to `origin/main`. Open a branch and a PR.

GitHub MUST enforce on `main`:

* Pull request required before merging.
* Named required status checks matching the `ci` workflow jobs.
* No force-push.
* No deletion of `main`.
* Rules apply to administrators (no god-mode bypass).

Agents MUST follow these rules even when GitHub settings fail to enforce them.

A second human reviewer is SHOULD once there is more than one maintainer. A solo maintainer MAY merge their own PR after CI is green.

## 5. History and squash

`main` SHOULD be linear.

The preferred merge method is **squash**. One PR MUST become one commit on `main`.

That squash commit message MUST describe the change (what + why). It MUST NOT be a dump of agent micro-commits (`wip`, `fix clippy`, `try again`, `ai changes`).

On a feature branch, agents MAY make many small commits. Those commits are disposable. They MUST be squashed at merge (GitHub “Squash and merge” or an equivalent local squash).

Squash-merge of a PR is not a history rewrite. Force-push of `origin/main` is.

`origin/main` MUST NOT be force-pushed. Published tags MUST NOT be deleted or moved.

Exception: a human MAY authorize a force-push of `main` or a tag move as incident response (leaked secret, legal takedown). Agents MUST NOT do this unprompted.

## 6. Tags and GitHub Releases

Public versions use `x.y.z` only (no `-beta` in `CFBundleShortVersionString`).

A GitHub Release MUST:

* Be cut from a git tag of that version.
* Include a human-readable changelog of functional and security-relevant changes.
* Include a SHA-256 (or stronger) checksum for every attached binary asset.

Agents MUST NOT create a version tag or GitHub Release unless a human asked for that release. They MUST NOT delete, retag, or overwrite a tag that has existed on `origin`.

CI MUST NOT be the job that Developer-ID-signs or notarizes macOS builds. Signing stays on the maintainer Mac until this policy is explicitly amended.

Apple API keys, Developer ID identities, notary credentials, and App Store Connect keys MUST NOT appear in Actions secrets, workflow files, or logs.

A future GitHub-hosted or self-hosted Mac runner for signing is an explicit policy change, not an agent shortcut.

SLSA provenance and Sigstore/cosign are out of scope until requested.

## 7. What CI is for

The `ci` workflow MUST verify the tree. It MUST NOT publish, sign, notarize, or deploy.

On `pull_request` and on `push` to `main`, CI MUST run the full current matrix:

* `rust` on `ubuntu-latest`, `macos-latest`, and `windows-latest`
* `gui frontend` typecheck and unit tests on Linux

Agents MUST NOT drop an OS from that matrix to “make CI faster.” Runner hardware does not change this requirement.

`strategy.fail-fast` MUST be `false` for the rust matrix so one OS failure still reports the others.

E2E (macOS Tauri/WebDriver), VeraCrypt CLI fixture generators, and the crypto-matrix KDF autoprobe MUST NOT run in this workflow.

## 8. Timeouts

Every job MUST set `timeout-minutes`. Hanging is a failure.

Defaults unless a documented reason exists:

* rust matrix jobs: `45` (Windows hosted clippy + test + fixtures can take ~20–35 min)
* gui frontend job: `15`

A job that can block on a password prompt, a TTY, or a KDF autoprobe MUST also bound the *process* (test helper timeout, `timeout(1)`, or skip on that OS). GitHub’s 6-hour default is not a hang detector.

If a test cannot be made to terminate quickly on an OS (for example Windows `rpassword` on `CONIN$`), it MUST be skipped on that OS with a comment pointing at this rule, not left to run forever.

## 9. Fixture environment in CI

CI MAY set these, pointing at committed fixtures under `testdata/static/`:

```text
CRYPTOVOL_STATIC_FAT_FIXTURE
CRYPTOVOL_STATIC_FAT_LFN_FIXTURE
CRYPTOVOL_STATIC_EXFAT_LFN_FIXTURE
CRYPTOVOL_STATIC_NTFS_LFN_FIXTURE
```

CI MUST NOT set `CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR`. Those ignored tests autoprobe extra KDFs and can run for tens of minutes.

`cargo test -- --ignored` in CI MUST `--exclude cryptovol-cli`. Some CLI ignored tests spawn the binary without `--kdf` and autoprobe.

Fixture bytes MUST be preserved exactly across checkouts. `testdata/**` MUST be marked binary in `.gitattributes` (`-text`). Agents MUST NOT “fix” CRLF in fixtures.

## 10. Commands CI runs

Rust jobs MUST run the same quality bar as local development:

```bash
cargo fmt --all --check          # at least on Linux
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets
cargo test --workspace --all-targets --exclude cryptovol-cli -- --ignored
```

GUI jobs MUST run:

```bash
npm ci
npm run typecheck
npm test
```

from `apps/cryptovol-gui`.

A Tauri `generate_context!` build in CI MUST have a placeholder `apps/cryptovol-gui/dist/index.html`. Agents MUST NOT require a full frontend production build just to satisfy clippy.

The rust toolchain channel MAY be `stable`. The *action* that installs it MUST still be SHA-pinned, on `master` history, with an explicit `toolchain:` input (section 11). A `rust-toolchain.toml` pin is SHOULD.

Node MUST be an explicit LTS major (currently 22), not `node-version: node`.

## 11. Workflow security

### Permissions

Every workflow MUST declare top-level permissions. The CI workflow MUST use at most:

```yaml
permissions:
  contents: read
```

Jobs MUST NOT raise permissions unless a future dedicated release workflow needs it, and then only on that job.

The repository default Actions token SHOULD be read-only in GitHub settings.

### Pinning

Every third-party `uses:` MUST be pinned to a full 40-character commit SHA, with a comment naming the tag or version:

```yaml
- uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4.4.0
```

Mutable tags (`@v4`, `@stable`, `@main`) MUST NOT appear as the pin.

Exception: a human MAY grant a documented, time-bounded exception to SHA pinning. Agents MUST NOT take that exception themselves. Dependabot PRs are the normal way to move pins.

Prefer first-party `actions/*` and well-known installers (`dtolnay/rust-toolchain`). New marketplace actions need a human reason.

`dtolnay/rust-toolchain` MUST be pinned to a `master` commit SHA and MUST pass `toolchain:` in `with:`. Pinning a `stable`/`nightly` branch tip without that input is incorrect for this action.

### Checkout

`actions/checkout` MUST set `persist-credentials: false` unless that job authenticates `git push` with `GITHUB_TOKEN`. CI jobs do not push.

### Triggers

CI MUST use `pull_request` and `push` to `main`.

Agents MUST NOT add:

* `pull_request_target` that checks out PR head
* `workflow_run` chains that pass secrets to untrusted artifacts
* `issue_comment` / `workflow_dispatch` inputs interpolated into `run:` without an `env:` intermediate

Untrusted context (`github.event.pull_request.title`, branch names, commit messages) MUST be passed into `run:` only via `env:`.

Fork PRs MUST NOT receive repository secrets (GitHub default). Keep it.

Self-hosted runners MUST NOT execute untrusted `pull_request` code from forks. They do not relax the matrix, timeout, or secret rules.

### Workflow review

`.github/workflows/` and `.github/dependabot.yml` are supply-chain sensitive. A `CODEOWNERS` file MUST list a maintainer for `.github/`. Requiring code-owner reviews in GitHub settings is SHOULD once there is more than one maintainer.

## 12. Secrets

Secrets MUST NOT be committed, logged, or printed in Actions output.

CI MUST NOT gain:

* `APPLE_*` / notary / Developer ID / ISSUER / KEY material
* App Store Connect API keys
* Any password, keyfile, or derived key for real user volumes

Test fixture password `test-password` is public test data. It MAY appear in docs and tests. It MUST NOT be used as a stand-in for a secret in a workflow `env:`.

`secrets: inherit` MUST NOT be used on reusable workflows. Pass named secrets only, and only to jobs that need them.

## 13. Dependabot

The repository MUST have `.github/dependabot.yml` covering:

* `github-actions` at `/`
* `cargo` at `/`
* `npm` at `/apps/cryptovol-gui`

Version updates SHOULD wait several days after a release (explicit `cooldown`; security updates stay immediate).

Dependabot MUST group updates per ecosystem. Each ecosystem MUST define:

* one `groups` rule with `applies-to: version-updates` and `patterns: ["*"]`
* one `groups` rule with `applies-to: security-updates` and `patterns: ["*"]`

So a weekly run opens at most one version-update PR and one security-update PR per ecosystem, not one PR per dependency. Ungrouped leftover PRs mean the group rule did not match; fix the config, do not accept a new flood.

`open-pull-requests-limit` MUST stay small (about 5). It is a backstop, not a substitute for grouping.

Dependabot MUST open PRs. Agents MUST NOT auto-merge GitHub Actions pin bumps. A human SHOULD glance at action SHA PRs (impostor-commit risk). Cargo/npm patch PRs MAY be merged after CI is green.

## 14. Agent operating rules (this domain)

Before changing `.github/`, git history, tags, or visibility, agents MUST read this file.

Agents MUST NOT:

1. Push to `origin/main`.
2. Force-push `origin/main`.
3. Change repository visibility.
4. Delete or move a tag that exists on `origin`.
5. Create a version tag or GitHub Release unless a human asked for that release.
6. Add signing or notarization secrets to Actions.
7. Set `CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR` in CI.
8. Run `cryptovol-cli --ignored` tests in CI.
9. Drop an OS from the CI matrix.
10. Use an unpinned `uses:` tag.
11. Add `pull_request_target`.
12. Lengthen or remove `timeout-minutes` to hide a hang.

If CI hangs, agents MUST treat that as a failure: cancel, bound the process or skip the OS, do not wait out the GitHub default timeout.

## 15. Human review checkpoints

Human review is required before:

* Force-pushing `main`
* Deleting or moving a published tag
* Adding Actions secrets
* Adding a workflow that publishes or signs
* Attaching a self-hosted runner
* Disabling a required CI job
* Changing the squash-on-merge or PR-required rules
