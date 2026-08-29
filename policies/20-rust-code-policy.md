# Rust Code Policy

This policy defines Rust implementation rules for API design, errors, ownership, safety, performance, tests, and documentation.

General engineering rules are defined in `00-engineering-policy.md`. Project structure rules are defined in `10-rust-project-structure-policy.md`. When rules conflict, the more specific policy wins.

## 1. Goals

Rust code MUST optimize for:

1. Soundness.
2. Correctness.
3. Explicitness.
4. Idiomatic Rust.
5. Small APIs.
6. Clear ownership.
7. Useful errors.
8. Testability.
9. Measured performance.

Code MUST NOT imitate object-oriented, dynamic, or exception-based designs when Rust has a clearer native pattern.

## 2. Idiomatic Rust

Code SHOULD use Rust's type system to make invalid states unrepresentable.

Prefer:

* Enums over stringly-typed states.
* Newtypes over raw primitives for domain values.
* Pattern matching over flag-heavy branching.
* Traits over inheritance-style abstractions.
* Composition over deep object graphs.
* Explicit data flow over hidden global access.
* `Result` over exceptions.
* `Option` over nullable sentinels.
* Iterators over manual indexing when clearer.

Code MUST NOT use unsafe, global mutable state, unchecked indexing, or panics to bypass normal Rust design.

## 3. Public APIs

Public APIs MUST be intentional, minimal, and documented.

Public APIs SHOULD be easy to call correctly and hard to call incorrectly.

Public types SHOULD implement `Debug`.

Public value types SHOULD implement `Clone`, `Eq`, `PartialEq`, `Ord`, `PartialOrd`, or `Hash` only when semantics are obvious and useful.

Public types crossing thread boundaries SHOULD be `Send` and `Sync` when practical.

Public functions SHOULD accept borrowed input unless ownership is required.

Prefer flexible input bounds when they improve ergonomics without hiding cost:

* Use `impl AsRef<Path>` for path input.
* Use `impl AsRef<str>` only when accepting owned and borrowed strings is truly useful.
* Use `impl IntoIterator` for collection input.
* Use `impl RangeBounds<T>` for ranges.
* Use `impl Read`, `impl Write`, or project traits for I/O boundaries.

Return concrete types by default.

Return `impl Trait` when it hides irrelevant implementation details.

Use trait objects only when dynamic dispatch is required.

Public APIs MUST NOT expose internal helper types, raw dependency types, or implementation details unless they are part of the contract.

## 4. Type Design

Types MUST encode domain meaning.

Use structs when fields form one concept.

Use tuple structs for simple newtypes.

Use enums for closed sets of alternatives.

Use `#[non_exhaustive]` only when external users need forward compatibility.

Use `PhantomData` only when it encodes a real ownership, lifetime, or type-state invariant.

Boolean parameters SHOULD be avoided in public APIs when an enum communicates intent better.

Primitive obsession SHOULD be avoided for IDs, offsets, sizes, modes, permissions, states, and validated values.

Validated values SHOULD be represented by dedicated types after validation.

Constructors MUST enforce invariants.

Invalid values MUST NOT be representable after construction unless the type is explicitly raw or unchecked.

## 5. Builders and Configuration

Use builders for complex construction.

Builders SHOULD be used when a type has many optional fields, cross-field validation, or future-compatible configuration.

Builders MUST validate in `build()`.

Builders MUST NOT allow invalid final values.

Simple required data SHOULD use constructors or struct literals instead of builders.

Configuration types SHOULD be explicit, serializable when useful, and validated before use.

Defaults MUST be safe.

## 6. Ownership and Borrowing

Borrow when ownership is not needed.

Take ownership when storing, transforming, or consuming a value.

Avoid cloning to satisfy the borrow checker before checking whether ownership can be structured better.

`clone()` MUST be intentional.

Hot-path clones SHOULD be avoided unless measured as irrelevant.

Use `Cow` only when it clearly reduces allocation or improves API ergonomics.

Use lifetimes only when they express real borrowing relationships.

Do not add lifetime parameters to avoid simple owned values when owned values are clearer.

Self-referential structs MUST be avoided unless using a proven abstraction.

## 7. Strings, Paths, and Bytes

Use `String` for owned UTF-8 text.

Use `&str` for borrowed UTF-8 text.

Use `PathBuf` for owned paths.

Use `&Path` for borrowed paths.

Use `OsString` and `OsStr` for platform strings.

Use `Vec<u8>` for owned bytes.

Use `&[u8]` for borrowed bytes.

Code MUST NOT assume filesystem paths are valid UTF-8.

Binary parsers MUST operate on bytes, not strings.

Text parsing MUST state or validate encoding assumptions.

## 8. Collections

Choose collections by access pattern.

Prefer:

* `Vec<T>` for ordered dense data.
* `HashMap<K, V>` for key lookup.
* `BTreeMap<K, V>` for stable ordering or range queries.
* `HashSet<T>` or `BTreeSet<T>` for uniqueness.
* `VecDeque<T>` for queue behavior.

Collections SHOULD be preallocated when size is known or hot-path growth matters.

Do not use maps for fixed small sets when an enum or array is clearer.

Do not expose mutable collections when narrower methods preserve invariants.

## 9. Error Handling

Recoverable failures MUST use `Result`.

Optional absence MUST use `Option`.

Errors MUST preserve context useful to the caller.

Library crates MUST expose typed errors.

Application crates MAY use dynamic error types at boundaries.

Prefer `thiserror` for library error enums when already available or justified.

Prefer `anyhow` or similar only in application-level orchestration, CLI, tests, examples, or prototypes.

Library public APIs SHOULD NOT expose `anyhow::Error`.

Errors from external dependencies SHOULD be wrapped or translated at crate boundaries.

Use `From` conversions when error propagation is common and lossless.

Use `map_err` when adding context or changing semantics.

Do not stringify errors early.

Do not discard source errors unless hiding sensitive information or simplifying intentional public API.

Error variants SHOULD name the failed condition.

Error messages SHOULD include relevant values, paths, offsets, states, or operation names.

## 10. Panic Policy

Panics MUST NOT be used for normal error handling.

Panics MAY be used for impossible states, violated internal invariants, programmer errors, and tests.

Public functions that may panic MUST document panic conditions.

Intentional `expect()` messages MUST explain the invariant being relied on.

`unwrap()` MUST NOT be used in production code unless the impossibility is local and obvious.

Prefer `expect("...")` over `unwrap()` when failure would indicate a bug.

Indexing with `[]` MUST be avoided when input may be invalid.

Use checked access for external, parsed, or user-controlled data.

## 11. Unsafe Policy

Safe Rust is required by default.

`unsafe` MUST have a concrete reason.

Allowed reasons include:

* FFI boundary.
* Proven performance hot path.
* Implementing a safe abstraction impossible in safe Rust.
* Required interaction with platform or hardware APIs.

Every unsafe block MUST have a nearby `SAFETY:` comment explaining why it is sound.

Unsafe code MUST minimize scope.

Unsafe invariants MUST be documented.

Safe APIs built on unsafe code MUST be sound for all safe callers.

Unsound safe abstractions MUST NOT exist.

Unsafe code SHOULD have tests, fuzzing, Miri coverage, or other validation appropriate to risk.

Agents MUST NOT add unsafe code unless explicitly required.

## 12. Concurrency

Shared mutable state SHOULD be minimized.

Prefer ownership transfer over shared mutation.

Use channels for ownership transfer when appropriate.

Use `Arc` only when shared ownership is required.

Use `Mutex` or `RwLock` only when mutation must be shared.

Lock scope MUST be minimal.

Do not hold locks across blocking I/O, `.await`, callbacks, or user-provided code.

Thread shutdown MUST be explicit when threads outlive the call scope.

Concurrent code MUST document important ordering, cancellation, and ownership assumptions.

Atomics MUST specify correct ordering.

Use `Ordering::SeqCst` until a weaker ordering is justified.

## 13. Async Rust

Async code MUST be cancellation-aware when cancellation can happen.

Do not block executor threads.

Blocking work in async code MUST use an appropriate blocking mechanism.

Do not hold locks across `.await`.

Do not use `async_trait` unless native async traits are insufficient or object safety is required.

Public async APIs SHOULD return `Result` with typed errors when failures are expected.

Long-running async tasks SHOULD include cooperative yield points where practical.

Spawned tasks MUST have clear ownership, error handling, and shutdown behavior.

Detached tasks SHOULD be avoided.

## 14. I/O and External Effects

Core logic SHOULD be pure or effect-light.

Filesystem, network, clock, randomness, process, environment, and OS access SHOULD be isolated.

I/O functions SHOULD accept traits or narrow abstractions when testability matters.

Production code MUST NOT perform hidden network, filesystem, process, or environment access from deep domain logic.

External input MUST be treated as untrusted.

Parsing MUST validate boundaries, sizes, encodings, versions, and checksums when applicable.

## 15. Parsing and Serialization

Parsers MUST reject invalid input explicitly.

Parsers MUST NOT panic on malformed input.

Parsers MUST check lengths before slicing.

Parsers MUST handle truncation, overflow, invalid tags, unsupported versions, and trailing data according to the format contract.

Use checked arithmetic for offsets, sizes, capacities, and untrusted numeric input.

Serialization MUST be deterministic when output stability matters.

Binary formats SHOULD use explicit endianness.

Text formats SHOULD define encoding and escaping behavior.

## 16. Numeric Safety

Arithmetic on untrusted sizes, offsets, indexes, counts, timestamps, and capacities MUST be checked.

Use `checked_*`, `saturating_*`, or `wrapping_*` intentionally.

Casts using `as` SHOULD be avoided when truncation, sign change, or precision loss is possible.

Prefer `TryFrom` for fallible numeric conversion.

Integer overflow MUST NOT be relied on except in explicitly wrapping algorithms.

Floating-point comparisons MUST account for precision when exact equality is not guaranteed.

## 17. Traits

Traits SHOULD express behavior, not object hierarchies.

Traits SHOULD be small.

Traits SHOULD have clear contracts.

Trait methods SHOULD avoid requiring allocation unless necessary.

Use associated types when one implementation has one natural output type.

Use generics when static dispatch and monomorphization are acceptable.

Use trait objects when heterogeneous runtime dispatch is required.

Blanket impls MUST be careful not to block downstream implementations.

Sealed traits MAY be used to prevent external implementations when invariants require it.

## 18. Generics

Generics SHOULD improve correctness, reuse, or API flexibility.

Generics MUST NOT obscure simple code.

Trait bounds SHOULD be as narrow as possible.

Prefer bounds on functions over bounds on entire impl blocks when only some methods need them.

Avoid exposing complex generic types in public APIs unless the complexity buys real value.

Use type aliases only when they improve readability.

## 19. Macros

Macros SHOULD be avoided by default.

Use macros when they remove real repetition, enforce consistency, or provide domain-specific syntax.

Macros MUST NOT hide control flow, ownership, errors, or unsafe behavior.

Public macros MUST be documented with examples.

Procedural macro logic SHOULD be tested outside the macro when possible.

## 20. Performance

Correct code comes first.

Optimize only measured or obvious hot paths.

Performance work SHOULD include benchmarks or profiling when practical.

Avoid unnecessary:

* Allocation.
* Cloning.
* String formatting.
* Dynamic dispatch.
* Locking.
* Syscalls.
* Parsing.
* Collection resizing.

Use references, slices, iterators, preallocation, and batching when they improve clear hot-path code.

Zero-copy designs MAY be used when they do not create brittle lifetimes or unsafe complexity.

Caching MUST define key, invalidation, memory growth, and concurrency behavior.

Micro-optimizations MUST NOT reduce clarity without evidence.

## 21. Logging and Observability

Libraries SHOULD use structured logging or tracing only when the project uses it.

Libraries MUST NOT print to stdout or stderr except for explicitly user-facing APIs.

CLI crates MAY print user output.

Logs MUST NOT contain secrets or sensitive data.

Error logs SHOULD include operation and context.

High-frequency logs SHOULD be avoided in hot paths.

## 22. Documentation

Public modules SHOULD have `//!` docs.

Public types and functions SHOULD have doc comments.

Docs MUST describe behavior, not implementation trivia.

Docs for fallible functions SHOULD include `# Errors` when useful.

Docs for panicking functions MUST include `# Panics`.

Docs for unsafe functions MUST include `# Safety`.

Examples SHOULD be small and correct.

Doc examples SHOULD compile unless marked `ignore` for a stated reason.

Docs MUST NOT include obsolete design history, agent reasoning, or speculative plans.

## 23. Testing

Tests MUST verify observable behavior.

Tests MUST NOT merely duplicate implementation logic.

Unit tests SHOULD cover local logic.

Integration tests SHOULD cover public behavior across module or crate boundaries.

Regression tests SHOULD be added for fixed bugs.

Property tests SHOULD be used for parsers, encoders, state machines, and invariants when useful.

Fuzz tests SHOULD be considered for untrusted binary or text parsers.

Tests MUST be deterministic.

Tests MUST NOT require network, wall clock timing, local machine state, or test order unless explicitly marked.

Use `#[should_panic]` only for panic contracts.

Test names SHOULD state the behavior under test.

## 24. CLI and Application Code

CLI code SHOULD be thin.

Argument parsing, user output, and exit mapping belong near the CLI boundary.

Business logic MUST live outside `main.rs`.

CLI errors SHOULD be human-readable.

Machine-readable output MUST be stable when documented.

Exit codes SHOULD be intentional.

Applications SHOULD use typed internal errors and may convert to user-facing errors at the boundary.

## 25. FFI

FFI boundaries MUST be isolated.

Raw pointers, foreign handles, and C strings MUST NOT leak into high-level APIs unless unavoidable.

Ownership transfer across FFI MUST be explicit.

FFI wrappers MUST define lifetime, thread-safety, encoding, nullability, and error behavior.

Callbacks from foreign code MUST not violate Rust aliasing or panic-safety rules.

Panics MUST NOT unwind across FFI boundaries.

## 26. Agent Rules

Before editing Rust code, agents MUST inspect nearby code for:

* Error style.
* Ownership style.
* Public API conventions.
* Test style.
* Feature gates.
* Logging style.
* Existing helper types.

Agents MUST preserve idioms already established unless they violate policy.

Agents MUST prefer small local changes over broad rewrites.

Agents MUST remove obsolete code after replacing behavior.

Agents MUST update tests when behavior changes.

Agents MUST update docs when public behavior changes.

Agents SHOULD run the narrowest relevant checks.

Final responses SHOULD state:

* What changed.
* Why the Rust design is sound.
* Which checks ran.
* Which checks did not run.
* Remaining risks.
