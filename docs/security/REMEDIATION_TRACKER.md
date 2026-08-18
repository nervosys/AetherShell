# Security Remediation Tracker

**Generated**: October 18, 2025
**From**: DOD Cybersecurity Audit
**Status**: 🟡 PARTIALLY RE-VERIFIED — see the re-assessment below

> ### Re-assessment, 2026-08-18
>
> The table below says **0 of 23 complete** and calls the CRITICALs release
> blockers. 8.0.0 is published, so that reading is either alarming or stale.
> Spot-checking against the code found it is **both**, which is why the counts
> are left alone rather than quietly flipped — an unverified "complete" is
> worse than an honest "unknown".
>
> **Stale (control shipped since, verified by running its tests):**
>
> | ID | Control | Evidence |
> | --- | --- | --- |
> | CRIT-1 | Agent command allowlist | `AGENT_ALLOW_CMDS` enforced in `src/security.rs`; `tests/safety.rs` 16 pass |
> | CRIT-2 | API key / secret hygiene | redaction in `src/safety.rs`; `tests/secret_hygiene.rs` 4 pass |
> | CRIT-3 | Path traversal | `validate_safe_path` + workspace jail; `tests/guard_enforcement.rs` 14 pass |
>
> This verifies the **controls exist and hold**. It does not satisfy the
> process criteria those entries also list — external security review, and a
> penetration test — which remain undone and are not something a test run can
> establish.
>
> **Accurate, and now fixed:**
>
> | ID | Finding | Outcome |
> | --- | --- | --- |
> | HIGH-2 | SQL Injection Prevention | **Was live.** `db_kv_get(db, "x' OR '1'='1")` returned another key's value; `db_kv_delete(db, "z'; DELETE FROM kv; --")` emptied the table. Fixed via `safety::sql_literal` / `sql_identifier`; `tests/sql_injection.rs` |
>
> **Full audit, 2026-08-18.** All 15 itemised findings checked against the
> code. (The header counts 8 LOW; no `LOW-n` sections were ever written, so
> there is nothing to audit there.)
>
> | ID | Verdict | Evidence |
> | --- | --- | --- |
> | CRIT-1 | stale — control shipped | `AGENT_ALLOW_CMDS` in `src/security.rs`; `tests/safety.rs` 16 pass |
> | CRIT-2 | stale — control shipped | redaction in `src/safety.rs`; `tests/secret_hygiene.rs` 4 pass |
> | CRIT-3 | stale — control shipped | `validate_safe_path` + jail; `tests/guard_enforcement.rs` 14 pass |
> | HIGH-1 | not exploitable via the HTTP surface | malformed bodies (empty/null/NaN/lone surrogate) all return 400/422, server stays up |
> | HIGH-2 | **was live — fixed** | SQL injection; `safety::sql_literal`; `tests/sql_injection.rs` |
> | HIGH-3 | control present | prompt-injection handling in `agent`, `safety`, `os_tools` |
> | HIGH-4 | **fixed** | 3 unguarded env tests + 2 needless `unsafe` blocks removed; 65/65 now guarded |
> | HIGH-5 | mitigated, not implemented | no rate limiter, but every route needs a 256-bit bearer token, so there is nothing to brute-force cheaply |
> | MED-1 | done | hash-chained tamper-evident audit log + `verify_audit` |
> | MED-2 | mitigated by the framework | axum 0.7 default 2 MB body cap — **measured**: an 8 MB body returns 413 |
> | MED-3 | **addressed, not by TLS** | a non-loopback bind is now *refused* unless `AETHER_ALLOW_REMOTE_BIND=true`. TLS would protect the token in transit but not the decision to expose a builtin-executing endpoint, which is the part worth a deliberate act; the refusal points at an SSH tunnel or a TLS-terminating proxy |
> | MED-4 | **was live — fixed** | MCP HTTP server executed builtins unauthenticated; cross-origin file read demonstrated |
> | MED-5 | **done** | `nosniff`, `X-Frame-Options: DENY`, `default-src 'none'; frame-ancestors 'none'`, `no-referrer`, `no-store` on both servers — verified on a live response |
> | MED-6 | done | gitleaks runs; non-blocking pending a git-history triage, stated in the workflow |
> | MED-7 | done | `cargo audit` + `cargo deny` now gate for real; found and fixed RUSTSEC-2026-0258 |
>
> Two of the fifteen were live vulnerabilities, both marked NOT STARTED and
> both accurate. The counts in the table above are still not flipped: process
> criteria (external review, penetration test) remain undone, and this audit
> establishes that controls exist and hold, which is a different claim.

---

## Quick Status Overview

| Priority       | Total  | Complete | In Progress | Not Started |
| -------------- | ------ | -------- | ----------- | ----------- |
| 🔴 **CRITICAL** | 3      | 0        | 0           | 3           |
| 🟠 **HIGH**     | 5      | 0        | 0           | 5           |
| 🟡 **MEDIUM**   | 7      | 0        | 0           | 7           |
| 🟢 **LOW**      | 8      | 0        | 0           | 8           |
| **TOTAL**      | **23** | **0**    | **0**       | **23**      |

---

## CRITICAL - Pre-Release Blockers

### ❌ CRIT-1: Agent Command Execution Sandbox
- **Status**: ❌ NOT STARTED
- **Severity**: CRITICAL (CVSS 9.1)
- **CWE**: CWE-78 (OS Command Injection)
- **Location**: `src/agent.rs`
- **Effort**: 3 days
- **Owner**: Unassigned
- **Blocker**: YES - Cannot release without fix

**Tasks**:
- [ ] Implement `AGENT_ALLOW_CMDS` enforcement
- [ ] Add argument validation function
- [ ] Create sandboxed execution environment
- [ ] Add comprehensive tests
- [ ] Security review by 2+ engineers
- [ ] Penetration test agent functionality

**Acceptance Criteria**:
- Agent can only call allowlisted commands
- Arguments are validated for injection attempts
- Unauthorized commands are rejected with clear errors
- All agent actions are logged
- Test coverage > 95%

---

### ❌ CRIT-2: API Key Security
- **Status**: ❌ NOT STARTED
- **Severity**: CRITICAL (CVSS 8.7)
- **CWE**: CWE-798, CWE-522
- **Location**: `src/ai.rs`, `src/ai_api/providers.rs`
- **Effort**: 5 days
- **Owner**: Unassigned
- **Blocker**: YES

**Tasks**:
- [ ] Replace plain env vars with `secrecy` crate
- [ ] Implement OS credential store integration
  - [ ] Windows Credential Manager
  - [ ] macOS Keychain
  - [ ] Linux Secret Service
- [ ] Add constant-time comparison for keys
- [ ] Implement key rotation mechanism
- [ ] Add key usage audit logging
- [ ] Remove keys from error messages
- [ ] Zeroize keys in memory on drop
- [ ] Add key expiration (90 days)

**Acceptance Criteria**:
- API keys never in plain text in memory
- Keys stored in OS-specific secure storage
- No keys in logs or error messages
- Key rotation works automatically
- Audit trail for all key usage

---

### ❌ CRIT-3: Path Traversal Prevention
- **Status**: ❌ NOT STARTED
- **Severity**: CRITICAL (CVSS 8.2)
- **CWE**: CWE-22, CWE-73
- **Location**: `src/builtins.rs` (ls, cat, find, etc.)
- **Effort**: 5 days
- **Owner**: Unassigned
- **Blocker**: YES

**Tasks**:
- [ ] Create `validate_safe_path()` function
- [ ] Implement path canonicalization
- [ ] Add allowed directory allowlist
- [ ] Handle symbolic links securely
- [ ] Add Unicode normalization
- [ ] Update all file operation functions
  - [ ] `bi_ls`
  - [ ] `bi_cat`
  - [ ] `bi_find`
  - [ ] `bi_head`
  - [ ] `bi_tail`
  - [ ] `bi_get_files`
  - [ ] `bi_get_content`
- [ ] Add comprehensive path traversal tests
- [ ] Penetration test file operations

**Acceptance Criteria**:
- All paths validated before access
- Directory traversal attacks blocked
- Symbolic link attacks prevented
- Clear error messages for blocked access
- Security test suite with 50+ attack patterns

---

## HIGH - Launch Requirements

### ❌ HIGH-1: Replace .unwrap() Usage
- **Status**: ❌ NOT STARTED
- **Severity**: HIGH (CVSS 7.1)
- **CWE**: CWE-248, CWE-703
- **Effort**: 1 week
- **Owner**: Unassigned

**Tasks**:
- [ ] Audit codebase for all `.unwrap()` calls (35+ found)
- [ ] Audit codebase for all `.expect()` calls
- [ ] Replace with proper error handling (`?` operator)
- [ ] Add context to all errors
- [ ] Update tests to expect `Result` types
- [ ] Run tests to ensure no panics

**Files to Update**:
- `tests/ai_agents_comprehensive.rs` (2 instances)
- `src/builtins.rs` (3+ instances)
- `src/eval.rs` (3+ instances)
- `src/ai_api/providers.rs` (2+ instances)
- And 20+ more locations

**Acceptance Criteria**:
- Zero `.unwrap()` calls in production code (tests OK)
- Zero panics on invalid input
- All errors return `Result` with context

---

### ❌ HIGH-2: SQL Injection Prevention
- **Status**: ❌ NOT STARTED
- **Severity**: HIGH (CVSS 8.6)
- **CWE**: CWE-89
- **Effort**: 5 days
- **Owner**: Unassigned

**Tasks**:
- [ ] Implement parameterized queries with `sqlx`
- [ ] Add query allowlist for agents
- [ ] Validate only SELECT queries in read-only mode
- [ ] Add query timeout (30 seconds)
- [ ] Limit result set size (10,000 rows)
- [ ] Add query audit logging
- [ ] Use read-only database connections
- [ ] Implement connection pooling with limits
- [ ] Security test with SQLMap

**Acceptance Criteria**:
- All queries use prepared statements
- No string concatenation for SQL
- SQL injection attacks blocked
- Comprehensive test suite

---

### ❌ HIGH-3: AI Prompt Injection Prevention
- **Status**: ❌ NOT STARTED
- **Severity**: HIGH (CVSS 7.8)
- **CWE**: CWE-20, CWE-94
- **Effort**: 5 days
- **Owner**: Unassigned

**Tasks**:
- [ ] Implement `validate_agent_goal()` function
- [ ] Add prompt length limits (4000 chars)
- [ ] Add tool count limits (20 max)
- [ ] Detect injection patterns
- [ ] Sanitize control characters
- [ ] Implement cost tracking per session
- [ ] Add rate limiting (10 agent calls/min)
- [ ] Log all agent goals for audit
- [ ] Implement content filtering
- [ ] Add user confirmation for sensitive ops

**Acceptance Criteria**:
- Prompt injection attacks detected
- Malicious prompts rejected
- Cost limits enforced
- All agent activity logged

---

### ❌ HIGH-4: Fix Unsafe Test Code
- **Status**: ❌ NOT STARTED
- **Severity**: HIGH (CVSS 6.9)
- **CWE**: CWE-783
- **Effort**: 2 days
- **Owner**: Unassigned

**Tasks**:
- [ ] Remove all `unsafe` blocks from tests
- [ ] Use `serial_test` for sequential tests
- [ ] Implement `EnvCleanup` drop guard
- [ ] Isolate test environments
- [ ] Fix environment variable pollution
- [ ] Add test isolation verification

**Acceptance Criteria**:
- Zero `unsafe` in test code
- Tests can run in parallel safely
- Environment cleanup guaranteed

---

### ❌ HIGH-5: Implement Rate Limiting
- **Status**: ❌ NOT STARTED
- **Severity**: HIGH (CVSS 7.5)
- **CWE**: CWE-770
- **Effort**: 3 days
- **Owner**: Unassigned

**Tasks**:
- [ ] Add `tower-governor` dependency
- [ ] Implement rate limiting middleware
- [ ] Configure per-IP limits
- [ ] Configure per-API-key limits
- [ ] Add 429 responses
- [ ] Log rate limit violations
- [ ] Implement progressive backoff
- [ ] Add CAPTCHA for repeat violations
- [ ] Load test rate limits

**Acceptance Criteria**:
- Rate limits enforced on all endpoints
- 429 responses with Retry-After headers
- DoS attacks mitigated
- Performance impact < 5%

---

## MEDIUM - Production Hardening

### ❌ MED-1: Comprehensive Security Logging
- **Status**: ❌ NOT STARTED
- **Severity**: MEDIUM (CVSS 5.3)
- **Effort**: 5 days

**Tasks**:
- [ ] Define security event taxonomy
- [ ] Implement structured logging (JSON)
- [ ] Log authentication attempts
- [ ] Log authorization failures
- [ ] Log file system access
- [ ] Log AI agent invocations
- [ ] Log API key usage
- [ ] Log configuration changes
- [ ] Log security violations
- [ ] Set up log aggregation
- [ ] Implement log retention (90 days)

---

### ❌ MED-2: Input Size Limits
- **Status**: ❌ NOT STARTED
- **Severity**: MEDIUM (CVSS 5.5)
- **Effort**: 3 days

**Tasks**:
- [ ] Define resource limits
  - [ ] MAX_FILE_SIZE: 100MB
  - [ ] MAX_ARRAY_SIZE: 100,000 elements
  - [ ] MAX_STRING_LENGTH: 10MB
  - [ ] MAX_RECURSION_DEPTH: 100
- [ ] Implement limits in all builtins
- [ ] Add clear error messages
- [ ] Add resource usage metrics
- [ ] Load test with large inputs

---

### ❌ MED-3: Enforce Strong TLS
- **Status**: ❌ NOT STARTED
- **Severity**: MEDIUM (CVSS 5.9)
- **Effort**: 3 days

**Tasks**:
- [ ] Enforce TLS 1.3 minimum
- [ ] Configure strong cipher suites only
- [ ] Implement certificate pinning
- [ ] Add HSTS headers
- [ ] Disable weak protocols (SSLv3, TLS 1.0/1.1)
- [ ] Add certificate validation tests

---

### ❌ MED-4: Restrict CORS
- **Status**: ❌ NOT STARTED
- **Severity**: MEDIUM (CVSS 5.4)
- **Effort**: 2 days

**Tasks**:
- [ ] Remove wildcard `*` from CORS origins
- [ ] Add specific allowed origins
- [ ] Implement strict CORS middleware
- [ ] Limit allowed methods
- [ ] Limit allowed headers
- [ ] Add CORS preflight caching

---

### ❌ MED-5: Add Security Headers
- **Status**: ❌ NOT STARTED
- **Severity**: MEDIUM (CVSS 5.0)
- **Effort**: 1 day

**Tasks**:
- [ ] Add X-Content-Type-Options: nosniff
- [ ] Add X-Frame-Options: DENY
- [ ] Add Strict-Transport-Security
- [ ] Add Content-Security-Policy
- [ ] Add X-XSS-Protection
- [ ] Add Referrer-Policy: no-referrer
- [ ] Test headers with securityheaders.com

---

### ❌ MED-6: Secrets Scanning
- **Status**: ❌ NOT STARTED
- **Severity**: MEDIUM (CVSS 5.7)
- **Effort**: 2 days

**Tasks**:
- [ ] Create GitHub Actions workflow
- [ ] Add TruffleHog scan
- [ ] Add Gitleaks scan
- [ ] Create pre-commit hook
- [ ] Add .gitignore entries for secrets
- [ ] Audit existing commits for secrets

---

### ❌ MED-7: Dependency Scanning
- **Status**: ❌ NOT STARTED
- **Severity**: MEDIUM (CVSS 5.8)
- **Effort**: 2 days

**Tasks**:
- [ ] Add cargo-audit to CI
- [ ] Add cargo-outdated checks
- [ ] Add cargo-supply-chain verification
- [ ] Implement monthly dependency updates
- [ ] Pin production dependencies
- [ ] Generate SBOM

---

## LOW - Continuous Improvement

(Detailed tasks omitted for brevity - see main audit report)

---

## Testing Requirements

### Security Test Suite
- [ ] Path traversal attack tests (50+ patterns)
- [ ] Command injection tests
- [ ] Prompt injection tests  
- [ ] SQL injection tests (when DB features added)
- [ ] Rate limiting tests
- [ ] Authentication bypass tests
- [ ] Authorization tests
- [ ] Input validation tests
- [ ] Fuzzing (72 hours continuous)
- [ ] Load testing (10,000 req/sec)
- [ ] Penetration testing (5 days)

### Test Coverage Goals
- [ ] Unit tests: > 80%
- [ ] Integration tests: > 70%
- [ ] Security tests: > 90%
- [ ] Overall: > 75%

---

## CI/CD Integration

### Required CI Checks
- [ ] `cargo test` - All tests pass
- [ ] `cargo clippy` - No warnings
- [ ] `cargo audit` - No vulnerabilities
- [ ] `cargo deny` - No banned dependencies
- [ ] TruffleHog - No secrets
- [ ] Gitleaks - No secrets
- [ ] Security unit tests pass
- [ ] Code coverage > 75%

### Pre-Merge Requirements
- [ ] All CI checks pass
- [ ] Security review (if touching sensitive code)
- [ ] 2+ approvals
- [ ] No merge commits (rebase required)

---

## Documentation Requirements

- [ ] Security Architecture Document
- [ ] Threat Model
- [ ] Incident Response Plan
- [ ] Security.txt file
- [ ] Vulnerability Disclosure Policy
- [ ] Security Training Materials
- [ ] Secure Coding Guidelines
- [ ] Deployment Security Checklist

---

## Compliance Tracking

### DISA STIG
- [ ] CAT I controls: 5/5 implemented
- [ ] CAT II controls: 15/15 implemented
- [ ] CAT III controls: 10/10 implemented

### NIST SP 800-53
- [ ] Access Control (AC)
- [ ] Audit and Accountability (AU)
- [ ] Security Assessment (CA)
- [ ] Configuration Management (CM)
- [ ] Identification and Authentication (IA)
- [ ] System and Communications Protection (SC)
- [ ] System and Information Integrity (SI)

---

## Sign-Off

### Phase 1 (Critical)
- [ ] Security Team Lead
- [ ] Engineering Lead
- [ ] QA Lead
- [ ] Penetration Tester

### Phase 2 (High)
- [ ] Security Team Lead
- [ ] Engineering Lead  
- [ ] Product Manager

### Phase 3 (Medium)
- [ ] Security Team Lead
- [ ] DevOps Lead

---

## Resources

- **Main Audit Report**: `docs/security/DOD_CYBERSECURITY_AUDIT.md`
- **NIST SP 800-53**: https://csrc.nist.gov/publications/detail/sp/800-53/rev-5/final
- **DISA STIG**: https://public.cyber.mil/stigs/
- **CWE Top 25**: https://cwe.mitre.org/top25/
- **OWASP Top 10**: https://owasp.org/Top10/

---

**Last Updated**: October 18, 2025  
**Next Review**: After Phase 1 completion
