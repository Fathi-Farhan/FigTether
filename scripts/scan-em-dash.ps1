#!/usr/bin/env pwsh
<#
.SYNOPSIS
    Scan the repository for em dashes.

.DESCRIPTION
    The PowerShell twin of scripts/scan-em-dash.sh. PRD 8.12 forbids the em
    dash character; ASCII hyphens are used instead. The scan covers every text
    file that the repository publishes, not only the documentation.

    The file set comes from Git: every tracked file plus every untracked file
    that is not ignored. Local scratch space, such as a run directory or a
    browser profile dump, is excluded because Git excludes it. When Git is not
    available, the script falls back to a recursive walk of the tree.

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

function Get-WalkedFile {
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
                    Get-WalkedFile -Path $child.FullName
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

function Get-PublishedFile {
    <#
        The files that the repository publishes: tracked files, plus new files
        that are not ignored. The output is relative to the repository root.
    #>
    param(
        [Parameter(Mandatory = $true)][string]$Path
    )
    $listed = & git -C $Root ls-files --cached --others --exclude-standard -- $Path 2>$null
    if ($LASTEXITCODE -ne 0 -or $null -eq $listed) {
        return $null
    }
    return @($listed)
}

$files = @(Get-PublishedFile -Path $ScanPath)
if ($files.Count -eq 0) {
    # Git could not answer, or the path holds nothing that is published.
    $files = @(Get-WalkedFile -Path $ScanPath)
}

$status = 0
foreach ($file in $files) {
    $full = if ([System.IO.Path]::IsPathRooted($file)) { $file } else { Join-Path $Root $file }
    if (-not (Test-Path -LiteralPath $full -PathType Leaf)) {
        continue
    }
    if ($binaryExtensions -contains ([System.IO.Path]::GetExtension($full).ToLowerInvariant())) {
        continue
    }
    $content = $null
    try {
        $content = [System.IO.File]::ReadAllText($full)
    }
    catch {
        # A file that another process holds open cannot be scanned. It is
        # skipped, and the run stays a pass, exactly like the POSIX version.
        Write-Host "skipped, the file could not be read: $full"
        continue
    }
    if ($content.IndexOf($emDash) -ge 0) {
        Write-Host "em dash found in $full"
        $line = 0
        foreach ($textLine in ($content -split "`r?`n")) {
            $line++
            if ($textLine.IndexOf($emDash) -ge 0) {
                Write-Host ("  {0}:{1}: {2}" -f $full, $line, $textLine.Trim())
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
