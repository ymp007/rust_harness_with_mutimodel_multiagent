[CmdletBinding()]
param([switch]$Install)
$ErrorActionPreference = 'Stop'
Push-Location -LiteralPath $PSScriptRoot
try {
    if ($Install) {
        & cargo install --path crates/y-cli --bin ycode --locked --force
        if ($LASTEXITCODE -ne 0) { throw 'ycode installation failed' }
        $commandDir = Join-Path $env:USERPROFILE '.cargo\bin'
    } else {
        & cargo build --release -p y-cli --bin ycode --locked
        if ($LASTEXITCODE -ne 0) { throw 'ycode release build failed' }
        $commandDir = Join-Path $PSScriptRoot 'target\release'
    }
    $env:PATH = $commandDir + [IO.Path]::PathSeparator + $env:PATH
    Write-Host 'Run ycode in your project. Set OPENROUTER_API_KEY first or use a project .env.'
    & ycode --version
    if ($LASTEXITCODE -ne 0) { throw 'ycode verification failed' }
} finally { Pop-Location }
