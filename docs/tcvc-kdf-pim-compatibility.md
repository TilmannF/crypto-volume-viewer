# TC/VC KDF and PIM Compatibility

This document describes the supported PBKDF2-HMAC KDF/hash profiles, PIM semantics, KDF autoprobing, and the crypto-matrix test workflow for the `tcvc` backend.

## Supported KDF/Hash Profiles

All profiles use PBKDF2-HMAC with the respective hash function and the AES-XTS encryption cipher. The cipher scope is limited to AES-XTS; no other ciphers are supported.

| Profile | Display name | Rust crate |
|---|---|---|
| SHA-512 | `SHA-512` | `sha2 = "0.11"` |
| SHA-256 | `SHA-256` | `sha2 = "0.11"` |
| Whirlpool | `Whirlpool` | `whirlpool = "0.11"` |
| BLAKE2s-256 | `BLAKE2s-256` | `blake2 = "0.11.0-rc.6"` (see note) |
| Streebog-512 | `Streebog` | `streebog = "0.11"` |

**BLAKE2s-256 implementation note:** The RustCrypto `hmac 0.13` crate requires `EagerHash` trait bounds. BLAKE2s uses `Lazy` buffering and does not implement `EagerHash`. `SimpleHmac<Blake2s256>` (from the same `hmac` crate) is used instead; this is equivalent to standard HMAC-BLAKE2s-256 and does not change the cryptographic output. The `blake2 0.11.0-rc.6` pre-release is required to match the `digest 0.11.x` trait family used by the other crates in the dependency graph.

## Unsupported Profiles

| Profile | Reason |
|---|---|
| Argon2id | Different memory/time cost semantics; separate milestone |
| RIPEMD-160 | The VeraCrypt CLI used in the crypto-matrix workflow cannot create normal file containers with RIPEMD-160 on current builds; no fixture generation possible |

No other cipher modes (Serpent, Twofish, Camellia, Kuznyechik, cascades) are implemented.

## PIM Semantics

PIM (Personal Iterations Multiplier) controls the PBKDF2 iteration count for file-hosted normal containers. The formula is uniform across all supported PBKDF2-HMAC hash algorithms:

| PIM value | Iteration count |
|---|---|
| omitted or `0` | 500,000 (VeraCrypt default) |
| `N > 0` | `15,000 + (N × 1,000)` |

Source: [VeraCrypt PIM documentation](https://veracrypt.io/en/Personal%20Iterations%20Multiplier%20(PIM).html)

The iteration count arithmetic uses checked arithmetic throughout. A PIM value that would overflow `u32` is rejected before any PBKDF2 call is made.

PIM values are not logged, not cached, not shown in error messages, and not stored after a header open attempt completes.

### Default PIM behavior

Omitting `--pim` uses 500,000 iterations. Passing `--pim 0` is equivalent.

```bash
cryptovol test-open container.hc           # default PIM: 500,000 iterations
cryptovol test-open container.hc --pim 0  # same
```

### Custom PIM behavior

Passing a positive integer uses `15,000 + (PIM × 1,000)` iterations. For example, `--pim 500` uses 515,000 iterations.

```bash
cryptovol test-open container.hc --pim 500  # 515,000 iterations
```

A container created with a custom PIM will not open with the default PIM and vice versa. Wrong PIM fails authentication cleanly without revealing which combination failed.

## KDF Autoprobing

When no `--kdf` hint is provided, `open_with_options` tries all supported KDFs in deterministic order against each header candidate (primary first, then backup):

```text
SHA-512 → SHA-256 → Whirlpool → BLAKE2s-256 → Streebog
```

The first KDF/candidate pair that produces a valid decrypted header is used. The matched KDF and PIM state are returned in `TcvcMatchedProfile` and reported in `test-open` output.

### KDF hint

Passing `--kdf <name>` skips autoprobing and tries only the named KDF:

```bash
cryptovol test-open container.hc --kdf sha256
cryptovol test-open container.hc --kdf whirlpool --pim 500
```

Accepted hint values: `sha512`, `sha256`, `whirlpool`, `blake2s`, `streebog`.

`--kdf` is a performance and debugging hint. Containers open correctly without it. Rejection checks in test scripts should use `--kdf` to avoid running all five KDFs during failure probes.

## `test-open` Output

On success, `test-open` reports the matched profile:

```text
TC/VC volume opened successfully.
Backend: tcvc
Header: primary
Encryption: AES-XTS
KDF/Hash: SHA-256
PIM: default
Read-only: yes
```

For a custom PIM:

```text
TC/VC volume opened successfully.
Backend: tcvc
Header: primary
Encryption: AES-XTS
KDF/Hash: SHA-512
PIM: 500
Read-only: yes
```

## Generated Crypto-Matrix Workflow

`scripts/test-with-tcvc-crypto-matrix.sh` creates a temporary set of containers covering all VeraCrypt-supported KDF/PIM combinations and verifies that `cryptovol` opens each one correctly.

Prerequisites: VeraCrypt CLI (resolved by `scripts/ensure-veracrypt-cli.sh`) and a release build of `cryptovol`.

```bash
./scripts/test-with-tcvc-crypto-matrix.sh
```

The script:

* generates one 20 MiB container per (KDF × PIM) combination under `testdata/generated/crypto-matrix/`
* skips profiles VeraCrypt cannot create (e.g. BLAKE2s-256 on some builds)
* verifies correct-password/correct-PIM opens succeed
* verifies wrong-password is rejected
* verifies wrong-PIM is rejected for custom-PIM containers
* uses `expect` to feed passwords through a pty (rpassword reads from `/dev/tty`, not piped stdin)
* uses `--kdf` hints on rejection checks to limit each check to one KDF instead of all five
* exits 0 if all non-skipped profiles pass

Generated containers stay in `testdata/generated/` and are not committed.

## Static Full-Pipeline Crypto-Matrix Fixtures

Three committed static fixtures exercise the full open → decrypt → FAT/LFN/extract pipeline across different KDF and PIM combinations.

### Baseline: `testdata/static/tcvc-aes-sha512-fat-lfn-unicode.hc`

```text
KDF/Hash: SHA-512
PIM:      default / 0
Password: test-password
```

Covered by LFN fixture tests (see [test-containers.md](test-containers.md)).

### `testdata/static/crypto-matrix/tcvc-aes-sha256-pim-default-fat-lfn-unicode.hc`

```text
KDF/Hash: SHA-256
PIM:      default / 0
Password: test-password
Contents: same file tree as testdata/static/fs-fat-lfn-original/
```

### `testdata/static/crypto-matrix/tcvc-aes-sha512-pim-500-fat-lfn-unicode.hc`

```text
KDF/Hash: SHA-512
PIM:      500
Password: test-password
Contents: same file tree as testdata/static/fs-fat-lfn-original/
```

### Running static fixture tests

```bash
CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR=$(pwd)/testdata/static/crypto-matrix \
  cargo test -- --ignored
```

The path must be absolute (or resolved with `$(pwd)`). Relative paths cause the fixture lookup to silently skip. The tests use `testdata/static/fs-fat-lfn-original/` as ground truth and verify extracted file SHA-256 hashes.

Normal `cargo test` skips all `#[ignore]` tests and does not require any fixture files.

## Future Scope

The following are deferred to later milestones:

* Keyfiles: additional key material input; adds complexity to the open path
* Hidden volumes: plausible-deniability containers; requires careful design to avoid violating deniability
* Argon2id: different memory/time cost KDF; separate iteration-count semantics
* Non-AES ciphers: Serpent, Twofish, Camellia, Kuznyechik, and cascades
