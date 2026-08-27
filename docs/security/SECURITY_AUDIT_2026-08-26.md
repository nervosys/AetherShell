# Security audit — 2026-08-26

White-box review of the tree at `77c7847`, performed by reading code and running
it. Frameworks: CWE, MITRE ATT&CK, NIST FIPS, CMMC 2.0 (NIST SP 800-171 Rev 2).

**What this is not.** Not a certification, not a penetration test, and not a CVE
assignment — identifiers are issued by a CNA and none is claimed here. Dependency
advisory IDs are real (RUSTSEC); first-party findings carry local IDs. CMMC
mappings are a self-assessment, and where a mapping is approximate it says so.

**Evidence grades.** *Demonstrated* — executed and observed. *Structural* —
established by reading the code path, not executed. *Measured* — quantified.

Seven findings are open. Eleven others were found and fixed earlier the same day
and are recorded in `docs/HANDOFF.md`; each is pinned by a ratchet.

---

## AS-2026-01 — the `approve` control does not constrain the agent it governs

**High · Demonstrated · CWE-863, CWE-862 · ATT&CK T1548 · CMMC AC.L2-3.1.7, AC.L2-3.1.5**

§5.3's policy table grades `Destructive` and `Exec` as **approve** in agent mode.
In practice the denied caller receives the approval token in the error, and
`approve(token)` is itself a builtin: dispatch index 1104, classified `Pure`,
therefore not centrally enforced, and `bi_approve` calls `grant_approval`
unconditionally.

`tests/approval_self_grant.rs` runs the loop end to end: the agent is denied,
reads the token out of its own error, calls `approve(token)`, and the refused
call then succeeds.

This is **documented as the intended loop** (§7.2), so the defect is not a
mismatch between code and design — it is that the guarantee implied by the policy
table is narrower than it reads:

* against an agent that **errs**, this is a real control — the dangerous call
  does not happen by default, a deliberate second step is required, and both
  steps are audited;
* against an agent that is **adversarial or prompt-injected**, the `approve` cell
  is equivalent to `allow`, because the attacker drives both steps.

For a shell whose stated purpose is to be driven by language models, prompt
injection is the defining threat, so the distinction is not academic.

The content-binding property §7.2 claims **does** hold and is worth keeping: a
token is a hash of one action descriptor and cannot be replayed against a
different call. That is asserted independently in the same file.

**Recommendation.** Decide and document which threat model is in scope. If the
adversarial one is, approval must arrive out-of-band — the existing
`AETHER_APPROVE` environment path, an A2UI prompt, or a separately authenticated
endpoint — and in-band `approve()` should be refused in agent mode.

---

## AS-2026-02 — the audit log lives inside the jail it audits, and its chain is unkeyed

**High · Structural · CWE-345, CWE-732 · ATT&CK T1070 · CMMC AU.L2-3.3.8, AU.L2-3.3.9**

In agent mode `audit_path()` defaults to `<workspace>/.ae/audit.log` — inside the
workspace jail, which is precisely the region the agent may write. Nothing
special-cases the path, so an ordinary `file.write` to it is allowed.

The tamper-evidence is a SHA-256 hash chain with **no key** (`sha256_hex` takes
only the entry text). A chain anyone can recompute is evidence against
corruption, not against an author. The audited party can truncate the log, or
rewrite it end-to-end with a fresh, internally consistent chain that
`audit_verify()` accepts.

**Recommendation.** Either fix closes it: default the log *outside* the workspace
root, at which point the jail itself refuses the agent's writes to it; and/or key
the chain (HMAC) with material the agent cannot read. The first is cheap and
reachable today by setting `AETHER_AUDIT_LOG` outside the workspace.

---

## AS-2026-03 — secrets are passed to helper processes on the command line

**Medium · Structural · CWE-214 · ATT&CK T1552 · CMMC IA.L2-3.5.10 (approximate), SC.L2-3.13.16**

Three sites place caller secrets in an argument vector, readable by any process
that can enumerate command lines — on Linux `/proc/<pid>/cmdline` is
world-readable by default:

| builtin | invocation |
|---|---|
| `crypto_encrypt` | `openssl enc … -pass pass:<password>` |
| `crypto_decrypt` | same form |
| `crypto_password_hash` | `openssl passwd -6 <password>` |

The Windows `crypto_hmac` path interpolates the key into a PowerShell command
line and is the same exposure class, though correctly quoted against injection.

**Recommendation.** `openssl` supports `-pass stdin`, `-pass fd:N` and
`-pass env:VAR`; the first two avoid both argv and the environment. These sites
already write data over a pipe, so the plumbing exists.

---

## AS-2026-04 — encryption is unauthenticated

**Medium · Structural · CWE-353, CWE-327 · ATT&CK T1565 · CMMC SC.L2-3.13.16**

`crypto_encrypt` uses `openssl enc -aes-256-cbc -pbkdf2`. Cipher and KDF are both
FIPS-approved and `-pbkdf2` is correctly present — without it OpenSSL's legacy
`EVP_BytesToKey` would derive keys with MD5. What is missing is **integrity**:
CBC is malleable, so ciphertext can be altered and `crypto_decrypt` cannot detect
it.

**Recommendation.** Move to an AEAD (`-aes-256-gcm`), or encrypt-then-MAC with the
existing HMAC helper. The API is a round trip within this tool, so a format
change is contained.

---

## AS-2026-05 — `crypto_uuid`'s fallback is a clock wearing a v4 label

**Medium · Measured · CWE-330, CWE-340 · SP 800-90A · CMMC SC.L2-3.13.11**

When `uuidgen` and `/proc/sys/kernel/random/uuid` are both unavailable — a
minimal container, or any non-Linux Unix — `crypto_uuid` falls through to a
hand-built "v4-like" UUID derived **entirely from a nanosecond timestamp**. It
sets the version nibble to `4` and the variant bits to `8`, advertising
randomness it does not have.

Measured: nanoseconds since epoch is a 61-bit quantity, so `ts >> 96`,
`ts >> 80` and `ts >> 64` are all identically zero. The value has the shape

```
00000000-0000-4000-98cc-<48 bits of clock>
```

Three of five groups constant, **zero bits of randomness**, monotonic.

Held at Medium rather than High because it is a third-tier fallback and no
first-party code uses `uuid()` for a token; the risk is to callers who reasonably
assume a v4 UUID is unpredictable. Silent degradation is the aggravating factor.

**Recommendation.** Fail loudly, or use the CSPRNG the API-key path already uses
(`rand::random`).

---

## AS-2026-06 — modulo bias when reducing random bytes to a charset

**Low · Measured · CWE-1241, CWE-331 · CMMC SC.L2-3.13.11 (approximate)**

`crypto_random_string` draws from a sound source on both platforms —
`System.Security.Cryptography.RandomNumberGenerator` on Windows, `/dev/urandom`
on Unix — then reduces each byte with `% charset.len()`. For the default
62-character set, 256 = 4×62 + 8, so the first eight characters are drawn with
probability 5/256 against 4/256 — **25% more likely**.

Quantified honestly, the cost is small: 5.9497 bits per character against a
uniform 5.9542, a loss of **0.0045 bits per character**, about 0.14 bits across a
32-character token. A real defect against a uniformity requirement and a
negligible one against a brute-force attacker. Listed because an auditor will
find it and the fix — rejection sampling — is four lines.

---

## AS-2026-07 — the FIPS posture is narrower than the documentation set implies

**Informational · Verified · FIPS 180-4, FIPS 197, SP 800-132, SP 800-90A · CMMC SC.L2-3.13.11**

`require_fips_hash` is **fully applied** — verified three ways, not assumed:
exactly three builtins accept a caller-chosen hash algorithm (`crypto_hash`,
`crypto_hash_file`, `crypto_hmac`); the `md5` and `sha1` match arms occur only in
those three; and there are exactly three `let algo` bindings, each followed within
seven lines by its gate.

Three scope caveats belong next to the claim:

* The gate covers **hashes only**. Cipher selection, key derivation and
  random-number generation are not gated by `AETHER_FIPS`, and AS-2026-05 is a
  non-approved generator reachable while FIPS mode is on.
* Cryptography is **delegated to the host** — the OpenSSL CLI and PowerShell's
  CNG wrappers. The validated-module boundary, if any, belongs to the operating
  system, not to AetherShell. `docs/security/CRYPTO_AND_FIPS.md` already states
  this plainly, which is to its credit.
* `docs/security/FIPS_140-2_COMPLIANCE.md` names a **superseded** standard.
  FIPS 140-3 is current and 140-2 validations are past their sunset. The document
  should be renamed and re-scoped against 140-3.

---

## Dependency position

`cargo-audit 0.22.2` against the RustSec database: **zero vulnerabilities**. Nine
advisories, all informational — five unmaintained crates, four soundness issues.
CI runs this on every push.

| Advisory | Crate | Class |
|---|---|---|
| RUSTSEC-2026-0221 | event-listener 5.4.1 | unsound |
| RUSTSEC-2026-0253 | lru 0.12.5 | unsound |
| RUSTSEC-2026-0002 | lru 0.12.5 | unsound |
| RUSTSEC-2026-0186 | memmap2 0.9.10 | unsound |
| RUSTSEC-2025-0119 | number_prefix | unmaintained |
| RUSTSEC-2024-0436 | paste | unmaintained |
| RUSTSEC-2024-0388 | derivative | unmaintained |
| RUSTSEC-2024-0384 | instant | unmaintained |
| RUSTSEC-2024-0370 | proc-macro-error | unmaintained |

**Assessment: acceptable.** The two `lru` advisories are worth tracking — a bump
to `lru 0.13+` clears both. The unmaintained set is dominated by build-time
proc-macro crates, which do not ship in the binary.

CMMC RA.L2-3.11.2 (vulnerability scanning) and SI.L2-3.14.1 (flaw remediation):
**met**, automated in CI.

---

## Limitations — read this before commissioning the external review

Every finding here, and the eleven remediated earlier the same day, came from a
single lens: **where does a value reach something that parses it?** That lens is
productive and narrow. It says nothing about:

* time-of-check/time-of-use between the jail check and the write;
* concurrency — the guard, the approval set and the audit chain are
  process-global mutable state;
* the approval-token lifecycle across process restarts;
* whether the effect taxonomy is the right model, as opposed to correctly
  implemented;
* the MCP and agent API transport surfaces, reviewed only where they touch the
  safety core;
* supply-chain integrity beyond advisory scanning — no reproducible-build or
  provenance check.

**The boundary of the method is the best available guess at where the remaining
defects are.** An independent review should start there rather than re-walking
the parser-injection ground, which is now covered by mechanical tests.
