# Security audit — 2026-09-01

Second audit. White-box review of the tree at `059484f`..HEAD, performed by
reading code **and running it**. Frameworks: CWE, MITRE ATT&CK, NIST FIPS,
CMMC 2.0 (NIST SP 800-171 Rev 2).

**Scope, and why it is narrow.** The 2026-08-26 audit covered the tree as it
then stood and is closed. This one covers what has changed since — roughly 790
added lines across nine files, three new cryptographic dependencies, and a
parser change. That code had never been audited, and some of it was written in
the same session that fixed the last findings, which is exactly the material
most likely to carry a fresh mistake.

**What this is not.** Not a certification, not a penetration test, not a CVE
assignment. CMMC mappings are a self-assessment.

**Evidence grades.** *Demonstrated* — executed and observed. *Structural* —
established by reading the code path. *Measured* — quantified.

---

## Summary

| ID | Severity | Evidence | Summary | Status |
|---|---|---|---|---|
| AS-2026-08 | Info | Structural | the cipher chain had one non-FIPS-approved primitive (Argon2id) | **fixed** |
| AS-2026-09 | Low | **Demonstrated** | the envelope version was not bound into the AEAD's AAD | **fixed** |
| AS-2026-10 | Low | **Demonstrated** | the string-repeat cap is per-operation and `+` walks past it | **open, documented** |
| AS-2026-11 | Low | Structural | the append-only audit sink was not protected by the workspace jail | **fixed** |
| AS-2026-12 | Info | **Demonstrated** | an undefined variable renders as `null` in string interpolation | **open, by design** |
| AS-2026-13 | Medium | **Demonstrated** | lambdas do not capture their defining environment; currying fails, sometimes silently | **open, documented** |

Also closed from the previous audit: **AS-2026-02's residue** now has a
supported mitigation (`AETHER_AUDIT_SINK`), and the `.gitignore` glob that
excluded a shipped example from the repository entirely.

---

## AS-2026-08 — the cipher chain had one non-approved primitive

**Info · Structural · NIST SP 800-132 · CMMC SC.L2-3.13.11**

10.0.0 moved `crypto.encrypt` to AES-256-GCM specifically so the cipher would
be FIPS-approved. The key derivation stayed Argon2id, which is **not**
approved — SP 800-132 approves PBKDF2. `FIPS_140-2_COMPLIANCE.md` was amended
to name that exception honestly, which is better than the blanket claim it
replaced but still leaves a deployment that must be all-approved unable to use
the builtin.

**FIXED.** Under `AETHER_FIPS` the key is derived with PBKDF2-HMAC-SHA256 at
600,000 iterations and the ciphertext carries the tag `AE1F`; otherwise it is
Argon2id and `AE1`. Argon2id remains the default because it is the better
choice against offline cracking, and this is a trade a FIPS deployment makes
deliberately rather than one imposed on everyone.

Two details that decide whether this is usable:

* **Decrypt reads the KDF from the envelope, never from the ambient mode.**
  Ciphertext written under `AETHER_FIPS` is readable without it and vice
  versa. Demonstrated in both directions; without this, toggling the mode
  would strand data.
* **The iteration count is a constant, not a field.** The envelope is not
  authenticated until after key derivation, so a count carried in the
  ciphertext would let an attacker demand an arbitrarily expensive derivation
  before the tag could reject it — a denial of service reachable by anyone who
  can hand the shell a ciphertext. Changing the count means minting a new
  version tag.

## AS-2026-09 — the envelope version was not bound into the AAD

**Low · Demonstrated · CWE-757 · ATT&CK T1600 · CMMC SC.L2-3.13.11**

Both envelope versions used the same additional authenticated data,
`AetherShell-AE1`. The version tag therefore selected the KDF but was not
itself authenticated.

The scheme still rejected a swapped tag — `AE1F` relabelled as `AE1` derives
the key with Argon2id instead of PBKDF2, produces a different key, and fails
the GCM tag. That is a *consequence of the two KDFs differing*, not a property
of the format: it would evaporate the moment two versions ever derived the
same key, which is exactly the situation a future version tag might create.

**FIXED.** Each version now has its own AAD, so substituting the tag is
rejected by the AEAD itself. Demonstrated: relabelling an `AE1F` ciphertext as
`AE1` returns `E_DECRYPT_FAILED: authentication failed`.

`AE1`'s AAD is deliberately unchanged. 10.0.0 shipped ciphertext authenticated
under exactly those bytes, and changing them would make every one of those
ciphertexts undecryptable — the fix must not become the incident.

## AS-2026-10 — the string-repeat cap is a speed bump, not a bound

**Low · Measured · CWE-770 · ATT&CK T1499 · CMMC SC.L2-3.13.1**

`"x" * n` is capped at 8 MiB so that a typo cannot ask for gigabytes. The cap
is **per operation**, and `+` is not capped at all:

```text
let a = "x" * 8000000     # allowed, just under the cap
a + a                     # 16,000,000 bytes — measured
```

So the guard stops the single most obvious form and nothing else. Reported
rather than quietly relied upon, because a limit that reads like a memory
bound and is not one is worse than no limit: it invites the assumption.

**Open, and what would close it.** A real bound needs a per-evaluation
allocation budget applied to every value-producing operation, not a constant
inside one match arm. That is a design change with its own cost — every string
operation would consult a running total — and it should be decided rather than
bolted on. Mitigating in the meantime: the language has no loop construct, so
unbounded growth requires recursion, which `safety::enter_call` already
depth-limits.

## AS-2026-11 — the audit sink was outside the jail's protection

**Low · Structural · CWE-732 · ATT&CK T1070 · CMMC AU.L2-3.3.8**

`safety::is_audit_artifact` — the rule that stops a jailed `file.write` from
touching the audit log — knew only about `audit_path()`. The new
`AETHER_AUDIT_SINK` is evidence of exactly the same kind. It is normally
outside the workspace, which is the point of it, but nothing enforced that,
and a sink placed inside the jail was an ordinary writable file.

**FIXED.** The sink path is now an audit artifact too.

## AS-2026-12 — an undefined variable interpolates as `null`

**Info · Demonstrated · CWE-457**

```text
print("v: ${nope_xyz}")   # -> "v: null"
```

A misspelled name in a string produces `null` rather than an error, silently.
A misspelled *field* or *function* does not — those now render
`${expr} [error: …]`, which is the fix made in 10.1.0.

**Open, by design.** This is the language's existing treatment of undefined
names — `print(nope_xyz)` is also `null` — and interpolation is being
consistent with it. Changing it is a language decision, not a security fix, so
it is recorded here rather than acted on. Worth knowing when reading a script
whose output contains an unexpected `null`.

## AS-2026-13 — lambdas do not capture their defining environment

**Medium · Demonstrated · CWE-664**

A lambda is stored as parameters plus a body and evaluated in the *caller's*
environment. A lambda over a binding that is still live therefore works:

```text
let factor = 3
let f = fn(x) => x * factor
f(2)                                  # 6
```

A lambda returned from another lambda does not, because the enclosing
parameter is gone by the time it is called:

```text
let mk = fn(factor) => fn(x) => x * factor
mk(3)(2)                              # unsupported op Mul on Int(2) and Null
```

**The failure is not always loud.** `Mul` rejects `Null`, so the arithmetic
case errors. Concatenation does not:

```text
let mk = fn(f) => fn(x) => "v: " + f
mk("A")(1)                            # "v: null"
```

— a wrong answer, no error, no warning. That is what raises this above the
informational findings: a partial-application helper can return plausible
nonsense.

Found by running `test-scripts/integration/test_complex_workflows.ae`, whose
"higher-order function pattern" test had been asserting this and failing since
it was written.

**Open.** Fixing it means giving `Lambda` a captured environment, which is a
change to the value model — `Lambda` derives `Serialize`, `Deserialize` and
`PartialEq`, so an `Env` field propagates into all three. That is a language
redesign with a wide blast radius and it should be decided deliberately, not
appended to an audit. Recorded here with a reproduction so the decision has
the evidence it needs. The affected script now uses the form the language does
support, with a comment pointing at this finding.

---

## Reviewed and found sound

Verified by execution, not by reading alone. Each of these was a plausible
place for the last session's changes to have gone wrong.

* **`str()` does not leak internals.** A lambda renders `<lambda>`, an error
  renders its message. No pointer, no debug spew.
* **Interpolation does not expand recursively.** A value that itself contains
  `${…}` is not re-interpolated, so there is no expansion bomb (the billion
  laughs shape).
* **The colour policy honours precedence.** `NO_COLOR` beats `FORCE_COLOR`,
  matching every other tool that implements both; stdout and stderr are
  decided independently.
* **The parser's statement-boundary fix does not drop statements.** Pinned by
  `tests/statement_boundaries.rs`, including that word-calls still take
  arguments on their own line.
* **The keyed audit chain rejects both downgrade directions** and the key is
  removed from the process environment before any child can inherit it.
* **`print` no longer truncates**, so a script cannot silently lose the tail of
  its own output — the previous behaviour was a correctness bug with an
  auditing edge: a long entry printed for a human review step was cut at 80
  characters.

## Dependency position

Three cryptographic dependencies added since the last audit: `aes-gcm`,
`hmac`, `pbkdf2` — all RustCrypto, all widely used. `chacha20` was moved off a
yanked version. No new advisories.

## What remains open across both audits

1. **In-process code can still forge the audit chain.** Whatever key the shell
   appends with, it can forge with. `AETHER_AUDIT_SINK` is now the supported
   mitigation — point it at a FIFO drained by a collector, a WORM mount, or a
   path where the shell's user has append but not write — but the integrity
   comes from what is behind the path, not from the shell. This is as closed as
   it gets without leaving the process.
2. **AS-2026-10**, above: no global allocation bound.
3. **An external penetration test**, which requires a third party.
