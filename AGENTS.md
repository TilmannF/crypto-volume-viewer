# AGENTS.md

## Agent Instructions

Before modifying code, read:

1. `policies/00-engineering-policy.md`
2. `policies/10-rust-project-structure-policy.md`
3. `policies/20-rust-code-policy.md`

These policies are normative. In case of conflict, the more specific Rust policy overrides the general engineering policy.

## Project Name

`cryptovol`

## Project Goal

`cryptovol` is a cross-platform, read-only command-line tool for inspecting and extracting files from encrypted volume/container formats without mounting them.

The project starts with support for TrueCrypt/VeraCrypt-compatible file-hosted containers, but the long-term design must remain generic enough to support additional encrypted volume formats later.

The tool must work on:

* macOS
* Linux
* Windows

The project is intended to be developed primarily with AI assistance. Therefore, all tasks must be broken down into small, testable, reviewable increments.

## Workflow Artifact Policy

Local planning and review artifacts under `.work/<feature-slug>/` are workflow state for the active agent session. They should remain on disk for local continuity, but they are intentionally ignored by Git and should not be committed unless a future repository policy explicitly changes this rule.

## Product Positioning

Use the public-facing name:

```text
Crypto Volume Viewer
```

Use the CLI/repository name:

```text
cryptovol
```

Do not brand the project as “VeraCrypt Viewer”.

Acceptable technical phrasing:

```text
A read-only userspace explorer for encrypted volume/container formats.
Initial support target: TrueCrypt/VeraCrypt-compatible file containers.
```

The project is not affiliated with VeraCrypt, TrueCrypt, Microsoft, Apple, or any other vendor.

## Core Principles

1. Read-only by design.
2. No kernel extensions.
3. No FUSE dependency.
4. No mounting.
5. No write support in the MVP.
6. No automatic preview extraction to insecure temporary files.
7. Explicit user action required for every extracted file.
8. Favor correctness and safety over speed.
9. Use memory-safe Rust wherever possible.
10. Treat all container contents as untrusted binary input.

## Initial MVP Scope

The first MVP should support:

* Cross-platform CLI.
* File-hosted encrypted container files only.
* TrueCrypt/VeraCrypt-compatible volume format backend.
* AES-XTS only.
* Password-based opening.
* Optional PIM support if practical.
* No keyfiles initially.
* No hidden volume support initially.
* No partition-hosted/system volumes.
* Read-only decrypted block reader.
* FAT32 or exFAT filesystem support.
* Directory listing.
* Single-file extraction.

MVP commands:

```bash
cryptovol info <container>
cryptovol test-open <container>
cryptovol ls <container> <path>
cryptovol extract <container> <source-path> <destination-path>
```

Example:

```bash
cryptovol info backup.hc
cryptovol test-open backup.hc
cryptovol ls backup.hc /
cryptovol extract backup.hc /documents/report.pdf ./report.pdf
```

## Long-Term Scope

Possible later backends:

* TC/VC-compatible volumes
* LUKS/dm-crypt
* BitLocker
* encrypted DMG
* FileVault/APFS encrypted volumes

Possible later filesystems:

* FAT32
* exFAT
* NTFS
* ext2/3/4
* HFS+
* APFS

Long-term commands may include:

```bash
cryptovol tree <container>
cryptovol cat <container> <source-path>
cryptovol hash <container> <source-path>
cryptovol metadata <container> <source-path>
cryptovol extract-dir <container> <source-dir> <destination-dir>
```

## Non-Goals

Do not implement these unless explicitly requested later:

* Write support.
* In-place modification of volumes.
* Mounting.
* FUSE integration.
* Kernel drivers.
* Password recovery.
* Brute forcing.
* Forensic bypass functionality.
* Cracking workflows.
* Automatic cloud sync integration.
* GUI.
* Background daemon.
* System-volume decryption.
* Boot-volume support.

## Security Model

The tool is for legitimate access to containers where the user already has the password/key material.

The tool must not include:

* Password cracking features.
* Brute-force automation.
* Wordlist support.
* Distributed cracking support.
* Credential harvesting.
* Attempts to bypass authentication.
* Exploit logic.
* Hidden-volume detection heuristics that violate plausible deniability.

The tool may include:

* Opening a volume with a provided password.
* Trying known supported KDF/hash/encryption combinations needed for format compatibility.
* Clear errors for unsupported formats.
* Safe read-only extraction.

## Architecture Overview

The architecture must separate encrypted volume handling from filesystem parsing.

```text
CLI
  ↓
format detector
  ↓
crypto volume backend
  ↓
decrypted block reader
  ↓
filesystem detector
  ↓
filesystem reader
  ↓
directory listing / file extraction
```

Core abstraction:

```text
Encrypted container file
  → crypto volume backend
  → decrypted block device abstraction
  → filesystem parser
  → files/directories
```

## Main Internal Concepts

### Crypto Volume Backend

A crypto volume backend answers:

```text
How do we turn this encrypted container into a read-only decrypted block reader?
```

Examples:

* `tcvc`
* `luks`
* `bitlocker`
* `dmg`

### Filesystem Backend

A filesystem backend answers:

```text
How do we interpret decrypted blocks as directories and files?
```

Examples:

* `fat`
* `exfat`
* `ntfs`
* `ext`
* `apfs`

### Block Reader

A decrypted block reader must expose safe random-access reads over the decrypted logical volume.

Suggested conceptual trait:

```rust
pub trait BlockReader {
    fn len(&self) -> u64;
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<()>;
}
```

Avoid leaking implementation details from one layer into another.

## Suggested Repository Layout

Start simple, but keep boundaries clear.

```text
cryptovol/
  AGENTS.md
  README.md
  Cargo.toml
  crates/
    cryptovol-cli/
      Cargo.toml
      src/
        main.rs
        commands/
          mod.rs
          info.rs
          test_open.rs
          ls.rs
          extract.rs

    cryptovol-core/
      Cargo.toml
      src/
        lib.rs
        error.rs
        block_reader.rs
        volume.rs
        filesystem.rs
        util.rs

    cryptovol-tcvc/
      Cargo.toml
      src/
        lib.rs
        header.rs
        kdf.rs
        xts.rs
        volume.rs

    cryptovol-fs-fat/
      Cargo.toml
      src/
        lib.rs
        fat32.rs
        exfat.rs

  tests/
    integration/
      cli_smoke.rs

  docs/
    architecture.md
    security.md
    format-support.md
    test-containers.md
```

If the workspace layout feels too heavy during the earliest bootstrap, start with one crate and split later. However, the logical module boundaries must remain.

## Preferred Language

Use Rust.

Reasons:

* Strong fit for binary parsing.
* Good fit for untrusted input handling.
* No garbage collector.
* Strong typing.
* Excellent CLI ecosystem.
* Good cross-platform support.
* Good long-term library/CLI separation.
* Compile-time enforcement helps AI-generated code converge toward correctness.

Do not switch to Go unless explicitly requested.

## Rust Guidelines

Use stable Rust.

Prefer:

* clear types
* explicit errors
* small functions
* small modules
* exhaustive tests
* no unsafe code unless absolutely necessary

Avoid:

* clever lifetime-heavy abstractions early
* premature async
* global mutable state
* panics in library code
* unwrap/expect outside tests or CLI top-level handling
* custom cryptographic primitive implementations unless unavoidable

For errors, prefer:

```rust
thiserror
anyhow
```

Suggested usage:

* `thiserror` in library crates.
* `anyhow` in CLI crate.

For CLI parsing, prefer:

```rust
clap
```

For password input, prefer a crate that avoids echoing input.

For zeroizing secrets, prefer:

```rust
zeroize
secrecy
```

## Crypto Guidelines

Do not implement cryptographic primitives from scratch.

Use well-maintained RustCrypto or similarly reputable crates when possible.

Acceptable categories:

* AES block cipher implementation from a reputable crate.
* XTS mode from a reputable crate if available and suitable.
* PBKDF2 from a reputable crate.
* SHA-256/SHA-512 from reputable crates.
* Argon2id from a reputable crate.
* HMAC from a reputable crate.
* Constant-time comparison from a reputable crate.

If no suitable XTS implementation exists, implement only the XTS orchestration around well-tested AES primitives and test thoroughly against known vectors.

All key material must be zeroized when possible.

Passwords must not be logged.

Derived keys must not be printed.

Debug output must never include secrets.

## File Format Compatibility

The first compatibility target is TC/VC-style encrypted volume files.

Implementation must be based on public format documentation and black-box compatibility tests with locally generated test containers.

Do not copy code from VeraCrypt or TrueCrypt unless licensing has been explicitly reviewed and accepted.

Prefer clean-room reimplementation based on:

* public documentation
* independent tests
* known-good locally generated containers

The project may mention compatibility with TC/VC-style volumes, but must not imply affiliation.

## Naming Rules

Use internal backend name:

```text
tcvc
```

Meaning:

```text
TrueCrypt/VeraCrypt-compatible
```

Avoid package names that imply official association, such as:

```text
veracrypt-core
veracrypt-reader
truecrypt-viewer
```

Preferred names:

```text
cryptovol-tcvc
cryptovol-core
cryptovol-cli
```

## CLI UX Rules

The CLI should be boring, explicit, and scriptable.

Good:

```bash
cryptovol ls backup.hc /
cryptovol extract backup.hc /foo/bar.txt ./bar.txt
cryptovol info backup.hc
```

Avoid interactive flows unless needed for password entry.

Password handling:

* If no password option is provided, prompt securely.
* Do not echo password input.
* Avoid accepting passwords via command-line arguments if possible, because they may leak through shell history or process lists.
* If a password argument is implemented for automation, document the risk clearly.
* Prefer `--password-file` only with strong warnings, if implemented later.

Exit codes:

```text
0 = success
1 = generic error
2 = invalid arguments
3 = unsupported format
4 = authentication failed
5 = filesystem not recognized
6 = extraction failed
```

Errors must be human-readable.

Machine-readable JSON output may be added later behind:

```bash
--json
```

## Initial Commands

### `info`

Purpose:

Show basic container-level information that can be determined safely.

Example:

```bash
cryptovol info backup.hc
```

Output should include:

```text
File: backup.hc
Size: 104857600 bytes
Detected: unknown encrypted/random-looking volume
Supported backends attempted: tcvc
Opened: no
```

If password is provided and opening succeeds:

```text
File: backup.hc
Size: 104857600 bytes
Backend: tcvc
Volume size: ...
Filesystem: FAT32
Read-only: yes
```

### `test-open`

Purpose:

Check whether the volume can be opened with the provided password.

Example:

```bash
cryptovol test-open backup.hc
```

Output:

```text
Volume opened successfully.
```

Or:

```text
Could not open volume: authentication failed or unsupported parameters.
```

Do not reveal which exact KDF/hash/encryption combination failed in a way that helps attackers beyond normal compatibility diagnostics.

### `ls`

Purpose:

List files/directories inside the decrypted volume.

Example:

```bash
cryptovol ls backup.hc /
```

Output should be stable and parseable enough for humans.

Example:

```text
drwxr-xr-x          0  Documents
-rw-r--r--    1048576  report.pdf
-rw-r--r--       4096  notes.txt
```

### `extract`

Purpose:

Extract exactly one file from the volume.

Example:

```bash
cryptovol extract backup.hc /Documents/report.pdf ./report.pdf
```

Rules:

* Refuse to overwrite existing files unless `--overwrite` is explicitly passed.
* Create parent directories only if `--parents` is explicitly passed.
* Preserve file content exactly.
* Metadata preservation can come later.
* Never modify the source container.

## Testing Strategy

Tests are mandatory.

Every AI-generated implementation step should include tests.

Test categories:

```text
unit tests
integration tests
known-answer tests
negative tests
fuzz/property tests later
```

### Unit Tests

Use unit tests for:

* binary parsing helpers
* offset calculations
* sector/tweak calculations
* KDF parameter parsing
* filesystem structures
* path normalization

### Integration Tests

Use integration tests for:

* CLI smoke tests
* opening tiny test containers
* listing known directory trees
* extracting known files
* failure on wrong password
* failure on unsupported format
* refusal to overwrite

### Test Containers

Do not commit real private containers.

Small synthetic test containers may be generated locally and committed only if:

* they contain no private data
* passwords are public test passwords
* licensing is clear
* size is small
* they are explicitly documented as test fixtures

Preferred test password:

```text
test-password
```

Preferred test files:

```text
/hello.txt
/dir/nested.txt
/binary.bin
```

Alternative:

Provide a script that generates test containers locally using installed external tools, while keeping generated files out of Git.

Use:

```gitignore
testdata/generated/
.examples/
*.hc
*.tc
```

unless a fixture is intentionally committed.

## AI Development Workflow

This project is intended to be implemented with AI assistance.

AI agents must follow this workflow:

1. Read `AGENTS.md` and the normative policies listed in the Agent Instructions section.
2. Read `README.md` if present.
3. Inspect existing code before modifying.
4. Make the smallest useful change.
5. Add or update tests.
6. Run formatting.
7. Run tests.
8. Summarize what changed.
9. Mention what remains unsupported.

Do not generate large untested code dumps.

Do not rewrite unrelated files.

Do not introduce new dependencies without explaining why.

Do not change public behavior silently.

Every change should be suitable for a small pull request.

## Prompting Rules For AI Agents

When asking an AI agent to implement something, prefer prompts like:

```text
Implement only the BlockReader abstraction and tests. Do not implement VeraCrypt parsing yet.
```

Good task size:

```text
Add a CLI skeleton with clap and commands info/test-open/ls/extract that currently return "not implemented".
```

Bad task size:

```text
Build the whole VeraCrypt viewer.
```

Good task size:

```text
Implement reading the first 512 bytes and the backup header location calculation. Add tests using temporary files.
```

Bad task size:

```text
Implement full VeraCrypt compatibility.
```

AI agents must stop and ask for human review when:

* cryptographic behavior is unclear
* licensing is unclear
* unsafe Rust appears necessary
* a dependency is unmaintained or suspicious
* a test vector cannot be found
* format documentation conflicts with observed behavior

## Implementation Roadmap

### Phase 0 — Bootstrap

Goal:

Create a compiling Rust workspace with empty CLI commands.

Tasks:

* Create workspace.
* Add `cryptovol-cli`.
* Add `cryptovol-core`.
* Add basic error types.
* Add `clap`.
* Implement commands returning “not implemented”.
* Add smoke tests.
* Add CI later.

Definition of done:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features
cargo test
```

passes.

### Phase 1 — Generic BlockReader

Goal:

Implement generic read-only block/file abstractions.

Tasks:

* Define `BlockReader`.
* Implement `FileBlockReader`.
* Add safe `read_exact_at`.
* Add offset/length validation.
* Add tests for reads, EOF, invalid offsets.

Definition of done:

* Can read arbitrary byte ranges from a normal file.
* No crypto yet.

### Phase 2 — Raw Header Reading

Goal:

Read candidate header locations for TC/VC-style file containers.

Tasks:

* Add `cryptovol-tcvc`.
* Read primary header candidate.
* Read backup header candidate.
* Add container size checks.
* Do not decrypt yet.
* Add tests for header offset calculations.

Definition of done:

* `cryptovol info` can show file size and candidate header offsets.

### Phase 3 — KDF/Header Decryption Skeleton

Goal:

Create the structure for trying supported KDF/hash/encryption combinations.

Tasks:

* Define KDF parameter structs.
* Define supported algorithms enum.
* Implement password prompt.
* Implement secret zeroization.
* Add “unsupported until implemented” paths.

Definition of done:

* CLI can prompt for password safely.
* No secrets are logged.

### Phase 4 — AES-XTS Header Open

Goal:

Open a minimal supported TC/VC-style test container.

Tasks:

* Implement required KDF(s).
* Implement AES-XTS decrypt path.
* Validate decrypted header.
* Extract volume metadata.
* Add known-good test fixture or generator script.

Definition of done:

* `cryptovol test-open <container>` succeeds for one known test container.
* Wrong password fails.

### Phase 5 — Decrypted Data BlockReader

Goal:

Expose decrypted volume contents as a random-access block reader.

Tasks:

* Implement logical-to-physical offset mapping.
* Implement sector-aligned decrypt reads.
* Handle partial reads.
* Add tests for known plaintext sectors.

Definition of done:

* Can read decrypted bytes from known offsets.

### Phase 6 — FAT32 or exFAT Read-Only

Goal:

List files from a simple filesystem inside an opened container.

Tasks:

* Implement or integrate FAT32/exFAT read-only parser.
* Detect filesystem.
* Read root directory.
* List entries.
* Normalize paths safely.

Definition of done:

* `cryptovol ls <container> /` lists known files.

### Phase 7 — File Extraction

Goal:

Extract a single file.

Tasks:

* Implement file read stream.
* Implement output writing.
* Refuse overwrite by default.
* Add tests for extracted file hashes.

Definition of done:

* `cryptovol extract` extracts known files byte-for-byte.

### Phase 8 — Hardening

Goal:

Improve reliability and safety.

Tasks:

* Add more malformed input tests.
* Add fuzz targets if practical.
* Add test matrix.
* Add cross-platform CI.
* Review dependencies.
* Document limitations.

Definition of done:

* Robust error behavior.
* No panics on malformed containers.
* Clear documentation.

## Dependency Policy

Dependencies are allowed, but must be justified.

Before adding a dependency, check:

* Is it maintained?
* Is the license compatible?
* Is it widely used?
* Does it avoid unsafe code where practical?
* Is the API stable enough?
* Can we test its behavior?

Avoid dependencies for trivial helpers.

Accept dependencies for:

* CLI parsing
* error handling
* password input
* cryptographic primitives
* zeroization
* filesystem parsing, if high quality
* temporary files in tests

## Licensing Policy

Do not copy source code from VeraCrypt, TrueCrypt, dislocker, cryptsetup, libguestfs, or filesystem projects unless the license is explicitly reviewed.

Implement compatibility from public documentation and tests.

The repository license is not decided in this file.

Until a license is chosen, avoid importing code snippets from external projects.

Document any dependency licenses.

## Documentation Requirements

Keep docs updated as functionality changes.

Required docs:

```text
README.md
docs/architecture.md
docs/security.md
docs/format-support.md
docs/test-containers.md
```

README should include:

* What the tool does.
* What it does not do.
* Supported platforms.
* Supported volume formats.
* Supported filesystems.
* Security model.
* Basic examples.
* Warning that extracted files are unencrypted.

## Security Documentation

`docs/security.md` must explain:

* Read-only design.
* No mount design.
* No FUSE/kernel extension design.
* Password handling.
* Secret zeroization.
* Temporary file policy.
* Extraction risks.
* Hidden volume considerations.
* Unsupported brute-force/cracking workflows.
* Responsible disclosure contact once available.

## Temporary File Policy

Do not write decrypted content to temporary files unless explicitly required.

For preview-like behavior, prefer streaming to stdout only when requested.

If temporary files are ever needed:

* Make this explicit.
* Use secure file creation.
* Avoid world-readable permissions.
* Delete when done.
* Document residual risk.

## Logging Policy

Default logging must be minimal.

Never log:

* passwords
* keyfiles
* derived keys
* salts plus password-derived values
* decrypted header contents that include key material
* decrypted file contents
* full private file paths unless necessary

Debug logging must still avoid secrets.

## Path Handling Rules

Paths inside containers are untrusted.

Prevent:

* path traversal
* absolute path extraction surprises
* overwriting unintended files
* symlink attacks during extraction
* platform-specific separator bugs

Extraction rules:

* Container paths use `/`.
* Destination paths use host OS conventions.
* Refuse overwrite by default.
* Normalize source paths.
* Do not allow `..` to escape intended extraction root when extracting directories later.

## Error Handling Rules

Library crates must return typed errors.

CLI crate may convert to human-readable messages.

Bad:

```rust
panic!("bad header")
unwrap()
expect("works")
```

Good:

```rust
return Err(Error::InvalidHeader);
```

Authentication failures should not reveal unnecessary details.

Unsupported formats should be clear but not noisy.

## Performance Goals

MVP performance can be modest.

Correctness matters more than speed.

Later optimizations may include:

* sector cache
* read-ahead
* parallel extraction
* mmap, only after careful review
* streaming hash calculation

Do not optimize before tests exist.

## Cross-Platform Requirements

The tool must avoid Unix-only assumptions.

Use Rust standard library abstractions where possible.

Do not assume:

* `/tmp`
* `/`
* case-sensitive filesystem
* Unix permissions
* fork
* shell-specific behavior

Windows must be considered from the start.

CI should eventually run on:

* ubuntu-latest
* macos-latest
* windows-latest

## Code Style

Prefer boring code.

Use:

```bash
cargo fmt
cargo clippy --all-targets --all-features
cargo test
```

No large functions.

No unrelated refactors.

No hidden behavior changes.

Public APIs should have short doc comments once stable.

## Commit Style

Use concise commits.

Examples:

```text
bootstrap rust workspace
add block reader trait
add file-backed block reader tests
add tcvc header offset calculation
add password prompt
```

Avoid:

```text
misc
stuff
big update
ai changes
```

## Human Review Checkpoints

Human review is required before:

* claiming compatibility with a real-world volume format
* adding write support
* adding hidden volume support
* adding keyfile support
* adding new crypto algorithms
* adding unsafe Rust
* publishing binaries
* choosing a license
* accepting external contributions

## Current Default Decisions

These defaults are assumed until changed:

```text
Language: Rust
CLI name: cryptovol
Product name: Crypto Volume Viewer
Initial backend: tcvc
Initial filesystem: FAT32 or exFAT
Mode: read-only
Mounting: never in MVP
FUSE: no
Kernel extensions: no
GUI: no
Password cracking: no
License: Apache-2.0
```

## Status

This project is at bootstrap stage.

No compatibility claims should be made yet.

Everything is experimental until tests prove otherwise.
