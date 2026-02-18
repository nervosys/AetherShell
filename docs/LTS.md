# AetherShell Long-Term Support (LTS) Policy

**Effective**: v1.0.0
**Last Updated**: June 2025

---

## Overview

AetherShell follows a predictable release and support lifecycle to give users and organizations confidence in long-term adoption. This document defines release types, support durations, and maintenance commitments.

---

## Release Types

| Type | Minor Version | Active Support | Security Support | Total |
|------|---------------|----------------|------------------|-------|
| **LTS** | Even (1.0, 1.2, 1.4...) | 18 months | 30 months | 30 months |
| **Standard** | Odd (1.1, 1.3, 1.5...) | 6 months | 12 months | 12 months |

- **Active Support**: Bug fixes, performance improvements, and security patches.
- **Security Support**: Critical and high-severity security patches only.

---

## Version Support Matrix

| Version | Type | Release | Active Until | Security Until | Status |
|---------|------|---------|--------------|----------------|--------|
| 1.0.x | LTS | 2025 Q3 | 2027 Q1 | 2028 Q1 | Planned |
| 0.3.x | Current | 2025 Q2 | 1.0.0 + 90 days | 1.0.0 + 90 days | Active |
| 0.2.x | EOL | 2025 Q1 | — | — | End of Life |
| 0.1.x | EOL | 2024 Q4 | — | — | End of Life |

---

## Patch Commitment

### LTS Releases

LTS releases receive:

1. **All security fixes** rated HIGH or CRITICAL (CVSS >= 7.0)
2. **Critical bug fixes** that cause data loss, crashes, or incorrect results
3. **Dependency updates** for security vulnerabilities
4. **No breaking changes** to public API, CLI flags, or builtin behavior

### Standard Releases

Standard releases receive:

1. **Security fixes** rated HIGH or CRITICAL
2. **Bug fixes** at maintainer discretion
3. May include minor behavioral changes with migration notes

---

## Supported Platforms

LTS releases are tested and supported on:

| Platform | Architecture | Tier |
|----------|-------------|------|
| Ubuntu 22.04+ | x86_64 | 1 (fully supported) |
| Ubuntu 22.04+ | aarch64 | 1 |
| macOS 13+ | x86_64 | 1 |
| macOS 13+ | aarch64 (Apple Silicon) | 1 |
| Windows 10+ | x86_64 | 1 |
| Windows 11 | aarch64 | 2 (best-effort) |
| Alpine Linux 3.18+ | x86_64 | 2 |
| Fedora 38+ | x86_64 | 2 |

- **Tier 1**: CI-tested on every commit. Regressions block release.
- **Tier 2**: CI-tested periodically. Regressions fixed on best-effort basis.

---

## Rust Toolchain Policy

| AetherShell Version | Minimum Rust Version (MSRV) |
|---------------------|----------------------------|
| 1.0.x LTS | 1.75.0 |
| 1.1.x | Latest stable at release time |

MSRV will not be raised within an LTS release series. Standard releases may raise MSRV.

---

## Security Response SLAs

| Severity | Response Time | Patch Release |
|----------|--------------|---------------|
| CRITICAL (CVSS >= 9.0) | 24 hours | 72 hours |
| HIGH (CVSS >= 7.0) | 48 hours | 7 days |
| MEDIUM (CVSS >= 4.0) | 7 days | 30 days |
| LOW (CVSS < 4.0) | 30 days | Next scheduled release |

See [SECURITY.md](docs/security/SECURITY.md) for vulnerability reporting procedures.

---

## Dependency Management

- **Cargo.lock** is committed and pinned for reproducible builds
- **cargo audit** runs in CI on every push
- **deny.toml** enforces advisory database checks and license compliance
- LTS branches receive dependency updates only for security fixes

---

## End-of-Life (EOL) Process

When a version reaches EOL:

1. **90-day notice** posted in release notes and CHANGELOG
2. **Migration guide** published for upgrading to the next LTS
3. **Final security patch** released if any outstanding advisories
4. **Repository branch** archived (read-only)
5. **No further patches** after EOL date

---

## Migration Support

Each LTS release includes:

- **Migration guide** documenting all breaking changes from the previous LTS
- **Deprecation warnings** for features removed in the next major version
- **Compatibility mode** for running scripts written for previous versions (via transpiler)

---

## Stability Guarantees

Within an LTS release series, the following are guaranteed stable:

- ✅ Builtin function signatures and return types
- ✅ CLI flags and exit codes
- ✅ Agent API endpoints and response format
- ✅ AetherShell language syntax
- ✅ Value type system (Int, Float, String, Array, Record, Lambda)
- ✅ Pipeline behavior
- ✅ Configuration file format
- ✅ Environment variable names

The following may change in patch releases:

- ⚠️ Error message text (not error codes)
- ⚠️ Performance characteristics
- ⚠️ TUI visual layout
- ⚠️ Internal module organization

---

## Contact

- **Security issues**: See [SECURITY.md](docs/security/SECURITY.md)
- **General support**: GitHub Issues
- **Enterprise support**: Contact nervosys.ai

---

*This policy is subject to revision. Changes will be announced in release notes with at least one release cycle of notice.*
