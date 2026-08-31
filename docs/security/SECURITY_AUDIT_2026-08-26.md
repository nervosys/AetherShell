# Security audit — 2026-08-26

White-box review of the tree at `77c7847`, performed by reading code and running
it. Frameworks: CWE, MITRE ATT&CK, NIST FIPS, CMMC 2.0 (NIST SP 800-171 Rev 2).

**What this is not.** Not a certification, not a penetration test, and not a CVE
assignment — identifiers are issued by a CNA and none is claimed here. Dependency
advisory IDs are real (RUSTSEC); first-party findings carry local IDs. CMMC
mappings are a self-assessment, and where a mapping is approximate it says so.

**Evidence grades.** *Demonstrated* — executed and observed. *Structural* —
established by reading the code path, not executed. *Measured* — quantified.

**Remediation status.** Five of the seven were fixed the same day —
AS-2026-01, -03, -05, -06 and the jail half of -02 — and each fix is marked in
place below. AS-2026-07 was corrected: as first filed it was wrong.

The two that were left open on purpose were closed on **2026-08-31**, in 10.0.0,
which is the release their breaking changes belonged in:

* **AS-2026-02** — the audit chain is keyed (HMAC-SHA256, opt-in), and a rewrite
  is now detected at the next append rather than only at the next offline
  verify. One residue remains and is named in the section: an append-only sink
  is a deployment decision, not a code change.
* **AS-2026-04** — `crypto_encrypt` is authenticated (AES-256-GCM with Argon2id), in a versioned envelope, with the legacy unauthenticated format
  refused by default so the envelope cannot be stripped to downgrade it.

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

**FIXED 2026-08-31 (10.0.0).** In two parts, because the finding had two halves.

*The jail half*, fixed 2026-08-26: the workspace jail refuses any guarded
filesystem write whose target is the active audit log or its directory
(`safety::is_audit_artifact`), compared lexically so a write to a path that is
about to *become* the log is caught too.

*The keying half.* The chain is now HMAC-SHA256 when a key is configured, via
`AETHER_AUDIT_KEY` (hex or raw, 32-byte minimum) or `AETHER_AUDIT_KEY_FILE`.
Forging a chain now requires the key rather than requiring nothing.

The key-management questions this finding said were "a design decision rather
than a patch" are answered as follows, and the answers are the interesting part:

* **Where the key lives:** with whoever launches the shell — the same
  out-of-band channel that already carries `AETHER_APPROVE`. Not in the
  workspace, which is the region the audited party may write.
* **Who can read it:** this process, once. `AETHER_AUDIT_KEY` is read on first
  use and then **removed from the process environment**, so no child inherits
  it. This is load-bearing rather than tidy: children inherit the parent
  environment by default, and a key an approved `Exec` can read is a key it can
  forge with, which would have left the chain exactly as unkeyed as before.
* **Rotation:** each entry carries a `key_id` (16 hex, derived from the key), so
  a key change reads as a key change rather than as tampering. The 32-byte
  minimum exists because `key_id` would otherwise be a brute-force oracle on a
  short key.
* **Downgrade:** the `mac` label lives *inside* the tagged core, and a keyed
  verifier refuses any entry lacking it. Without that, an attacker would simply
  rewrite the log with a plain SHA-256 chain — which needs no key — and it would
  verify clean. `tests/audit_chain_keyed.rs` asserts both directions, including
  an explicit assertion that the forgery *does* pass an unkeyed verifier, so the
  test cannot quietly become vacuous.

Keying is **opt-in**: with no key configured the chain stays plain SHA-256, as
before. A security control that needs somewhere to put a key cannot invent that
somewhere on the operator's behalf, and defaulting to a key stored next to the
log would be theatre.

*Detection timing.* An approved `Exec` runs arbitrary code, and no jail rule
stops `sh -c '> audit.log'`. Keying makes that rewrite fail an offline verify;
it does not by itself make anyone look. So the audit layer now compares the
file's tail against what this process last wrote, on every append, and when they
diverge it writes a chained `audit_chain / tamper-detected` entry recording the
hash the chain was expected to continue from. A truncation therefore leaves a
permanent marker at the point of discovery instead of a clean-looking file. The
tail read is bounded to the last 64 KiB so this stays O(1) in log size.

**What remains open, and cannot be closed here.** Code running *inside this
process* can still forge, because whatever key the process appends with it can
also forge with. No in-process scheme fixes that; it needs an append-only sink
the process cannot rewrite — remote syslog, a WORM mount — which is a deployment
decision, not a code change. That is now the only residue of this finding.

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

**FIXED 2026-08-31 (10.0.0).** Option 1, taken deliberately in a major release
so the format break is versioned and announced rather than sprung.

`crypto_encrypt` now uses **AES-256-GCM** in-process, with the key derived
from the password by **Argon2id** — the KDF `auth.rs` already uses for password
storage, for the same reason. Output is a versioned envelope:

```
AE1.<b64 salt>.<b64 nonce>.<b64 ciphertext‖tag>
```

Salt (16 bytes) and nonce (12 bytes) are fresh per message from the same CSPRNG
as the API-key path, so the derived key is unique per ciphertext and encrypting
the same value twice does not produce the same output.

Three things came free with moving the cipher in-process, and they are worth
naming because each was a separate open item:

* **AS-2026-03 no longer applies to these two builtins.** There is no child
  process, so there is no password handoff to a child environment.
* **The builtin works on Windows.** The openssl path was `#[cfg(unix)]`, so
  `crypto.encrypt` had been returning `E_UNIMPLEMENTED` on the platform most of
  this project's development happens on — and the local test suite could say
  nothing about it either way.
* **"It decrypted" now means "it was not modified."** Poly1305 verifies the tag
  before any plaintext is released, so a wrong password and a modified
  ciphertext are the same answer, and neither releases data.

**The downgrade, which is the part worth reviewing.** An attacker who cannot
forge a tag will instead strip the envelope and present the remains as a legacy
ciphertext, hoping decrypt falls back to the unauthenticated path. So legacy
AES-256-CBC input is refused by default with `E_DECRYPT_UNAUTHENTICATED`, and is
readable only when the operator sets `AETHER_CRYPTO_LEGACY_DECRYPT=1`. That is a
breaking change for anyone holding pre-10.0.0 ciphertext, and deliberately a
loud one: the refusal names the variable and says to re-encrypt. Data written by
an older AetherShell stays recoverable in one step; it does not stay recoverable
*silently*, because silence is what the attacker wants.

**Why AES-GCM and not ChaCha20-Poly1305.** The latter is the more usual choice
for new code and would have been simpler to reach for. It is also not
FIPS-approved, and this project ships `docs/security/FIPS_140-2_COMPLIANCE.md`
asserting approved algorithms throughout and maps its findings to CMMC. Trading
an approved cipher (AES-256-CBC) for an unapproved one *inside a security
remediation* would have quietly falsified that claim for exactly the audience
the document is written for. AES-256-GCM is the approved AEAD and gives the
identical integrity guarantee, so the approval costs nothing here.

The KDF is the one primitive in this chain that is not FIPS-approved: Argon2id,
not PBKDF2. That is a deliberate choice of resistance to password cracking over
approval, it is unchanged from what `auth.rs` already used for password storage,
and it is now stated in the FIPS document rather than left implicit.

The objection raised against option 2 in the original filing — that hand-rolled
crypto in a remediation is how remediations become findings — is why this uses a
vetted AEAD rather than composing `openssl enc` with `openssl dgst` by hand. The
objection raised against option 1 — breaking every existing ciphertext with no
migration path — is answered by the version tag and the opt-in legacy path.

`tests/crypto_authenticated.rs` covers the round trip, modification of each
field of the envelope, the wrong password, nonce/salt freshness, the envelope
strip, and availability on every platform.

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
