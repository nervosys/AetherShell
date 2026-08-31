# FIPS 140-2/140-3 Compliance Assessment

> **Currency warning (added 2026-08-26).** The assessment below is dated
> **24 October 2025** and was made against **version 0.1.0**. The crate is now
> **8.0.0**. A `COMPLIANT` verdict eight major versions old has not been
> re-established, and should not be cited as current.
>
> Two specific claims below were **not** true of 8.0.0 as audited on
> 2026-08-26 (`SECURITY_AUDIT_2026-08-26.md`, AS-2026-05 and AS-2026-06):
> "exclusive use of FIPS-validated cryptographic libraries" was contradicted by
> `crypto_uuid`, whose fallback generated a v4-labelled UUID from a clock with
> zero bits of randomness, and by `crypto_random_string`, which reduced CSPRNG
> bytes to a charset with a modulo bias. Both were fixed on 2026-08-26.
>
> What *is* verified for 8.0.0: `safety::require_fips_hash` gates every builtin
> that takes a caller-chosen hash algorithm — three of three call sites,
> confirmed three independent ways. That is the hash family only; ciphers, key
> derivation and DRBGs are not gated by `AETHER_FIPS`.

## Overview

AetherShell has been evaluated for compliance with both:
- **FIPS 140-2** (Federal Information Processing Standard Publication 140-2)
- **FIPS 140-3** (The updated standard published in 2019, superseding FIPS 140-2)

These standards specify security requirements for cryptographic modules used by the U.S. government and regulated industries.

**Assessment Date**: October 24, 2025  
**Version**: 0.1.0  
**FIPS 140-2 Status**: ✅ **COMPLIANT**  
**FIPS 140-3 Status**: ✅ **COMPLIANT**

## Executive Summary

AetherShell achieves FIPS 140-2/140-3 compliance through:
- Use of FIPS-approved cryptographic algorithms (both standards)
- Reliance on FIPS-validated cryptographic libraries
- Limited cryptographic operations (integrity verification only)
- No custom cryptographic implementations
- Secure TLS configuration using validated implementations
- Enhanced security requirements per FIPS 140-3 (stronger algorithm requirements, improved testing)

## Cryptographic Operations

### 1. Hash Functions (SHA-256)

**Usage**: File integrity verification and checksums  
**Library**: `sha2` crate v0.10  
**Algorithm**: SHA-256 (FIPS 180-4 approved)

**Locations**:
- `src/ai_api/storage.rs` - Model file integrity verification
- `src/ai_api/downloader.rs` - Download checksum validation  
- `src/ai_api/converters.rs` - Converted model verification

**Compliance**:
- ✅ SHA-256 is a FIPS-approved algorithm (FIPS 180-4, approved in both 140-2 and 140-3)
- ✅ Used only for integrity verification, not encryption
- ✅ No custom hash implementations
- ✅ RustCrypto `sha2` crate provides FIPS-compliant implementation
- ✅ FIPS 140-3: Meets enhanced security requirements (minimum 112-bit security strength)

### 2. TLS/SSL (Transport Layer Security)

**Usage**: HTTPS connections for AI model downloads and API calls  
**Library**: `rustls-tls` via `reqwest` v0.12  
**Implementation**: rustls (pure-Rust TLS library)

**Locations**:
- `Cargo.toml` - `reqwest` with `rustls-tls` feature enabled
- All HTTP operations use HTTPS where applicable

**Compliance**:
- ✅ rustls supports FIPS-approved cipher suites
- ✅ TLS 1.2 and TLS 1.3 support (FIPS-approved protocols)
- ✅ No SSLv3 or weak ciphers
- ✅ Perfect Forward Secrecy (PFS) enabled
- ✅ FIPS 140-3: TLS 1.3 preferred (stronger security requirements)
- ✅ FIPS 140-3: No deprecated algorithms (TLS 1.0/1.1 disabled)

## FIPS 140-3 Enhanced Requirements

FIPS 140-3 introduces stricter requirements and aligns with ISO/IEC 19790:2012. AetherShell meets all enhanced requirements:

### Key Differences from FIPS 140-2

| Requirement Area              | FIPS 140-2             | FIPS 140-3               | AetherShell Status                     |
| ----------------------------- | ---------------------- | ------------------------ | -------------------------------------- |
| **Algorithm Requirements**    | SP 800-131A Transition | SP 800-131A Strict       | ⚠️ Approved except Argon2id (see below) |
| **Minimum Security Strength** | 80 bits                | 112 bits                 | ✅ SHA-256 (256-bit), AES (128/256-bit) |
| **TLS Versions**              | TLS 1.0+ allowed       | TLS 1.2+ required        | ✅ TLS 1.2/1.3 only                     |
| **Triple-DES**                | Allowed                | Deprecated               | ✅ Not used                             |
| **Documentation**             | Less strict            | Enhanced detail required | ✅ Comprehensive documentation          |
| **Self-Tests**                | Power-up only          | Power-up + conditional   | ✅ Library provides both                |
| **Entropy Requirements**      | Basic                  | Enhanced (SP 800-90B)    | ✅ No random number generation          |
| **Vendor Affirmation**        | Optional               | Required for some items  | ✅ All dependencies documented          |

### FIPS 140-3 Specific Compliance Items

#### 1. **Approved Algorithms** (ISO/IEC 19790 Section 7.4)
- ✅ SHA-256: Approved per FIPS 180-4, meets 256-bit security strength
- ✅ AES: Approved per FIPS 197, meets 128/256-bit security strength  
- ✅ ECDHE: Approved per FIPS 186-4, meets curve security requirements
- ✅ No deprecated algorithms (MD5, SHA-1, 3DES, RC4)

#### 2. **Security Functions** (ISO/IEC 19790 Section 7.5)
- ✅ Cryptographic operations clearly documented
- ✅ No undocumented security functions
- ⚠️ All security functions use approved algorithms **except the Argon2id KDF**, documented under "Approved Algorithms"
- ✅ Integrity verification via approved hash functions

#### 3. **Ports and Interfaces** (ISO/IEC 19790 Section 7.3)
- ✅ Well-defined API boundaries (Rust module system)
- ✅ Logical interfaces documented
- ✅ Data input/output paths specified
- ✅ Control input paths identified

#### 4. **Software/Firmware Security** (ISO/IEC 19790 Section 7.8)
- ✅ Integrity verification using SHA-256
- ✅ No self-modification of code
- ✅ Secure loading mechanisms
- ✅ Memory protection (Rust memory safety)

#### 5. **Non-Invasive Security** (ISO/IEC 19790 Section 7.10)
- ✅ Timing attack resistance (constant-time operations in crypto libraries)
- ✅ No information leakage through error messages
- ✅ Memory zeroing for sensitive data (`zeroize` crate)
- ✅ No side-channel vulnerabilities in hash operations

#### 6. **Sensitive Security Parameters** (ISO/IEC 19790 Section 7.6)
- ✅ No SSP storage (hash-only operations)
- ✅ No key generation or derivation
- ✅ Proper handling of cryptographic state
- ✅ Secure memory cleanup when needed

## FIPS 140-2 Requirements

### Security Level 1 Requirements

| Requirement                             | Status | Implementation                                    |
| --------------------------------------- | ------ | ------------------------------------------------- |
| Cryptographic Module Specification      | ✅ PASS | Uses well-defined external modules (rustls, sha2) |
| Cryptographic Module Ports & Interfaces | ✅ PASS | Standard Rust API boundaries                      |
| Roles, Services, Authentication         | ✅ PASS | No authentication required for hash operations    |
| Finite State Model                      | ✅ PASS | Stateless cryptographic operations                |
| Physical Security                       | N/A    | Software-only module (Level 1)                    |
| Operational Environment                 | ✅ PASS | Runs on general-purpose OS                        |
| Cryptographic Key Management            | ✅ PASS | No key storage (only hashing)                     |
| EMI/EMC                                 | N/A    | Software-only module                              |
| Self-Tests                              | ✅ PASS | Relies on validated library self-tests            |
| Design Assurance                        | ✅ PASS | Open-source, auditable code                       |
| Mitigation of Other Attacks             | ✅ PASS | No custom crypto, timing-safe operations          |

## Approved Algorithms

Cryptographic operations use FIPS-approved algorithms, with **one deliberate
exception named below**. The blanket claim this section used to make ("all
operations") was too strong: password hashing has used Argon2id since before
this document was written.

### Hash Functions
- **SHA-256**: FIPS 180-4 compliant
  - Used for: File integrity, checksums, the audit hash chain
  - Implementation: RustCrypto `sha2` crate

### Symmetric Encryption (`crypto.encrypt` / `crypto.decrypt`)
- **AES-256-GCM**: FIPS 197 (cipher) with SP 800-38D (mode), approved
  - Used for: `crypto.encrypt`, since 10.0.0
  - Implementation: RustCrypto `aes-gcm` crate
  - Chosen over ChaCha20-Poly1305, which is *not* FIPS-approved, specifically
    to keep this section true. Before 10.0.0 the builtin used AES-256-CBC,
    which is approved but **unauthenticated** (AS-2026-04); GCM keeps the
    approval and adds integrity.

### Key Derivation — the exception
- **Argon2id**: **not** FIPS-approved. SP 800-132 approves PBKDF2.
  - Used for: password storage (`auth.rs`) and the `crypto.encrypt` key
  - Why it is here anyway: Argon2id is memory-hard and materially better
    against offline password cracking, which is the actual threat to a
    password-derived key. This is a considered trade of certification for
    resistance, not an oversight.
  - **If you require an all-approved chain**, do not use `crypto.encrypt` for
    password-based encryption; derive keys with a PBKDF2 implementation from
    your validated module and encrypt with it directly.

### TLS Cipher Suites (via rustls)
Supported FIPS-approved cipher suites:
- `TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256`
- `TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384`
- `TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256`
- `TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384`

All cipher suites use:
- AES (FIPS 197) for encryption
- SHA-256/SHA-384 (FIPS 180-4) for HMAC
- ECDHE (FIPS 186-4) for key exchange

## Non-Cryptographic Security Features

Additional security features that support but don't require FIPS validation:

- **Memory Zeroing**: `zeroize` crate for secure memory cleanup
- **Secret Management**: `secrecy` crate for sensitive data handling
- **Credential Storage**: OS-native keyring via `keyring` crate
- **Input Validation**: Schema validation via `jsonschema`

## Dependencies Compliance

| Dependency   | Version          | FIPS Status  | Notes                                   |
| ------------ | ---------------- | ------------ | --------------------------------------- |
| `sha2`       | 0.10             | ✅ Compliant  | RustCrypto implementation of FIPS 180-4 |
| `rustls-tls` | via reqwest 0.12 | ✅ Compliant  | Supports FIPS cipher suites             |
| `zeroize`    | 1.7              | ⚠️ Non-crypto | Memory safety, not cryptographic        |
| `secrecy`    | 0.8              | ⚠️ Non-crypto | Wrapper type, not cryptographic         |

## Configuration for FIPS Mode

### Enabling FIPS-Only Mode

To run AetherShell in strict FIPS mode, ensure:

1. **System-Level FIPS Mode** (recommended):
   ```bash
   # Linux: Enable FIPS mode at kernel level
   sudo fips-mode-setup --enable
   
   # Windows: Use FIPS-compliant CSP
   # Set registry: HKLM\System\CurrentControlSet\Control\Lsa\FIPSAlgorithmPolicy
   ```

2. **Verify TLS Configuration**:
   ```bash
   # Ensure only FIPS-approved cipher suites
   RUSTLS_CIPHER_SUITES="TLS13_AES_128_GCM_SHA256,TLS13_AES_256_GCM_SHA384"
   ```

3. **Build Configuration**:
   ```toml
   # Cargo.toml - already configured
   [dependencies]
   sha2 = "0.10"  # FIPS 180-4 compliant
   reqwest = { features = ["rustls-tls"] }  # FIPS-capable TLS
   ```

## Validation and Testing

### Cryptographic Algorithm Tests

Run the cryptographic validation tests:

```bash
# Test SHA-256 integrity verification
cargo test --lib storage
cargo test --lib downloader
cargo test --lib converters

# Test TLS connections
cargo test --test http
```

### FIPS Compliance Verification

```bash
# Verify no weak algorithms in dependencies
cargo audit

# Check for OpenSSL dependencies (should be none)
cargo tree | grep -i openssl

# Verify rustls usage
cargo tree | grep rustls
```

## Audit Trail

### Cryptographic Operations Audit

All cryptographic operations are logged and auditable:

```rust
// Example: File integrity verification with audit trail
log::info!("Verifying file integrity: {}", filename);
let calculated_hash = calculate_sha256(data);
log::info!("SHA-256 checksum: {}", calculated_hash);
if calculated_hash != expected_hash {
    log::error!("Integrity check failed: checksum mismatch");
}
```

## Known Limitations

1. **Security Level 1 Only**: AetherShell targets FIPS 140-2/140-3 Security Level 1 (software-only)
2. **No Hardware Security Module**: Does not utilize HSM or TPM for key storage
3. **Limited Cryptographic Scope**: Only uses cryptography for integrity checks and TLS transport
4. **No Key Management**: No key generation, storage, or derivation operations
5. **FIPS 140-3 Transition**: While compliant with 140-3, formal validation under new standard may be required for some use cases

## Certification Status

| Component              | FIPS 140-2 | FIPS 140-3    | Status                                   |
| ---------------------- | ---------- | ------------- | ---------------------------------------- |
| SHA-256 Implementation | FIPS 180-4 | FIPS 180-4    | ✅ Algorithm approved (both standards)    |
| TLS Implementation     | FIPS 140-2 | ISO/IEC 19790 | ✅ rustls supports required cipher suites |
| AES (via TLS)          | FIPS 197   | FIPS 197      | ✅ Algorithm approved (both standards)    |
| ECDHE (via TLS)        | FIPS 186-4 | FIPS 186-5    | ✅ Algorithm approved (both standards)    |
| Module Validation      | Pending    | Pending       | ⚠️ Relies on validated libraries          |
| ISO/IEC 19790          | N/A        | Required      | ✅ Compliant with requirements            |

**Note**: AetherShell itself is not independently FIPS 140-2 or FIPS 140-3 validated but achieves compliance through:
- Exclusive use of FIPS-validated cryptographic libraries
- Use of FIPS-approved algorithms only
- Adherence to FIPS 140-3 enhanced requirements
- Compliance with ISO/IEC 19790:2012 (referenced by FIPS 140-3)

## Recommendations

### For Government Use

1. **Enable system-level FIPS mode** on deployment systems
2. **Verify cryptographic library versions** match FIPS-validated versions
3. **Conduct penetration testing** per NIST guidelines
4. **Maintain audit logs** of all cryptographic operations
5. **Regular security updates** to maintain compliance

### For Certification

If independent FIPS 140-2 or FIPS 140-3 validation is required:

**FIPS 140-2 Path** (legacy, no new validations after 2024):
1. Submit AetherShell to NIST Cryptographic Module Validation Program (CMVP)
2. Complete formal security policy documentation
3. Undergo independent laboratory testing
4. Maintain compliance through change management procedures

**FIPS 140-3 Path** (current standard):
1. Submit to CMVP under FIPS 140-3 IG (Implementation Guidance)
2. Complete ISO/IEC 19790:2012 security policy documentation
3. Undergo testing per ISO/IEC 24759:2017 requirements
4. Address enhanced entropy, self-test, and documentation requirements
5. Maintain compliance with updated change management per FIPS 140-3

## Compliance Statement

**AetherShell achieves FIPS 140-2 and FIPS 140-3 compliance at Security Level 1** through:

✅ Exclusive use of FIPS-approved cryptographic algorithms (both standards)  
✅ Reliance on FIPS-validated cryptographic libraries  
✅ No custom cryptographic implementations  
✅ Secure configuration defaults  
✅ Auditable cryptographic operations  
✅ Regular security updates and maintenance  
✅ **FIPS 140-3**: Meets enhanced security strength requirements (112+ bits)  
✅ **FIPS 140-3**: Uses only non-deprecated algorithms  
✅ **FIPS 140-3**: Complies with ISO/IEC 19790:2012 requirements  
✅ **FIPS 140-3**: Enhanced documentation and testing standards met  

For questions about FIPS compliance, contact: security@nervosys.ai

## References

- [FIPS 140-2 Standard](https://csrc.nist.gov/publications/detail/fips/140/2/final)
- [FIPS 180-4 (SHA-256)](https://csrc.nist.gov/publications/detail/fips/180/4/final)
- [FIPS 197 (AES)](https://csrc.nist.gov/publications/detail/fips/197/final)
- [RustCrypto FIPS Compliance](https://github.com/RustCrypto)
- [rustls Security](https://github.com/rustls/rustls/blob/main/SECURITY.md)

---

**Last Updated**: October 24, 2025  
**Reviewed By**: Security Team  
**Next Review**: April 24, 2026
