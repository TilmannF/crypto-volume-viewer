# Rust Project Structure Policy

This policy defines Rust repository, workspace, crate, dependency, feature, and tooling rules.

General engineering rules are defined in `00-engineering-policy.md`. Code-level Rust rules are defined separately. When rules conflict, the more specific policy wins.

## 1. Goals

The project structure MUST optimize for:

1. Clear ownership.
2. Fast navigation.
3. Small public APIs.
4. Explicit dependencies.
5. Reliable builds.
6. Agent-readable code.
7. Safe incremental change.

The repository MUST NOT grow implicit structure, hidden coupling, or unused compatibility layers.

## 2. Workspace

A repository with more than one Rust crate MUST use a Cargo workspace.

The workspace root `Cargo.toml` SHOULD define:

* `workspace.members`
* `workspace.package`
* `workspace.dependencies`
* `workspace.lints`
* `workspace.metadata` when useful

Common package metadata SHOULD be centralized in `workspace.package`.

Shared dependency versions MUST be centralized in `workspace.dependencies`.

Shared lint configuration SHOULD be centralized in `workspace.lints`.

Crate-local `Cargo.toml` files SHOULD contain only crate-specific metadata, dependencies, features, targets, and overrides.

Workspace members MUST be explicit. Avoid broad globs when they make accidental crate inclusion likely.

## 3. Repository Layout

Rust crates SHOULD live under `crates/`.

Recommended layout:

```text
Cargo.toml
Cargo.lock
README.md
AGENTS.md
policies/
  00-engineering-policy.md
  10-rust-project-structure-policy.md
  20-rust-code-policy.md
crates/
  app-core/
  app-cli/
  app-io/
  app-test-support/
tests/
testdata/
benches/
examples/
docs/
```

The exact names MAY differ, but structure MUST remain simple and predictable.

Crates MUST NOT be nested inside other crates.

Crates MUST NOT be placed under another crate's `src/`.

Top-level directories SHOULD have one clear purpose.

Large unrelated assets MUST NOT be mixed with source code.

## 4. Crate Boundaries

Each crate MUST have one clear responsibility.

Prefer fewer crates until a boundary is real.

Create a new crate when code:

* Has a distinct responsibility.
* Has different dependency needs.
* Needs independent testing.
* Should be reusable.
* Must compile without application infrastructure.

Do not create a crate only to mirror a folder, layer, or future plan.

Crate names MUST be stable, specific, and domain-oriented.

Avoid vague crate names such as `common`, `shared`, `utils`, `helpers`, or `misc`.

A crate named `*-core` MUST NOT depend on CLI, UI, network, filesystem, database, or process-specific infrastructure unless that is its domain.

## 5. Recommended Crate Roles

For medium-sized applications, prefer this split when applicable:

* `*-core`: domain types, pure logic, validation, algorithms.
* `*-io`: filesystem, network, database, or external system integration.
* `*-cli`: command-line parsing, user output, process exit behavior.
* `*-test-support`: shared fixtures, builders, fakes, and test utilities.
* `*-format` or `*-parser`: parsing, serialization, file formats, protocols.
* `*-macros`: proc macros only, when required.

These roles are examples, not mandatory layers.

Core crates SHOULD be dependency-light.

Application crates MAY depend on infrastructure crates.

Infrastructure crates MUST NOT force dependencies into core crates.

## 6. Dependency Direction

Dependencies MUST form an acyclic graph.

Domain logic SHOULD be near the dependency root.

Application entry points SHOULD be near the dependency leaves.

High-level orchestration MAY depend on concrete infrastructure.

Core logic SHOULD depend on abstractions or data, not concrete external systems.

A lower-level crate MUST NOT depend on a higher-level crate.

Circular design pressure SHOULD be resolved by moving shared types to a smaller crate or inverting the dependency.

## 7. Public API Surface

Public APIs MUST be intentional.

Items MUST remain private unless needed by another crate or external user.

Avoid broad `pub use` trees.

Re-exports SHOULD provide one canonical import path.

The same public item SHOULD NOT be reachable through multiple unrelated paths.

Internal modules SHOULD be private.

Use `pub(crate)` for cross-module internals inside one crate.

Use `pub(super)` only when it clarifies local ownership.

Public modules SHOULD be stable, small, and documented.

## 8. Module Layout

A crate's `src/lib.rs` SHOULD define the public module structure.

A binary crate's `src/main.rs` SHOULD be thin.

Complex application logic MUST NOT live in `main.rs`.

Recommended binary layout:

```text
src/
  main.rs
  cli.rs
```

Recommended library layout:

```text
src/
  lib.rs
  error.rs
  config.rs
  domain/
  service/
  io/
```

Use directories only when a module has multiple cohesive submodules.

Avoid `mod.rs` unless the existing project style uses it.

Module names MUST describe domain purpose, not vague technical buckets.

## 9. Binary and Library Split

Reusable logic MUST live in library crates.

Binaries SHOULD only parse input, call library code, handle user output, and map errors to exit behavior.

A crate MAY contain both `src/lib.rs` and `src/main.rs` when the binary is a thin wrapper around the library.

Multiple binaries SHOULD live in `src/bin/` only when they share the same library crate and dependency set.

## 10. Cargo.lock

Application repositories MUST commit `Cargo.lock`.

Pure library repositories MAY commit `Cargo.lock`, but MUST follow the repository convention.

Workspace applications with binaries MUST commit one root `Cargo.lock`.

Agents MUST NOT delete or regenerate `Cargo.lock` unless dependency resolution intentionally changed.

## 11. Rust Edition and MSRV

The Rust edition MUST be explicit.

The MSRV SHOULD be explicit when the project promises one.

All crates in a workspace SHOULD use the same edition.

All crates in a workspace SHOULD share the same MSRV unless a narrower exception is documented.

MSRV changes MUST be intentional.

Agents MUST NOT raise MSRV accidentally by adding dependencies or language features.

## 12. Dependency Policy

Every dependency MUST have a clear purpose.

Prefer existing workspace dependencies before adding new ones.

Prefer standard library for trivial functionality.

New dependencies SHOULD be:

* Maintained.
* Widely used or clearly justified.
* License-compatible.
* Minimal for the need.
* Compatible with MSRV.
* Compatible with target platforms.

Agents MUST NOT add dependencies for small helpers, simple parsing, formatting, or one-off convenience.

Unused dependencies MUST be removed.

Dependency versions SHOULD be specified once in the workspace.

Crates SHOULD opt into workspace dependencies with:

```toml
dependency-name = { workspace = true }
```

Crate-specific features SHOULD be enabled in the consuming crate, not globally, unless all consumers need them.

## 13. Dependency Features

Features MUST be additive.

Features MUST NOT disable behavior.

Features MUST NOT change public API semantics in surprising ways.

Default features SHOULD be minimal.

Optional dependencies MUST be connected to explicit features.

Feature names SHOULD describe capability, not dependency names, unless exposing the dependency is the point.

Feature combinations SHOULD compile.

Agents SHOULD run feature checks when changing features.

## 14. Dev Dependencies

Test-only dependencies MUST be declared as `dev-dependencies`.

Benchmark-only dependencies SHOULD be scoped to benchmarks where practical.

Test helpers used across crates SHOULD live in a dedicated test-support crate.

Production crates MUST NOT depend on test-support crates outside `dev-dependencies`.

Test-support crates MUST NOT leak into public production APIs.

## 15. Build Scripts

Build scripts SHOULD be avoided.

A `build.rs` MUST have a concrete need.

Build scripts MUST be deterministic.

Build scripts MUST NOT depend on undeclared local machine state.

Build scripts MUST print clear rerun directives.

Generated code SHOULD go into `OUT_DIR`.

Generated source committed to the repository MUST be clearly marked.

## 16. Unsafe and FFI Structure

FFI code SHOULD live in a dedicated crate or module.

Unsafe boundary code SHOULD be isolated.

Safe wrappers SHOULD expose validated, idiomatic Rust APIs.

Raw FFI types SHOULD NOT leak into high-level crates unless unavoidable.

FFI crates SHOULD have focused tests for ownership, lifetime, encoding, and error boundaries.

## 17. Macros

Macros SHOULD be avoided unless they clearly reduce real repetition or enforce correctness.

Proc macros MUST live in a dedicated proc-macro crate.

Macro crates SHOULD contain minimal logic.

Complex logic used by macros SHOULD live in a normal library crate and be tested there.

Public macros MUST be documented with examples.

## 18. Tests

Unit tests SHOULD live near the code they test.

Integration tests SHOULD live in top-level `tests/` or crate-local `tests/`.

Large fixtures SHOULD live in `testdata/`.

Fixtures MUST be deterministic.

Fixtures SHOULD be small unless size is the behavior being tested.

Ignored tests MUST state why they are ignored and how to run them.

Tests requiring external services MUST be explicitly marked or feature-gated.

Shared test utilities MUST NOT be copied across crates when a test-support crate is appropriate.

## 19. Examples and Benches

Examples SHOULD live in `examples/`.

Examples MUST compile when they are part of normal checks.

Benchmarks SHOULD live in `benches/`.

Benchmarks MUST NOT be treated as correctness tests.

Performance-sensitive APIs SHOULD have benchmarks when optimization work is done.

## 20. Tooling

The repository SHOULD support these commands:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets
```

When applicable, the repository MAY also support:

```sh
cargo test --workspace --all-features
cargo doc --workspace --no-deps
cargo audit
cargo deny check
cargo hack check --workspace --feature-powerset --no-dev-deps
cargo udeps --workspace --all-targets
cargo miri test
```

Agents SHOULD run the narrowest relevant checks after a change.

Agents MUST report which checks were run.

Agents MUST NOT claim unrun checks passed.

## 21. Lints and Formatting

Formatting MUST be handled by `rustfmt`.

Code MUST NOT fight the formatter.

Clippy warnings SHOULD be fixed, not silenced.

Lint allows MUST be narrow, local, and justified.

Workspace lint policy SHOULD be centralized.

Crate-level lint overrides MUST have a reason.

## 22. Documentation Layout

Repository-level documentation SHOULD explain:

* Project purpose.
* Main crates.
* How to build.
* How to test.
* Important features.
* Important architecture decisions.

Crate-level documentation SHOULD explain crate responsibility and public API entry points.

Architecture docs SHOULD be short and current.

Obsolete docs MUST be updated or removed.

## 23. Agent Workflow

Before changing structure, agents MUST inspect:

* Workspace `Cargo.toml`.
* Target crate `Cargo.toml`.
* Existing module layout.
* Existing dependency direction.
* Existing test layout.

When adding code, agents SHOULD place it where the nearest existing pattern suggests.

When no pattern exists, agents SHOULD prefer the smallest local structure.

Agents MUST NOT introduce new top-level directories, crates, features, or dependencies without a concrete need.

After structural changes, agents SHOULD check:

* Workspace membership.
* Dependency direction.
* Feature declarations.
* Public re-exports.
* Relevant tests.
* Formatting and linting.

Final responses SHOULD state:

* Files changed.
* Structure decisions made.
* Dependencies added or removed.
* Checks run.
* Known risks.