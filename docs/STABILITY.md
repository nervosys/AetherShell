# AetherShell Stability Policy

> **Version:** Pre-1.0 (current: v0.3.1)
> **Last Updated:** February 2026

This document defines backward compatibility guarantees, deprecation policies, and the roadmap to a v1.0.0 stable release.

---

## Current Stability Level

AetherShell is in **pre-release** (v0.x). The language, builtins, and APIs are production-tested but may evolve. Breaking changes are possible between minor versions, but we minimize them and document all changes in CHANGELOG.md.

| Component | Stability | Notes |
|-----------|-----------|-------|
| Core syntax | **Stable** | Variables, lambdas, pipelines, match, try/catch |
| Value types | **Stable** | Int, Float, String, Array, Record, Lambda, Null, Bool |
| Type inference | **Stable** | Hindley-Milner engine, type annotations |
| Builtin signatures | **Stable** | Existing builtins won’t change return types |
| Module system | **Stable** | `module.function()` syntax |
| CLI flags | **Stable** | `--tui`, `-c`, `--version`, `--help` |
| Agent API endpoints | **Provisional** | May add fields; won’t remove existing ones |
| MCP tools | **Provisional** | Tool set may expand; won’t remove existing tools |
| TUI interface | **Provisional** | Key bindings and layout may change |
| Plugin TOML format | **Provisional** | Schema may evolve |
| Internal Rust API | **Unstable** | `pub` items in `lib.rs` may change between versions |

### Stability Levels

- **Stable**: Will not break between patch versions. Deprecation + migration path before removal.
- **Provisional**: Additive changes only (new fields, new endpoints). No removals without deprecation.
- **Unstable**: May change at any time. Use at your own risk.

---

## Backward Compatibility Guarantees

### What We Guarantee (v0.3+)

1. **Existing AetherShell syntax will continue to parse.** Scripts written for v0.3 will parse in future versions.

2. **Builtin return types are stable.** If `sys.hostname()` returns `Value::Str`, it will always return `Value::Str`. If `ls(".")` returns `Value::Array(Vec<Value::Record>)`, the schema will only gain new fields — never lose or rename existing ones.

3. **Pipeline semantics are stable.** `|`, `map`, `where`, `reduce`, `select` will behave the same way.

4. **CLI interface is stable.** `ae -c 'expr'`, `ae --tui`, `ae script.ae` will continue to work.

5. **Agent API is additive.** New endpoints and response fields may be added; existing ones won’t be removed or renamed without a deprecation period.

### What May Change (v0.x)

1. **New builtins and modules** may be added at any time.
2. **Internal Rust API** (`pub mod` items in `lib.rs`) may change signatures.
3. **Error messages** may be improved or reformatted.
4. **Performance characteristics** may change (generally: improve).
5. **TUI layout and key bindings** are not yet frozen.

---

## Deprecation Policy

When a feature must change or be removed:

1. **Announce** in the CHANGELOG and release notes at least one minor version before removal.
2. **Emit a runtime warning** when the deprecated feature is used.
3. **Provide a migration path** — document the replacement and ideally provide automated migration.
4. **Remove** in the next minor version after the deprecation period.

---

## Versioning Scheme

AetherShell follows [Semantic Versioning 2.0](https://semver.org/):

- **MAJOR** (x.0.0): Breaking changes to the shell language or public API
- **MINOR** (0.x.0): New features, new builtins, new modules (backward-compatible)
- **PATCH** (0.0.x): Bug fixes, performance improvements, documentation

---

## v1.0.0 Release Criteria

| Criterion | Status | Notes |
|-----------|--------|-------|
| Language syntax frozen | ✅ | No syntax changes planned |
| Builtin return types documented | ✅ | All builtins return typed Values |
| Comprehensive rustdoc | ✅ | All `pub mod` items documented |
| Migration guide | ✅ | Bash/Zsh/PowerShell → AetherShell |
| Stability policy | ✅ | This document |
| Test coverage > 95% intent | ✅ | 1,237+ tests, 100% pass rate |
| Security audit | ⬜ | Third-party audit planned |
| Performance benchmarks | ✅ | 5 benchmark suites |
| Cross-platform CI | ✅ | Linux + Windows + macOS |
| API documentation | ✅ | rustdoc on all public modules |
| Deprecation policy defined | ✅ | See above |
| LTS commitment | ⬜ | Post-1.0 |

---

## Long-Term Support (LTS) Plan

After v1.0.0:

- **LTS releases** will be tagged every 6 months (1.0, 1.6, 2.0, etc.)
- **LTS branches** receive bug fixes and security patches for 18 months
- **Non-LTS releases** receive patches until the next minor release
- **Breaking changes** (2.0, 3.0) will follow a 6-month deprecation cycle

---

## Security Policy

- **Vulnerability reports**: SECURITY.md or email security@nervosys.ai
- **Response time**: Critical vulnerabilities acknowledged within 48 hours
- **Patch releases**: Security fixes released to latest + all active LTS branches
- **Dependency audits**: `cargo audit` runs in CI on every push

---

## How to Check Your Version

```ae
sys.version()
# → "0.3.1"
```

```bash
ae --version
# → ae 0.3.1
```

---

*AetherShell — one language, every platform, deterministic typed output.*
