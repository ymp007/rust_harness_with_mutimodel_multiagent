# ============================================================================
# scripts/run_all_benchmarks.ps1
# Master Automated Systems Benchmark & Verification Runner for Y Code
# PowerShell 5.1 / 7+ compatible. Parity with scripts/run_all_benchmarks.sh
#
# Usage:
#   powershell -ExecutionPolicy Bypass -File .\scripts\run_all_benchmarks.ps1
#   powershell -ExecutionPolicy Bypass -File .\scripts\run_all_benchmarks.ps1 -SkipRelease
#   powershell -ExecutionPolicy Bypass -File .\scripts\run_all_benchmarks.ps1 -SkipTests -SkipRelease -SkipBench
# ============================================================================

[CmdletBinding()]
param(
    [switch]$SkipFormat,
    [switch]$SkipTests,
    [switch]$SkipRelease,
    [switch]$SkipBench
)

$ErrorActionPreference = "Stop"

# Repo root = parent of scripts/ (handles space in "harness project")
$RepoRoot = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $RepoRoot
try {
    # Ensure cargo on PATH
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        $cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
        if (Test-Path -LiteralPath $cargoBin) {
            $env:PATH = "$cargoBin;$env:PATH"
        }
    }
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        throw "cargo not found. Install Rust via https://rustup.rs/"
    }

    Write-Host "==========================================================================" -ForegroundColor Cyan
    Write-Host "          STARTING Y CODE MASTER SYSTEMS VERIFICATION" -ForegroundColor Cyan
    Write-Host "==========================================================================" -ForegroundColor Cyan
    Write-Host "Repo: $RepoRoot" -ForegroundColor Gray

    function Invoke-Step([string]$Label, [string[]]$Cmd) {
        Write-Host ""
        Write-Host $Label -ForegroundColor Yellow
        Write-Host "  > $($Cmd -join ' ')" -ForegroundColor Gray
        & $Cmd[0] @($Cmd | Select-Object -Skip 1)
        if ($LASTEXITCODE -ne 0) {
            throw "FAILED ($LASTEXITCODE): $($Cmd -join ' ')"
        }
    }

    if (-not $SkipFormat) {
        Invoke-Step "[1/5] Format check..." @("cargo", "fmt", "--all", "--", "--check")
        Invoke-Step "[2/5] Clippy (deny warnings)..." @("cargo", "clippy", "--workspace", "--all-targets", "--all-features", "--locked", "--", "-D", "warnings")
    } else {
        Write-Host "[1-2/5] Skipped (format/clippy)." -ForegroundColor Gray
    }

    if (-not $SkipTests) {
        Invoke-Step "[3/5] Test matrix..." @("cargo", "test", "--workspace", "--all-features", "--locked")
    } else {
        Write-Host "[3/5] Skipped (tests)." -ForegroundColor Gray
    }

    if (-not $SkipRelease) {
        Invoke-Step "[4/5] Release build (LTO=fat, may take 10-20 min)..." @("cargo", "build", "--release", "--workspace", "--locked")
    } else {
        Write-Host "[4/5] Skipped (release build)." -ForegroundColor Gray
    }

    if (-not $SkipBench) {
        Write-Host ""
        Write-Host "[5/5] Guardrails benchmark (release build required)..." -ForegroundColor Yellow
        $benchExe = Join-Path $RepoRoot "target\release\y-bench.exe"
        $benchBin = Join-Path $RepoRoot "target/release/y-bench"
        if (Test-Path -LiteralPath $benchExe) {
            & $benchExe
        } elseif (Test-Path -LiteralPath $benchBin) {
            & $benchBin
        } else {
            throw "Benchmark binary not found. Run without -SkipRelease first: $benchExe"
        }
        if ($LASTEXITCODE -ne 0) {
            throw "y-bench FAILED ($LASTEXITCODE): at least one guardrail breached its threshold"
        }
    } else {
        Write-Host "[5/5] Skipped (bench)." -ForegroundColor Gray
    }

    Write-Host ""
    Write-Host "==========================================================================" -ForegroundColor Green
    Write-Host "          Y CODE VERIFICATION COMPLETE: ALL GATES PASSED" -ForegroundColor Green
    Write-Host "==========================================================================" -ForegroundColor Green
}
finally {
    Pop-Location
}
