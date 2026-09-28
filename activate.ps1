# Y Code Harness Activation Script
[CmdletBinding()]
param (
    [switch]$NoBrowser,
    [switch]$Benchmark
)

$ErrorActionPreference = "Stop"

Write-Host "=========================================================================" -ForegroundColor Cyan
Write-Host "           Y CODE - ULTRA-LOW-OVERHEAD AGENT HARNESS ACTIVATION          " -ForegroundColor Cyan
Write-Host "=========================================================================" -ForegroundColor Cyan

# Add Cargo to PATH if needed
$cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
if (Test-Path $cargoBin) {
    if ($env:PATH -notlike "*$cargoBin*") {
        $env:PATH = $cargoBin + ";" + $env:PATH
    }
}

$daemonExe = Join-Path $PSScriptRoot "target\release\y-daemon.exe"
$cliExe = Join-Path $PSScriptRoot "target\release\y-cli.exe"

if (-not (Test-Path $daemonExe) -or -not (Test-Path $cliExe)) {
    Write-Host "Building release binaries (first-time setup)..." -ForegroundColor Yellow
    cargo build --release --bin y-daemon --bin y-cli
}

# Check if y-daemon is already running
$runningDaemon = Get-Process -Name "y-daemon" -ErrorAction SilentlyContinue

if (-not $runningDaemon) {
    Write-Host "Spawning resident background daemon (y-daemon)..." -ForegroundColor Green
    Start-Process -FilePath $daemonExe -WorkingDirectory $PSScriptRoot -WindowStyle Hidden
    Start-Sleep -Milliseconds 600
} else {
    Write-Host "Resident y-daemon is already active (PID: $($runningDaemon.Id))." -ForegroundColor Green
}

# Check health endpoint
try {
    $status = Invoke-RestMethod -Uri "http://127.0.0.1:4040/api/status" -TimeoutSec 2
    Write-Host "Daemon Status:        ONLINE" -ForegroundColor Green
    Write-Host "   WorkingSet:        $($status.working_set_mb) MB (SLA: < 10.0 MB)"
    Write-Host "   Harness Overhead:  $($status.overhead_ms) ms (SLA: < 1.20 ms)"
    Write-Host "   Windows Pipe:      \\.\pipe\y_ipc"
    Write-Host "   Web Dashboard:     http://127.0.0.1:4040/"
} catch {
    Write-Host "Daemon is initializing on pipe and web port..." -ForegroundColor Yellow
}

if (-not $NoBrowser) {
    Write-Host "Launching UI Dashboard in browser (http://127.0.0.1:4040/)..." -ForegroundColor Cyan
    Start-Process "http://127.0.0.1:4040/"
}

if ($Benchmark) {
    Write-Host ""
    Write-Host "Executing 3 benchmark turns..." -ForegroundColor Magenta
    & $cliExe "Benchmark turn 1"
    & $cliExe "Benchmark turn 2"
    & $cliExe "Benchmark turn 3"
}

Write-Host ""
Write-Host "[Y CODE READY] Dispatch turns anytime via CLI:" -ForegroundColor Green
Write-Host "  .\target\release\y-cli.exe `"Your prompt here`"" -ForegroundColor Gray
Write-Host "  .\target\release\y-cli.exe ui" -ForegroundColor Gray
Write-Host ""
