# Dependency Security Management

**MED-005 Implementation**: Automated dependency vulnerability scanning and management

## Overview

AetherShell implements comprehensive dependency security scanning to detect and address:
- Known security vulnerabilities (CVEs)
- Unmaintained dependencies
- License compliance issues
- Supply chain attacks
- Secrets in version control

## Automated Scanning

### GitHub Actions Workflow

The `.github/workflows/security-audit.yml` workflow runs automatically:

- **On every push to main**: Full security audit
- **On pull requests**: Dependency review and vulnerability check
- **Weekly (Mondays 9 AM UTC)**: Comprehensive scan including outdated dependencies
- **On demand**: Manual workflow trigger

### What's Scanned

1. **Vulnerability Database** (`cargo audit`)
   - Checks against RustSec Advisory Database
   - Identifies CVEs in dependencies
   - Reports unmaintained crates

2. **Outdated Dependencies** (`cargo-outdated`)
   - Weekly scan for newer versions
   - Root dependencies prioritized
   - JSON reports generated

3. **Supply Chain Security** (`cargo-deny`)
   - License compliance checking
   - Dependency source verification
   - Banned crate detection
   - Duplicate version warnings

4. **SBOM Generation** (`cargo-sbom`)
   - CycloneDX format (industry standard)
   - SPDX format (compliance)
   - Updated on every release

5. **Secret Scanning**
   - Gitleaks: Detects hardcoded secrets
   - TruffleHog: High-confidence secret detection
   - Pre-commit hooks for local checking

## Current Status

### Known Unmaintained Dependencies (Acceptable)

These are **warnings only** - no known vulnerabilities:

| Crate              | Version | Reason       | Via                | Status                              |
| ------------------ | ------- | ------------ | ------------------ | ----------------------------------- |
| `derivative`       | 2.2.0   | Unmaintained | keyring → zbus     | ✅ Low risk (transitive)             |
| `instant`          | 0.1.13  | Unmaintained | keyring → async-io | ✅ Low risk (transitive)             |
| `paste`            | 1.0.15  | Unmaintained | ratatui            | ✅ Will be removed in ratatui update |
| `proc-macro-error` | 1.0.4   | Unmaintained | utoipa             | ✅ Can upgrade to utoipa 5.x         |

**Risk Assessment**: ⚠️ **LOW**
- All are transitive dependencies (not directly used)
- From trusted, well-maintained parent crates
- No known security vulnerabilities
- Alternatives being tracked

### Upgrade Path

```bash
# To remove proc-macro-error (when ready):
cargo update utoipa --precise 5.0.0

# To check for ratatui updates:
cargo update ratatui

# Note: keyring dependencies will be updated by keyring maintainers
```

## Local Development

### Install Security Tools

```bash
# Vulnerability scanning
cargo install cargo-audit

# Outdated dependency checking
cargo install cargo-outdated

# Supply chain verification
cargo install cargo-deny

# SBOM generation
cargo install cargo-sbom
```

### Run Manual Scans

```bash
# Quick vulnerability check
cargo audit

# Detailed report with JSON output
cargo audit --json > audit-report.json

# Check for outdated dependencies
cargo outdated --root-deps-only

# Verify supply chain
cargo deny check

# Generate SBOM
cargo sbom > sbom.json
```

### Pre-Commit Hook Setup

```bash
# Install the pre-commit hook
cp .github/scripts/pre-commit .git/hooks/pre-commit
chmod +x .git/hooks/pre-commit

# Test it
git commit --dry-run

# Bypass if needed (for false positives)
git commit --no-verify
```

## Incident Response

### If Vulnerability Detected

1. **Assess Severity**
   ```bash
   cargo audit --json | jq '.vulnerabilities[] | {id, severity, crate}'
   ```

2. **Check Affected Code**
   ```bash
   cargo tree -i <vulnerable-crate>
   ```

3. **Update Immediately**
   ```bash
   cargo update <vulnerable-crate>
   cargo test --all
   ```

4. **Verify Fix**
   ```bash
   cargo audit
   ```

5. **Deploy Emergency Patch**
   - Create hotfix branch
   - Update SECURITY.md
   - Tag emergency release
   - Notify users via GitHub Security Advisory

### If No Update Available

1. **Remove Dependency** (if possible)
2. **Find Alternative** (check crates.io)
3. **Vendor and Patch** (last resort)
   ```bash
   cargo vendor
   # Apply security patch manually
   ```
4. **Report Upstream** (create issue with CVE details)

## Reporting Security Issues

### For AetherShell Vulnerabilities

**Email**: security@nervosys.ai  
**PGP Key**: (See SECURITY.md)

**Do NOT** open public issues for security vulnerabilities.

### For Dependency Vulnerabilities

1. Check if already reported: https://rustsec.org
2. If not, report to:
   - Crate maintainer (via GitHub)
   - RustSec: https://github.com/RustSec/advisory-db
   - Report via `cargo audit fix` if supported

## Compliance

### NIST SP 800-53 Controls

- **SA-12**: Supply Chain Protection ✅
- **RA-5**: Vulnerability Scanning ✅
- **SI-2**: Flaw Remediation ✅

### SBOM Standards

- **CycloneDX 1.4**: Generated automatically ✅
- **SPDX 2.3**: Generated automatically ✅
- **Retention**: 90 days in CI artifacts

## Metrics

### Security Posture

- **Critical Vulnerabilities**: 0 ✅
- **High Severity**: 0 ✅
- **Medium Severity**: 0 ✅
- **Unmaintained (warnings)**: 4 ⚠️
- **Last Scan**: Automated weekly
- **Response Time**: < 24 hours for critical

### Dependency Health

- **Total Dependencies**: 504
- **Direct Dependencies**: ~40
- **Average Age**: Regularly updated
- **License Compliance**: 100% ✅

## References

- [RustSec Advisory Database](https://rustsec.org)
- [cargo-audit Documentation](https://github.com/rustsec/rustsec/tree/main/cargo-audit)
- [cargo-deny Documentation](https://embarkstudios.github.io/cargo-deny/)
- [SBOM Standards](https://www.cisa.gov/sbom)
- [NIST SP 800-53 Rev. 5](https://csrc.nist.gov/publications/detail/sp/800-53/rev-5/final)

---

**Last Updated**: October 27, 2025  
**Next Review**: Weekly (automated)  
**Owner**: Security Team
