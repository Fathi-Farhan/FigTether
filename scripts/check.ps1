#!/usr/bin/env pwsh
<#
.SYNOPSIS
    Run the local quality gate for FigTether.

.DESCRIPTION
    The PowerShell twin of scripts/check.sh. It covers Rust formatting, Rust
    lint, Rust tests, the documentation format check, and the em dash scan.

.EXAMPLE
    pwsh -File scripts/check.ps1
#>

$ErrorActionPreference = 'Stop'

$Root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Set-Location $Root

function Write-Step {
    param([string]$Message)
    Write-Host "==> $Message"
}

function Invoke-Native {
    param(
        [Parameter(Mandatory = $true)][string]$FilePath,
        [string[]]$Arguments = @()
    )
    # Many tools write progress to stderr. That output is not an error, so the
    # preference is relaxed for the call and the exit code is checked instead.
    $previous = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        & $FilePath @Arguments
        $code = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previous
    }
    if ($code -ne 0) {
        throw "$FilePath failed with exit code $code"
    }
}

Write-Step 'cargo fmt --all --check'
Invoke-Native -FilePath 'cargo' -Arguments @('fmt', '--all', '--check')

Write-Step 'cargo clippy --workspace --all-targets -- -D warnings'
Invoke-Native -FilePath 'cargo' -Arguments @('clippy', '--workspace', '--all-targets', '--', '-D', 'warnings')

Write-Step 'cargo test --workspace'
Invoke-Native -FilePath 'cargo' -Arguments @('test', '--workspace')

$npm = Get-Command npm -ErrorAction SilentlyContinue
if ($null -ne $npm) {
    if (-not (Test-Path 'node_modules')) {
        Write-Step 'npm ci'
        Invoke-Native -FilePath $npm.Source -Arguments @('ci')
    }
    Write-Step 'npm run format:check'
    Invoke-Native -FilePath $npm.Source -Arguments @('run', 'format:check')
}
else {
    Write-Host '==> npm was not found on PATH, the documentation format check is skipped'
}

Write-Step 'em dash scan'
Invoke-Native -FilePath (Join-Path $PSScriptRoot 'scan-em-dash.ps1')

Write-Host 'all checks passed'
