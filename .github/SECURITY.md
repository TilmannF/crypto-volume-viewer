# Security Policy

Crypto Volume Viewer is a local, read-only tool. It does not phone home. Report issues that could leak passwords, key material, or decrypted contents, or that could write to a source container.

## Reporting a vulnerability

**Email [cryptovol@flgnr.com](mailto:cryptovol@flgnr.com).** Do not file a public GitHub issue for security-sensitive findings.

Include:

- What you ran (CLI or GUI, version or commit)
- What you expected, what happened
- Whether a password or decrypted bytes appeared in logs, errors, or crash output

You should get a reply. There is no bug bounty.

## What this project will not do

No password recovery, brute force, wordlists, hidden-volume detection that violates plausible deniability, or exploit logic. Requests for those are not security reports.

## Encryption

The app implements its own volume decryption (AES-XTS and several KDFs) using maintained Rust crypto crates. That is not HTTPS-only encryption. See [docs/security.md](../docs/security.md).
