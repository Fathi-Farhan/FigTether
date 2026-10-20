#!/usr/bin/env pwsh
<#
.SYNOPSIS
    Start the FigTether daemon as the PentAGI assessment target.

.DESCRIPTION
    The daemon runs on a dedicated port and with a dedicated runtime directory,
    so an assessment never occupies the default bridge port and never touches
    the production runtime directory. The script waits until the health
    endpoint of the daemon answers, then prints the port and the runtime
    directory.

    The script is safe to run twice: when a daemon already answers on the
    assessment port, the existing one is reused and no second daemon is
    started.

    The PowerShell twin of scripts/pentagi/start-target.sh. It works with
    Windows PowerShell 5.1 and with PowerShell 7 or newer.

.EXAMPLE
    pwsh -File scripts/pentagi/start-target.ps1 -Port 39047
#>

[CmdletBinding()]
param(
    [int]$Port = 39047,
    [string]$RuntimeDir,
    [string]$Binary,
    [int]$Timeout = 30,
    [switch]$Help
)

$ErrorActionPreference = 'Stop'

# The default bridge port of FigTether. The assessment target never uses it.
$DefaultBridgePort = 38947

$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
if ([string]::IsNullOrWhiteSpace($RuntimeDir)) {
    $RuntimeDir = Join-Path $RepoRoot 'tmp\pentagi-assessment\runtime'
}

function Write-Step {
    param([string]$Message)
    Write-Host "==> $Message"
}

function Write-Note {
    param([string]$Message)
    Write-Host $Message
}

function Stop-WithError {
    param([string]$Message)
    [Console]::Error.WriteLine("start-target: $Message")
    exit 1
}

# A comparable form of a path. The comparison only has to recognise the
# production runtime directory, so a false refusal is acceptable and a false
# acceptance is not.
function Get-NormalizedPath {
    param([string]$Path)
    if ([string]::IsNullOrWhiteSpace($Path)) { return '' }
    $text = $Path.Replace('\', '/').ToLowerInvariant()
    return $text.TrimEnd('/')
}

# The platform runtime directory of a production FigTether installation.
function Get-PlatformRuntimeDir {
    if (-not [string]::IsNullOrWhiteSpace($env:LOCALAPPDATA)) {
        return (Join-Path $env:LOCALAPPDATA 'figtether')
    }
    return ''
}

function Get-HealthBody {
    param([string]$Uri)
    try {
        $response = Invoke-WebRequest -Uri $Uri -UseBasicParsing -TimeoutSec 3 -ErrorAction Stop
        return [string]$response.Content
    } catch {
        return ''
    }
}

# Whether the health endpoint answers for this daemon and this port.
function Test-Health {
    param(
        [string]$Uri,
        [int]$ExpectedPort
    )
    $body = Get-HealthBody -Uri $Uri
    if ([string]::IsNullOrWhiteSpace($body)) { return $false }
    $compact = $body -replace '\s', ''
    return $compact.Contains('"port":' + $ExpectedPort)
}

if ($Help) {
    Write-Note 'usage: pwsh -File scripts/pentagi/start-target.ps1 [-Port PORT] [-RuntimeDir PATH]'
    Write-Note '                                              [-Binary PATH] [-Timeout SECONDS]'
    exit 0
}

if ($Port -lt 1 -or $Port -gt 65535) {
    Stop-WithError "the assessment port must be between 1 and 65535: $Port"
}
if ($Port -eq $DefaultBridgePort) {
    Stop-WithError "the assessment target must not use the default bridge port $DefaultBridgePort"
}
if ($Timeout -lt 1) {
    Stop-WithError "the timeout must be at least one second: $Timeout"
}
if ([string]::IsNullOrWhiteSpace($RuntimeDir)) {
    Stop-WithError 'the runtime directory must not be empty'
}

$ProductionDir = Get-PlatformRuntimeDir
if (-not [string]::IsNullOrWhiteSpace($ProductionDir)) {
    if ((Get-NormalizedPath $RuntimeDir) -eq (Get-NormalizedPath $ProductionDir)) {
        Stop-WithError "the assessment target must not use the production runtime directory: $ProductionDir"
    }
}

New-Item -ItemType Directory -Force -Path $RuntimeDir | Out-Null
$RuntimeDir = (Resolve-Path $RuntimeDir).Path

# Locate the daemon binary.
if ([string]::IsNullOrWhiteSpace($Binary)) {
    $targetRoot = $RepoRoot
    if (-not [string]::IsNullOrWhiteSpace($env:CARGO_TARGET_DIR)) {
        $targetRoot = $env:CARGO_TARGET_DIR
    }
    $candidates = @(
        (Join-Path $targetRoot 'release\figtether.exe'),
        (Join-Path $targetRoot 'debug\figtether.exe'),
        (Join-Path $targetRoot 'release\figtether'),
        (Join-Path $targetRoot 'debug\figtether'),
        (Join-Path $RepoRoot 'target\release\figtether.exe'),
        (Join-Path $RepoRoot 'target\debug\figtether.exe'),
        (Join-Path $RepoRoot 'target\release\figtether'),
        (Join-Path $RepoRoot 'target\debug\figtether')
    )
    foreach ($candidate in $candidates) {
        if (Test-Path -LiteralPath $candidate) {
            $Binary = $candidate
            break
        }
    }
}
if ([string]::IsNullOrWhiteSpace($Binary)) {
    $found = Get-Command figtether -ErrorAction SilentlyContinue
    if ($null -ne $found) { $Binary = $found.Source }
}
if ([string]::IsNullOrWhiteSpace($Binary)) {
    Stop-WithError 'the figtether binary was not found, build it with "cargo build --release" or pass -Binary'
}
if (-not (Test-Path -LiteralPath $Binary)) {
    Stop-WithError "the figtether binary does not exist: $Binary"
}

$HealthUri = "http://127.0.0.1:$Port/health"

Write-Step 'assessment target'
Write-Note "port: $Port"
Write-Note "runtime directory: $RuntimeDir"
Write-Note "binary: $Binary"

if (Test-Health -Uri $HealthUri -ExpectedPort $Port) {
    Write-Note "health: answered (GET $HealthUri)"
    Write-Note 'note: a daemon already answers on the assessment port, no second daemon was started'
    Set-Content -Path (Join-Path $RuntimeDir 'assessment-port') -Value $Port -Encoding ascii
    Set-Content -Path (Join-Path $RuntimeDir 'assessment-runtime-dir') -Value $RuntimeDir -Encoding ascii
    exit 0
}

$StdoutLog = Join-Path $RuntimeDir 'daemon.stdout.log'
$StderrLog = Join-Path $RuntimeDir 'daemon.stderr.log'
$PidFile = Join-Path $RuntimeDir 'daemon.pid'

# The daemon is detached from the console of the caller with "start /b", so
# the script returns while the daemon keeps running in the background.
$cmdLine = 'start "" /b "' + $Binary + '" daemon --port ' + $Port +
    ' --runtime-dir "' + $RuntimeDir + '" --log-level info' +
    ' > "' + $StdoutLog + '" 2> "' + $StderrLog + '"'
Start-Process -FilePath 'cmd.exe' -ArgumentList '/c', $cmdLine -WindowStyle Hidden | Out-Null

$deadline = (Get-Date).AddSeconds($Timeout)
$ready = $false
while ((Get-Date) -lt $deadline) {
    if (Test-Health -Uri $HealthUri -ExpectedPort $Port) {
        $ready = $true
        break
    }
    Start-Sleep -Seconds 1
}

if (-not $ready) {
    Write-Note "health: no answer (GET $HealthUri)"
    Write-Note 'the daemon output follows'
    foreach ($log in @($StdoutLog, $StderrLog)) {
        if (Test-Path -LiteralPath $log) {
            Get-Content -LiteralPath $log -TotalCount 40 | ForEach-Object { Write-Host $_ }
        }
    }
    Stop-WithError "the assessment target did not start on port $Port"
}

# The identifier of the detached process comes from the listening socket.
$daemonPid = 0
try {
    $connection = Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue |
        Select-Object -First 1
    if ($null -ne $connection) { $daemonPid = $connection.OwningProcess }
} catch {
    $daemonPid = 0
}
Set-Content -Path $PidFile -Value $daemonPid -Encoding ascii

Write-Note "daemon pid: $daemonPid"
Write-Note "health: answered (GET $HealthUri)"
Set-Content -Path (Join-Path $RuntimeDir 'assessment-port') -Value $Port -Encoding ascii
Set-Content -Path (Join-Path $RuntimeDir 'assessment-runtime-dir') -Value $RuntimeDir -Encoding ascii
Write-Note 'the assessment target is ready'
