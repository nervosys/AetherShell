# AetherShell Security Audit Exclusions
# This file documents legitimate security findings that are acceptable

## Unsafe Code Blocks
- **src/agent.rs:373,420**: Legitimate unsafe blocks for system-level resource limiting (sandboxing)
  - Used exclusively for security hardening (CPU/memory limits)
  - Platform-specific (Linux/macOS) resource control
  - Critical for preventing resource exhaustion attacks
  - Properly documented and contained

## Critical Unwrap/Expect Usage
- **src/ai.rs:1047**: HTTP client creation in lazy_static
  - This is a fatal system configuration error if it fails
  - Happens at startup only, not during runtime operations
  - Properly documented as FATAL error

## Documentation Examples (False Positives)
All "secret" findings in documentation are example placeholders:
- README.md: "your-api-key", "sk-..." examples
- docs/*.md: All API key examples are clearly marked as placeholders
- These are necessary for user documentation

## Test Data (False Positives)
- tests/ai_mcp.rs: Test JSON with "key": "value" - not actual secrets
- src/agent.rs:562: Test arguments with generic key-value pairs

## Security Compliance Status: ✅ APPROVED

The identified security findings have been reviewed and are either:
1. Necessary for security functionality (unsafe sandboxing code)
2. Properly handled fatal errors (HTTP client initialization)  
3. Documentation examples clearly marked as placeholders
4. Test data with no sensitive content

All findings are documented, justified, and do not pose actual security risks.

**Review Date**: November 4, 2025
**Next Review**: December 4, 2025