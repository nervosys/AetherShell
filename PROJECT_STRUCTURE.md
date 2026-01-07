# AetherShell - Project Organization

This document describes the organized structure of the AetherShell project.

## 📁 Directory Structure

```
AetherShell/
├── 📁 src/                     # Source code
│   ├── 📁 ai_api/             # AI integration modules
│   ├── 📁 bin/                # Binary crate modules
│   ├── 📁 transpile/          # Code transpilation modules
│   ├── 📁 tui/                # Terminal UI modules
│   └── 📄 *.rs                # Core Rust modules
├── 📁 tests/                   # Unit and integration tests
├── 📁 examples/               # AetherShell example scripts
├── 📁 test-scripts/           # Test scripts for manual testing
│   ├── 📁 builtins/          # Builtin function tests
│   └── 📁 integration/       # Integration test scripts
├── 📁 benches/                # Performance benchmarks
├── 📁 docs/                   # Documentation
│   ├── 📁 specs/             # Technical specifications
│   ├── 📁 dev-notes/         # Development notes and fixes
│   ├── 📁 security/          # Security documentation
│   ├── 📁 release/           # Release and launch documentation
│   ├── 📄 TUI_GUIDE.md       # TUI usage guide
│   ├── 📄 IMMUTABILITY_IMPLEMENTATION.md  # Immutability features
│   ├── 📄 LIVE_DEMO.md       # Live demo documentation
│   └── 📄 README_*.md        # Various README versions
├── 📁 scripts/                # Utility scripts
│   ├── 📄 security_audit.py  # Security audit script
│   └── 📄 verify_fips_compliance.ps1  # FIPS compliance verification
├── 📁 web/                    # Web terminal components
├── 📁 .github/                # GitHub workflows and templates
├── 📄 README.md               # Main project README
├── 📄 Cargo.toml              # Rust project configuration
├── 📄 CHANGELOG.md            # Project changelog
├── 📄 CONTRIBUTING.md         # Contribution guidelines
├── 📄 LICENSE                 # Project license
└── 📄 deny.toml               # Dependency security configuration
```

## 🎯 File Organization Principles

### Source Code (`src/`)
- **Core modules**: Direct `.rs` files in `src/` for fundamental shell functionality
- **AI integration**: `ai_api/` contains all AI-related functionality
- **Binary tools**: `bin/` contains additional binary utilities
- **Transpilation**: `transpile/` handles code conversion between shell languages
- **TUI components**: `tui/` contains terminal user interface code

### Testing Structure
- **`tests/`**: Rust unit and integration tests (run with `cargo test`)
- **`test-scripts/`**: Manual test scripts written in AetherShell
  - `builtins/`: Tests for individual builtin commands
  - `integration/`: Complex integration test scenarios

### Documentation (`docs/`)
- **`specs/`**: Technical specifications and architecture documents
- **`dev-notes/`**: Development notes, fixes, and troubleshooting guides
- **Root docs**: User-facing guides and tutorials

### Examples and Demos
- **`examples/`**: Educational examples showing AetherShell features
- **`demos/`**: Showcase scripts demonstrating advanced capabilities

### Temporary Files (`temp/`)
- Temporary files, debug artifacts, and build outputs
- This directory is gitignored

## 🛠️ Development Workflow

1. **Code changes**: Work in `src/` directory
2. **Testing**: Run `cargo test` for Rust tests, use scripts in `test-scripts/` for manual testing
3. **Documentation**: Update relevant files in `docs/` when adding features
4. **Examples**: Add usage examples to `examples/` for new features

## 📝 File Naming Conventions

- **Rust files**: `snake_case.rs`
- **AetherShell scripts**: `descriptive_name.ae`
- **Documentation**: `UPPERCASE.md` for specs, `Title_Case.md` for guides
- **Test scripts**: `test_feature_name.ae`

This organization follows Rust project conventions while maintaining clear separation of concerns for the various components of AetherShell.