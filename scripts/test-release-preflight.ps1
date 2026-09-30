# Test release preflight without creating or pushing any real Git tags.
$ErrorActionPreference = 'Stop'
$releaseScript = Join-Path $PSScriptRoot 'release.ps1'
$testRoot = Join-Path (Split-Path $PSScriptRoot -Parent) ('temp/release-preflight-' + [guid]::NewGuid())
New-Item -ItemType Directory -Path $testRoot | Out-Null
$shellExecutable = (Get-Process -Id $PID).Path
$harnessPath = Join-Path $testRoot 'harness.ps1'
$harness = @'
param([string]$ReleaseScript, [string]$Mode, [switch]$DryRun)
$ErrorActionPreference = 'Stop'
function global:git {
    $global:LASTEXITCODE = 0
    switch ($args[0]) {
        'rev-parse' { if ($Mode -eq 'wrong-repository') { Split-Path (Get-Location).Path -Parent } else { (Get-Location).Path } }
        'branch' { if ($Mode -eq 'branch-failure') { $global:LASTEXITCODE = 2 } else { 'master' } }
        'status' { if ($Mode -eq 'dirty') { ' M Cargo.toml' } }
        'show-ref' { $global:LASTEXITCODE = 1; if ($Mode -eq 'existing-tag') { $global:LASTEXITCODE = 0 } }
        'tag' { Set-Content -LiteralPath 'tag-called' -Value 'yes'; if ($Mode -eq 'tag-failure') { $global:LASTEXITCODE = 2 } }
        'push' { Set-Content -LiteralPath 'push-called' -Value 'yes'; if ($Mode -eq 'push-failure') { $global:LASTEXITCODE = 2 } }
        default { throw "Unexpected git command: $args" }
    }
}
& $ReleaseScript -Version '13.0.0' -DryRun:$DryRun
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
'SCRIPT_RETURNED_SUCCESS'
'@
Set-Content -LiteralPath $harnessPath -Value $harness -Encoding utf8
$cases = @(
    @{ Mode = 'clean'; Version = '13.0.0'; DryRun = $true; Success = $true; Tag = $false; Push = $false },
    @{ Mode = 'wrong-repository'; Version = '13.0.0'; DryRun = $true; Success = $false; Tag = $false; Push = $false },
    @{ Mode = 'clean'; Version = '12.0.2'; DryRun = $true; Success = $false; Tag = $false; Push = $false },
    @{ Mode = 'dirty'; Version = '13.0.0'; DryRun = $true; Success = $false; Tag = $false; Push = $false },
    @{ Mode = 'branch-failure'; Version = '13.0.0'; DryRun = $true; Success = $false; Tag = $false; Push = $false },
    @{ Mode = 'existing-tag'; Version = '13.0.0'; DryRun = $true; Success = $false; Tag = $false; Push = $false },
    @{ Mode = 'tag-failure'; Version = '13.0.0'; DryRun = $false; Success = $false; Tag = $true; Push = $false },
    @{ Mode = 'push-failure'; Version = '13.0.0'; DryRun = $false; Success = $false; Tag = $true; Push = $true }
)
$index = 0
foreach ($case in $cases) {
    $index++
    $fixture = Join-Path $testRoot "case-$index"
    New-Item -ItemType Directory -Path $fixture | Out-Null
    $fixtureScripts = Join-Path $fixture 'scripts'
    New-Item -ItemType Directory -Path $fixtureScripts | Out-Null
    $fixtureRelease = Join-Path $fixtureScripts 'release.ps1'
    Copy-Item -LiteralPath $releaseScript -Destination $fixtureRelease
    # A dependency at the target version must not hide the wrong package version.
    $manifest = "[package]`nname = `"fixture`"`nversion = `"$($case.Version)`"`n[dependencies]`nother = { version = `"13.0.0`" }`n"
    Set-Content -LiteralPath (Join-Path $fixture 'Cargo.toml') -Value $manifest
    Push-Location $fixture
    try {
        $arguments = @('-NoProfile', '-File', $harnessPath, '-ReleaseScript', $fixtureRelease, '-Mode', $case.Mode)
        if ($case.DryRun) { $arguments += '-DryRun' }
        # Windows PowerShell turns expected child stderr into error records.
        # Judge the child by its exit code, then restore strict harness errors.
        $ErrorActionPreference = 'Continue'
        & $shellExecutable @arguments *> (Join-Path $fixture 'output.log')
        $childExit = $LASTEXITCODE
        $ErrorActionPreference = 'Stop'
        $succeeded = $childExit -eq 0
        if ($succeeded -ne $case.Success) { throw "Case $index ($($case.Mode)): unexpected process outcome. See $fixture/output.log" }
        if ((Test-Path -LiteralPath 'tag-called') -ne $case.Tag) { throw "Case ${index}: unexpected tag command." }
        if ((Test-Path -LiteralPath 'push-called') -ne $case.Push) { throw "Case ${index}: unexpected push command." }
        if (-not $succeeded -and (Get-Content -LiteralPath 'output.log' -Raw) -match 'Release v13.0.0 initiated') {
            throw "Case ${index}: failure was reported as a successful release."
        }
    } finally { Pop-Location }
}
Write-Output "$index release preflight checks passed; no real Git commands executed."
