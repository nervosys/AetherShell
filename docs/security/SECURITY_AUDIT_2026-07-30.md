# Security Audit — 2026-07-30

Scope: the `master` tree at `2d8b969`, covering the `aethershell`,
`aethershell-lsp` and `agentic-eval` crates, the browser extension, and the CI
and release workflows.

Method: `cargo audit` (1173 RUSTSEC advisories), `cargo deny check` (advisories,
bans, licenses, sources), targeted source review of the cryptographic, policy
and audit surfaces, and a repository-wide scan for credentials and personal
data. Findings were fixed in this pass unless recorded otherwise below.

Frameworks requested and addressed: CVE/RUSTSEC, MITRE ATT&CK, NIST FIPS, and
CMMC 2.0.

---

## Summary

| # | Finding | Class | Severity | Status |
|---|---------|-------|----------|--------|
| 1 | `quinn-proto` remote memory exhaustion | RUSTSEC-2026-0185 | High (7.5) | Fixed |
| 2 | Unimplemented crypto builtins reported success | CWE-347 / CWE-311 | High | Fixed |
| 3 | Committed WASM leaked a developer's username | CWE-532 | Low | Fixed (tree only) |
| 4 | `rbac.*`, `perm.acl_*`, `sso.*` advertised but unimplemented | CWE-1104 | Medium | Documented, not implemented |
| 5 | Builtin registry split hides names from agent discovery | Correctness | Low | Documented |

Nothing in this audit indicates a compromise or data exposure to a third party.
Finding 3 concerns one developer's username, published in a public repository.

---

## 1. CVE / advisory posture — RUSTSEC-2026-0185

`cargo audit` reported one vulnerability: unbounded out-of-order stream
reassembly in `quinn-proto` 0.11.14 permitting remote memory exhaustion
(CVSS 7.5). Updated to 0.11.16.

Exploitability in AetherShell was nil rather than merely low. `quinn` appears in
`Cargo.lock` only as an optional dependency of `reqwest`'s `http3` feature,
which is not enabled; `cargo tree -i quinn-proto --target all --all-features`
reports "nothing to print", i.e. it is never compiled into any shipped artifact.
It was updated regardless so that a clean lockfile cannot mask a future real
exposure.

`cargo audit` now exits 0. Ten findings remain as accepted warnings
(`derivative`, `instant`, `number_prefix`, `paste`, `proc-macro-error`
unmaintained; `anyhow`, `lru`, `memmap2`, `rand` ×2 unsound). These are
transitive, previously reviewed, and allowlisted. `cargo deny check` passes all
four gates.

## 2. Unimplemented crypto builtins reported success (CWE-347, CWE-311)

The most serious finding, and not dependency-related.

`eval::is_truthy` maps `Value::Str(s)` to `!s.is_empty()` and `Value::Error` to
`false`. Seven cryptographic builtins were stubs that returned an explanatory
*string*, which is therefore truthy and indistinguishable from success:

```
if crypto.verify_signature(sig, data, key) { deploy() }   # branch always taken
```

A script gating on signature or certificate verification proceeded with nothing
verified. `crypto.encrypt` expressed the same defect differently: it returned
the sentence `"Encryption requires OpenSSL"` where the caller expects
ciphertext, so persisting the result stored prose in place of an encrypted
secret, silently.

This was not limited to unusual configurations. `encrypt`, `decrypt` and
`verify_cert` shell out to the `openssl` CLI under `#[cfg(unix)]` only, so on
Windows the stub was the *only* path; on Unix, any `openssl` failure fell
through to it.

All seven now return `Err` with the `E_UNIMPLEMENTED` code, matching the
existing `E_*` convention. `tests/crypto_fail_closed.rs` asserts the property
that matters — that these never yield a truthy value — rather than exact
strings, so a genuine implementation satisfies the test unchanged.

## 3. Committed build artifact leaked a developer's username (CWE-532)

`integrations/browser-extension/wasm/` held committed `wasm-bindgen` output
embedding absolute build paths, and hence the building developer's username, in
panic-location strings shipped to anyone loading the extension:

```
C:\Users\<user>\.cargo\registry\src\...\serde_json-1.0.149\src\error.rs
C:\Users\<user>\.rustup\toolchains\stable-x86_64-pc-windows-msvc\...
```

These are build artifacts: `build.ps1` copies them out of `web/pkg` (correctly
untracked) and `build.ps1 -Clean` deletes the directory wholesale. The extension
README already lists building the WASM as installation step 1 and labels the
directory "(generated)", so removing them breaks nothing that was documented to
work. Removed and gitignored.

`build.ps1` now also passes `--remap-path-prefix` via `CARGO_ENCODED_RUSTFLAGS`
so a future local build cannot reintroduce the leak. Cargo's `[profile]
trim-paths` would be tidier but is still unstable as of Cargo 1.97.1. Verified
empirically on a scratch crate with a registry dependency: three
username-bearing paths before, zero after.

**Incomplete.** This fixes the working tree and future builds. The artifact
remains in git history and is still fetchable from prior commits. Scrubbing it
requires a history rewrite and force-push, which was not performed.

`.mailmap` also maps a personal address to the organisation address. That is
what a mailmap is for, and both addresses are already present in commit
metadata, so it exposes nothing new — noted for completeness. It is excluded
from the published crate.

No hardcoded credentials were found. Matches for `sk-`, `ghp_`, `AKIA`, PEM
blocks and JWTs are all fixtures belonging to the redaction machinery itself,
whose behaviour is covered by `tests/secret_hygiene.rs` (4/4 passing): secret
shapes and secret-named fields are scrubbed from agent output, and secrets never
reach the hash-chained audit log.

## 4. Advertised access-control surface is unimplemented (CWE-1104)

`modules.rs` maps user-facing names to builtin names, and nothing verified the
targets existed. 80 of 919 aliases pointed at builtins that do not exist. Nine
were naming drift and were corrected (`crypto.verify`, `crypto.key_generate`,
six `gui.window_*`, `input.multi_select`).

The remaining 71 are genuinely unimplemented and are now pinned in
`tests/module_aliases.rs` as an allowlist that can only shrink. Among them:

```
rbac_check  rbac_permissions  rbac_revoke  rbac_roles  rbac_user_roles
rbac_create_role  rbac_delete_role
perm_acl_get  perm_acl_set  perm_owner_get  perm_owner_set
sso_providers  sso_refresh  sso_user_info
```

**AetherShell advertises a role-based access control, ACL and SSO surface that
it does not implement.** Calling them yields `unknown builtin`, so the failure
is *closed* and this is not itself a vulnerability — but any deployment that
believes it is enforcing authorization through `rbac.check` is enforcing
nothing. This was documented rather than implemented; implementing it is a
product decision, not an audit fix.

The access controls that *do* exist and are exercised by tests are the policy
gates: `E_POLICY_DENY`, `E_NEEDS_APPROVAL`, `E_EGRESS_DENIED`,
`E_OUTSIDE_WORKSPACE`, `E_BUDGET_EXCEEDED` (`tests/safety.rs` 13/13,
`tests/security_user_simulation.rs` 16/16).

## 5. Builtin registry split

Dispatch has two layers: `BUILTIN_LOOKUP` (name → index → function-pointer
table) and a trailing fallback `match`. Only the first is public, so `agent`,
`ai` and `swarm` dispatch correctly yet are invisible to anything enumerating
`BUILTIN_LOOKUP` — including `agent_api`'s dynamic discovery, whose purpose is
telling an agent what it may call. Documented in `tests/module_aliases.rs`;
unifying the two is deferred.

---

## NIST FIPS

`AETHER_FIPS` gates non-approved hash algorithms. `require_fips_hash` rejects
MD5 and SHA-1 with `E_FIPS_DISALLOWED`; the SHA-2 family passes. All four
weak-hash entry points are gated, and the two remaining MD5 call sites are
deliberate and correctly conditioned:

- `marketplace.rs` accepts a 32-hex legacy MD5 package checksum only when
  `!fips_enabled()`, preferring SHA-256 (64-hex) — documented in place.
- `persistence.rs` likewise skips MD5 comparison under FIPS.

The existing doc comment already draws the distinction that matters, and it is
worth repeating: this enforces *approved-algorithm-only* at the application
layer. It does **not** make the underlying crypto a FIPS-140-validated module.
Any claim of FIPS 140-2/140-3 validation would be unsupported.

Separately, package integrity in `marketplace.rs` rests on a checksum served by
the same registry that serves the payload, so it detects corruption, not a
malicious or compromised registry — independent of hash strength. Signature
verification against a pinned key would be required for that, and
`crypto.verify_signature` is (per finding 2) unimplemented.

## MITRE ATT&CK

| Technique | Relevance | Control |
|---|---|---|
| T1059 Command and Scripting Interpreter | AetherShell *is* an interpreter; 668 `Command::new` sites in `builtins.rs` | Effect classification (`safety::effect_of`), policy gates, approval prompts, hash-chained audit log |
| T1195 Supply Chain Compromise | Agent packages fetched from a registry | Checksum verified; **no signature verification** (findings 2, 4) — residual risk |
| T1552 Unsecured Credentials | Secrets in env/output/logs | Redaction of secret shapes and names; env reads gated in agent mode; secrets excluded from the audit log (`tests/secret_hygiene.rs`) |
| T1027 Obfuscated Files or Information | Homograph/invisible-character path spoofing | `is_deceptive_char` rejects invisible and bidi characters in `validate_safe_path` (CWE-1007) |
| T1005 Data from Local System | Filesystem access by an agent | `E_OUTSIDE_WORKSPACE` workspace confinement |
| T1071 Application Layer Protocol | Network egress by an agent | `E_EGRESS_DENIED` egress policy |

## CMMC 2.0

Assessed as a software component, not an accredited enclave; CMMC applies to
organisations, so this maps practice families to implemented controls only.

| Family | Implemented | Gap |
|---|---|---|
| AU (Audit & Accountability) | Hash-chained audit log, verifiable, secrets redacted before persistence | `audit_stream`, `audit_retention` unimplemented — no retention enforcement |
| AC (Access Control) | Workspace confinement, egress policy, approval gates, budget limits | `rbac.*` and `perm.acl_*` unimplemented (finding 4) |
| IA (Identification & Authentication) | Keyring-backed credential storage | `sso.*` unimplemented |
| SC (System & Communications Protection) | TLS via rustls by default | `crypto.encrypt`/`sign` unimplemented; now fail closed rather than silently (finding 2) |
| SI (System & Information Integrity) | `cargo audit`/`cargo deny` in CI; advisories clean | Package integrity lacks signature verification |

---

## Limitations

- Static and test-based review; no fuzzing, no dynamic analysis, no penetration
  testing, no formal threat model.
- The 71 unimplemented aliases were enumerated, not individually reviewed for
  what a caller might assume of them.
- FIPS assessment covers algorithm restriction only. No validated cryptographic
  module is present or claimed.
- Finding 3's history exposure is unresolved by design; it needs an explicit
  decision to rewrite history.
- Verification ran on Windows with the 1.97 toolchain. The Unix-only `openssl`
  paths in the crypto builtins were reviewed by reading, not executed.
