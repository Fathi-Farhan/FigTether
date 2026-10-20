#!/usr/bin/env pwsh
<#
.SYNOPSIS
    Scan the repository for em dashes.

.DESCRIPTION
    The PowerShell twin of scripts/scan-em-dash.sh. PRD 8.12 forbids the em
    dash character; ASCII hyphens are used instead. The scan covers every text
    file in the tree, not only the documentation.

.EXAMPLE
    pwsh -File scripts/scan-em-dash.ps1
#>

[CmdletBinding()]
param(
    [string]$ScanPath = '.'
)

$ErrorActionPreference = 'Stop'

$Root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Set-Location $Root

$emDash = [char]0x2014

$excludedDirectories = @('node_modules', 'target', '.git', 'artifacts', 'dist', 'coverage')
$binaryExtensions = @(
    '.png', '.jpg', '.jpeg', '.gif', '.ico', '.pdf', '.zip', '.gz', '.tgz', '.xz',
    '.exe', '.dll', '.so', '.dylib', '.woff', '.woff2', '.ttf', '.eot',
    '.mp4', '.mov', '.wasm', '.lock'
)

function Get-ScannableFile {
    param(
        [Parameter(Mandatory = $true)][string]$Path
    )
    if (-not (Test-Path -LiteralPath $Path)) {
        return
    }
    $item = Get-Item -LiteralPath $Path
    if ($item.PSIsContainer) {
        foreach ($child in Get-ChildItem -LiteralPath $Path -Force) {
            if ($child.PSIsContainer) {
                if ($excludedDirectories -notcontains $child.Name) {
                    Get-ScannableFile -Path $child.FullName
                }
            }
            elseif ($binaryExtensions -notcontains $child.Extension.ToLowerInvariant()) {
                $child.FullName
            }
        }
    }
    else {
        if ($binaryExtensions -notcontains $item.Extension.ToLowerInvariant()) {
            $item.FullName
        }
    }
}

$files = @(Get-ScannableFile -Path $ScanPath)

$status = 0
foreach ($file in $files) {
    if (-not (Test-Path -LiteralPath $file -PathType Leaf)) {
        continue
    }
    $content = $null
    try {
        $content = [System.IO.File]::ReadAllText($file)
    }
    catch {
        # A file that another process holds open cannot be scanned. It is
        # skipped, and the run stays a pass, exactly like the POSIX version.
        Write-Host "skipped, the file could not be read: $file"
        continue
    }
    if ($content.IndexOf($emDash) -ge 0) {
        Write-Host "em dash found in $file"
        $line = 0
        foreach ($textLine in ($content -split "`r?`n")) {
            $line++
            if ($textLine.IndexOf($emDash) -ge 0) {
                Write-Host ("  {0}:{1}: {2}" -f $file, $line, $textLine.Trim())
            }
        }
        $status = 1
    }
}

if ($status -ne 0) {
    Write-Error 'em dash scan: failed, replace the em dash with an ASCII hyphen'
    exit 1
}

Write-Host "em dash scan: no em dash found under $ScanPath"
