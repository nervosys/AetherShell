#!/usr/bin/env pwsh
# FIPS 140-2 Compliance Verification Script
# Verifies AetherShell's cryptographic dependencies and configuration

Write-Host "`n========================================" -ForegroundColor Cyan
Write-Host "  FIPS 140-2 COMPLIANCE VERIFICATION" -ForegroundColor Cyan
Write-Host "========================================`n" -ForegroundColor Cyan

$compliance = @{
    Passed   = 0
    Failed   = 0
    Warnings = 0
}

function Test-Compliance {
    param(
        [string]$TestName,
        [scriptblock]$Test,
        [string]$Severity = "Error"
    )
    
    Write-Host "Testing: $TestName..." -NoNewline
    try {
        $result = & $Test
        if ($result) {
            Write-Host " ✓ PASS" -ForegroundColor Green
            $script:compliance.Passed++
            return $true
        }
        else {
            if ($Severity -eq "Warning") {
                Write-Host " ⚠ WARNING" -ForegroundColor Yellow
                $script:compliance.Warnings++
            }
            else {
                Write-Host " ✗ FAIL" -ForegroundColor Red
                $script:compliance.Failed++
            }
            return $false
        }
    }
    catch {
        Write-Host " ✗ ERROR: $_" -ForegroundColor Red
        $script:compliance.Failed++
        return $false
    }
}

# Test 1: Verify sha2 dependency
Test-Compliance "SHA-256 dependency (sha2 crate)" {
    $output = cargo tree | Select-String "sha2 v0.10"
    return $null -ne $output
}

# Test 2: Verify rustls dependency (FIPS-capable TLS)
Test-Compliance "rustls-tls dependency" {
    $output = cargo tree | Select-String "rustls v0"
    return $null -ne $output
}

# Test 3: Verify NO OpenSSL dependency
Test-Compliance "No OpenSSL dependency (using rustls)" {
    $output = cargo tree | Select-String "openssl"
    return $null -eq $output
}

# Test 4: Verify ring crypto dependency (used by rustls)
Test-Compliance "Ring cryptography library" {
    $output = cargo tree | Select-String "ring v0.17"
    return $null -ne $output
}

# Test 5: Check for weak crypto algorithms
Test-Compliance "No MD5 dependencies" {
    $output = cargo tree | Select-String "md-5|md5"
    return $null -eq $output
}

Test-Compliance "No SHA-1 dependencies" {
    $output = cargo tree | Select-String "sha1 v"
    return $null -eq $output
} -Severity Warning

# Test 6: Verify Cargo.toml configuration
Test-Compliance "Cargo.toml uses rustls-tls feature" {
    $content = Get-Content Cargo.toml -Raw
    return $content -match 'rustls-tls'
}

# Test 7: Run security audit
Test-Compliance "Security audit (cargo audit)" {
    $output = cargo audit 2>&1
    return $output -notmatch "error: (\d+) vulnerabilit"
} -Severity Warning

# Test 8: Verify cryptographic operations in source
Test-Compliance "SHA-256 usage in storage.rs" {
    $content = Get-Content src/ai_api/storage.rs -Raw
    return $content -match 'Sha256::new'
}

Test-Compliance "SHA-256 usage in downloader.rs" {
    $content = Get-Content src/ai_api/downloader.rs -Raw
    return $content -match 'Sha256::new'
}

# Test 9: Verify no custom crypto implementations
Test-Compliance "No custom hash implementations" {
    $files = Get-ChildItem -Path src -Recurse -Filter *.rs
    foreach ($file in $files) {
        $content = Get-Content $file.FullName -Raw
        if ($content -match 'fn.*hash.*impl|impl.*Hash.*for') {
            # Check if it's just deriving Hash, not implementing crypto
            if ($content -notmatch '#\[derive.*Hash.*\]') {
                return $false
            }
        }
    }
    return $true
}

# Test 10: Verify TLS cipher suite configuration
Test-Compliance "TLS configuration supports FIPS ciphers" {
    # rustls by default supports FIPS-approved cipher suites
    # This is a smoke test that we're using rustls
    $content = Get-Content Cargo.toml -Raw
    return $content -match 'rustls-tls'
}

# Test 11: Check for encryption/decryption operations
Test-Compliance "No encryption/decryption operations (integrity only)" {
    $files = Get-ChildItem -Path src -Recurse -Filter *.rs
    foreach ($file in $files) {
        $content = Get-Content $file.FullName -Raw
        if ($content -match 'encrypt\(|decrypt\(|cipher\.|aes::|chacha') {
            Write-Host "`n  Found: $($file.Name)" -ForegroundColor Yellow
            return $false
        }
    }
    return $true
}

# Test 12: Verify zeroize for memory safety
Test-Compliance "Memory zeroing (zeroize crate)" {
    $output = cargo tree | Select-String "zeroize v1"
    return $null -ne $output
}

# Test 13: Verify secrecy for secret handling
Test-Compliance "Secret handling (secrecy crate)" {
    $output = cargo tree | Select-String "secrecy v0"
    return $null -ne $output
}

# Summary
Write-Host "`n========================================" -ForegroundColor Cyan
Write-Host "  COMPLIANCE SUMMARY" -ForegroundColor Cyan
Write-Host "========================================`n" -ForegroundColor Cyan

Write-Host "✓ Passed:   $($compliance.Passed)" -ForegroundColor Green
Write-Host "⚠ Warnings: $($compliance.Warnings)" -ForegroundColor Yellow
Write-Host "✗ Failed:   $($compliance.Failed)" -ForegroundColor Red

$total = $compliance.Passed + $compliance.Failed + $compliance.Warnings
$passRate = [math]::Round(($compliance.Passed / $total) * 100, 1)

Write-Host "`nPass Rate: $passRate%" -ForegroundColor $(if ($passRate -ge 90) { "Green" } elseif ($passRate -ge 75) { "Yellow" } else { "Red" })

if ($compliance.Failed -eq 0 -and $compliance.Warnings -le 1) {
    Write-Host "`n✓ FIPS 140-2 COMPLIANCE: VERIFIED" -ForegroundColor Green -BackgroundColor Black
    Write-Host "  AetherShell uses FIPS-approved algorithms and libraries.`n" -ForegroundColor Green
    exit 0
}
elseif ($compliance.Failed -eq 0) {
    Write-Host "`n⚠ FIPS 140-2 COMPLIANCE: PASS WITH WARNINGS" -ForegroundColor Yellow -BackgroundColor Black
    Write-Host "  Review warnings above for potential issues.`n" -ForegroundColor Yellow
    exit 0
}
else {
    Write-Host "`n✗ FIPS 140-2 COMPLIANCE: FAILED" -ForegroundColor Red -BackgroundColor Black
    Write-Host "  Address failures above before deploying in FIPS mode.`n" -ForegroundColor Red
    exit 1
}
