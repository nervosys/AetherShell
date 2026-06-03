# Cryptographic Posture & FIPS 140-3 Path

Status of cryptography in AetherShell and the path to FIPS-validated deployments.
Companion to the security audit (CVE / NIST FIPS / MITRE ATT&CK / CMMC 2.0).

## Algorithm inventory

| Use | Algorithm | FIPS-approved? | Where |
|---|---|---|---|
| Plan tokens (`apl_<…>`) | SHA-256 | ✅ yes | `builtins.rs` |
| Checkpoint / state integrity | **SHA-256** (was MD5) | ✅ yes | `persistence.rs` |
| Package download verification | **SHA-256**, legacy MD5 read-only | ✅ (new) | `marketplace.rs` |
| Auth token hashing | SHA-256 | ✅ yes | `auth.rs` |
| File-hash builtin (user-selected) | SHA-256/384/512, **SHA-1/MD5 optional** | partial | `builtins.rs` |
| Non-crypto fingerprint | `md5_simple` (labeled non-cryptographic) | n/a | `builtins.rs` |
| TLS | rustls (ring backend) | not validated | `reqwest` |
| RNG | `getrandom` (OS CSPRNG) + `rand` ChaCha | not validated | various |
| Secret storage | OS keychain via `keyring` | OS-dependent | `secure_config.rs` |

## Hardening completed in this audit

- **MD5 → SHA-256 for all integrity checks.** MD5 is collision-broken, so using it
  as an integrity guard is forgeable (an attacker can craft a colliding checkpoint
  or package). Checkpoint integrity (`persistence.rs`) and package-download
  verification (`marketplace.rs`) now compute SHA-256. Legacy MD5 digests are still
  *read* (selected by digest length) so existing on-disk state and registry packages
  keep validating and re-save/re-publish forward to SHA-256. The `md5` crate is
  retained **only** for reading legacy digests, never for new writes.
- **0 dependency CVEs** (`cargo audit`) after patch bumps; the TLS stack
  (`rustls-webpki`) cert-path-validation flaws are remediated.

## FIPS 140-3 status — NOT validated (algorithms approved, modules not)

AetherShell uses FIPS-**approved algorithms** (SHA-2 family) but **not
FIPS-140-validated cryptographic modules**. The RustCrypto (`sha2`), `ring`, and
`rand` implementations are not FIPS-validated. SHA-1 and MD5 remain available as
*user-selectable* options in the general file-hash builtin (parity with
`sha1sum`/`md5sum`); they are not used for any security decision.

**This is acceptable for general use but does not meet FIPS-required deployments**
(e.g. CMMC L2 SC.L2-3.13.11, FedRAMP), which require a validated module.

## Path to a FIPS-validated build (deployment decision)

A regulated build is a build-system/toolchain change, intentionally not enabled by
default (it requires a FIPS-validated C toolchain and changes CI):

1. **TLS:** switch the rustls backend to **`aws-lc-rs` with the `fips` feature**
   (a FIPS 140-3 validated module) — via `reqwest`/`rustls` feature selection,
   ideally behind a `fips` cargo feature.
2. **DRBG:** ensure randomness flows from the validated module's DRBG for any
   security-relevant key/nonce generation.
3. **Disallow legacy algorithms:** remove SHA-1 and MD5 from the security path
   entirely (the file-hash *utility* options may stay, clearly labeled non-FIPS).
4. **Document the validation boundary** and the module certificate in the
   deployment's System Security Plan (SSP).

Until (1)–(4) are completed and the toolchain is validated, builds must be
documented as **FIPS-pending / not-validated**.

## Control mapping (summary)

| Framework | Status |
|---|---|
| **CVE** | ✅ 0 vulnerabilities (`cargo audit`); supply-chain gate (`cargo deny`) repaired |
| **NIST FIPS** | ⚠️ approved algorithms; modules not validated — see path above |
| **MITRE ATT&CK** | mitigations: effect gating, redaction, hash-chained audit, governors, plugin gate (T1129/T1574), egress allowlist (T1041) |
| **CMMC 2.0** | AC/AU/IA/SI/SR covered; SC FIPS-crypto (3.13.11) pending the build above |
