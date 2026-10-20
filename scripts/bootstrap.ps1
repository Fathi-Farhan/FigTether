#!/usr/bin/env pwsh
<#
.SYNOPSIS
    Prepare a local FigTether development environment.

.DESCRIPTION
    The PowerShell twin of scripts/bootstrap.sh. It installs the rustfmt and
    clippy components and the documentation tooling. It works with Windows
    PowerShell 5.1 and with PowerShell 7 or newer.

.EXAMPLE
    pwsh -File scripts/bootstrap.ps1
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

Write-Step "repository root: $Root"

Write-Step "rustup components (rustfmt, clippy)"
$rustup = Get-Command rustup -ErrorAction SilentlyContinue
if ($null -ne $rustup) {
    Invoke-Native -FilePath $rustup.Source -Arguments @('component', 'add', 'rustfmt', 'clippy')
    Invoke-Native -FilePath $rustup.Source -Arguments @('show')
}
else {
    Write-Host "rustup was not found on PATH, skipping the toolchain setup"
    Write-Host "install it from https://rustup.rs and rerun this script"
}

Write-Step "documentation tooling (npm ci)"
$npm = Get-Command npm -ErrorAction SilentlyContinue
if ($null -ne $npm) {
    Invoke-Native -FilePath $npm.Source -Arguments @('ci')
}
else {
    Write-Host "npm was not found on PATH, skipping the documentation tooling"
    Write-Host "install Node.js LTS from https://nodejs.org and rerun this script"
}

Write-Step "optional release tooling"
Write-Host "the audit, SBOM, and release tasks need these tools:"
Write-Host "  cargo install cargo-deny cargo-audit cargo-cyclonedx"

Write-Host "bootstrap finished"
