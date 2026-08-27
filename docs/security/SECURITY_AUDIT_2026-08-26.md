# Security audit — 2026-08-26

White-box review of the tree at `77c7847`, performed by reading code and running
it. Frameworks: CWE, MITRE ATT&CK, NIST FIPS, CMMC 2.0 (NIST SP 800-171 Rev 2).

**What this is not.** Not a certification, not a penetration test, and not a CVE
assignment — identifiers are issued by a CNA and none is claimed here. Dependency
advisory IDs are real (RUSTSEC); first-party findings carry local IDs. CMMC
mappings are a self-assessment, and where a mapping is approximate it says so.

**Evidence grades.** *Demonstrated* — executed and observed. *Structural* —
established by reading the code path, not executed. *Measured* — quantified.

**Remediation status (same day).** Five of the seven were fixed after the audit
was written — AS-2026-01, -02, -03, -05 and -06 — and each fix is marked in place
below. **AS-2026-04 was deliberately not fixed**, for a reason given in its
section. AS-2026-07 was corrected: as first filed it was wrong.

Eleven further issues were found and fixed earlier the same day and are recorded
in `docs/HANDOFF.md`; each is pinned by a ratchet.

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

**FIXED 2026-08-26.** In-band `approve()` is now refused in **agent mode only**;
human mode is untouched and the documented REPL loop still works. The out-of-band
channel the design already provides — `AETHER_APPROVE` / `AETHER_APPROVE_ALL`, set
by whoever launches the agent — is unaffected, so no capability was removed that
was not already reachable by someone outside the agent's control.

The assumption behind that choice, stated so it can be overridden: **the
adversarial threat model is in scope.** For a shell built to be driven by language
models, prompt injection is the defining threat, and hardening in that direction is
the conservative error.

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

**PARTIALLY FIXED 2026-08-26.** The workspace jail now refuses any guarded
filesystem write whose target is the active audit log or its directory
(`safety::is_audit_artifact`), compared lexically so a write to a path that is
about to *become* the log is caught too.

**What that does not close, stated plainly:** an approved `Exec` call can still
reach the file, and the chain is still unkeyed. Keying it requires key management —
where the key lives, who can read it, what happens on rotation — which is a design
decision rather than a patch, and is left open deliberately.

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

**FIXED 2026-08-26.** `crypto_password_hash` reads the password from stdin
(`openssl passwd -6 -stdin`). `crypto_encrypt` and `crypto_decrypt` pass it through
the child environment (`-pass env:AE_OPENSSL_PASS`) rather than argv, because their
stdin already carries the plaintext. `/proc/<pid>/environ` is owner-readable only,
where `/proc/<pid>/cmdline` is world-readable — strictly better, and honest about
not being perfect.

---

## AS-2026-04 — encryption is unauthenticated

**Medium · Structural · CWE-353, CWE-327 · ATT&CK T1565 · CMMC SC.L2-3.13.16**

`crypto_encrypt` uses `openssl enc -aes-256-cbc -pbkdf2`. Cipher and KDF are both
FIPS-approved and `-pbkdf2` is correctly present — without it OpenSSL's legacy
`EVP_BytesToKey` would derive keys with MD5. What is missing is **integrity**:
CBC is malleable, so ciphertext can be altered and `crypto_decrypt` cannot detect
it.

**PARTIALLY ADDRESSED 2026-08-26 — and a separate bug found while doing it.**

`crypto_decrypt` reported a *decryption failure* as
`E_UNIMPLEMENTED: requires the openssl CLI (Unix only)`. It ran openssl, openssl
refused the input, and the code fell through to the "tool is missing" arm. So the
caller was told the tool was absent when the tool had in fact rejected their
data — which, for an unauthenticated cipher, is the **one signal that a
ciphertext has been tampered with**. It now returns `E_DECRYPT_FAILED`, carries
openssl's own message, and says plainly that a *successful* decrypt is not proof
of integrity either.

That does not make the scheme authenticated. It stops the one detection channel
that does exist from being mislabelled as a missing dependency.

Confirmed by measurement rather than memory: OpenSSL 3.5.7 answers
`enc: AEAD ciphers not supported`, so `-aes-256-gcm` genuinely is unreachable
through this path.

**The authentication gap itself remains open, deliberately.** The reason matters
more than the finding.

The obvious fix is unavailable: `openssl enc` refuses AEAD ciphers, so
`-aes-256-gcm` is not reachable through the CLI this builtin uses. That leaves
three options, and each is a decision rather than a patch:

1. **Add an AEAD crate** (`aes-gcm`) and encrypt in-process. Correct, but it adds
   a dependency and **breaks every existing ciphertext** with no migration path.
   Silently making previously-encrypted data undecryptable is not a change to
   make on an auditor's initiative.
2. **Encrypt-then-MAC by hand** with a second `openssl dgst -hmac` call. No new
   dependency, but it means designing a bespoke construction — key separation,
   framing, constant-time comparison — and hand-rolled crypto in a remediation is
   how remediations become findings.
3. **Leave it and say so**, which is what was done.

The honest position: `crypto_encrypt` provides **confidentiality, not
integrity**. AES-256-CBC and PBKDF2 are both FIPS-approved and correctly
invoked; ciphertext can be altered undetectably. Callers who need
tamper-evidence should not rely on this builtin alone. Option 1 is the right
long-term answer and belongs in a release where the format break can be
versioned and announced.

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

**FIXED 2026-08-26.** The fallback now draws 16 bytes from `rand::random` — the
same CSPRNG `auth.rs` uses for API keys — and sets the version and variant bits over
real entropy, so the v4 label is now earned.

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
find it and the fix is four lines.

**FIXED 2026-08-26.** Rejection sampling on both platforms: bytes at or above
`256 - (256 % n)` are discarded rather than folded. The Unix path already requested
twice the bytes it needed, which covers the rejections.

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
* **Correction to this finding as first filed.** It claimed
  `docs/security/FIPS_140-2_COMPLIANCE.md` "names a superseded standard". That was
  wrong and was reached by reading the *filename* rather than the document: the
  title is "FIPS 140-2/140-3 Compliance Assessment", it covers both, it states
  that 140-3 supersedes 140-2, and it is explicit that AetherShell is not
  independently validated. The document is more honest than the finding was.

  The real problem, found on reading it: it asserts **`COMPLIANT` for both
  standards**, dated **24 October 2025** against **version 0.1.0**, and that
  verdict has been carried unqualified into **8.0.0** — eight major versions and,
  as of today, eleven fixed vulnerabilities later. Its claim of "exclusive use of
  FIPS-validated cryptographic libraries" was contradicted at 8.0.0 by AS-2026-05
  and AS-2026-06, both hand-rolled generators. A currency warning has been added
  to the head of that document; re-running the assessment against 8.0.0 is the
  actual remediation.

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
