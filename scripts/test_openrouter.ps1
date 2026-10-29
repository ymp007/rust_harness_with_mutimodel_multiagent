[CmdletBinding()]
param(
    [string]$Model = '',
    [string]$Prompt = 'Say hello in one sentence.',
    [switch]$Agent,
    [switch]$Stream
)
$ErrorActionPreference = 'Stop'
$RepoRoot = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $RepoRoot
try {
    # Always rebuild so a stale binary cannot give a false smoke-test result.
    & cargo build --release -p y-cli --bin ycode --locked
    if ($LASTEXITCODE -ne 0) { throw 'ycode release build failed' }
    $cli = Join-Path $RepoRoot 'target\release\ycode.exe'
    $modelArgs = @()
    if ($Model) { $modelArgs = @('--model', $Model) }
    & $cli doctor @modelArgs
    if ($LASTEXITCODE -ne 0) { throw 'Set OPENROUTER_API_KEY in your environment or .env; doctor failed' }
    & $cli ai @modelArgs $Prompt
    if ($LASTEXITCODE -ne 0) { throw 'Plain chat failed (401=key, 402=credits, 429=rate limit)' }
    & $cli ai @modelArgs --stream $Prompt
    if ($LASTEXITCODE -ne 0) { throw 'Streaming chat failed' }
    if ($Agent) {
        & $cli -p --read-only @modelArgs 'Use read_file to read README.md, then explain the actual install command in two sentences.'
        if ($LASTEXITCODE -ne 0) { throw 'Coding-agent source inspection failed' }
    }
    Write-Host 'OpenRouter smoke tests passed. Run ycode to test interactive edits and command approvals.'
} finally { Pop-Location }
