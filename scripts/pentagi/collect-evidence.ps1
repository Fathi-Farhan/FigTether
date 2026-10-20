#!/usr/bin/env pwsh
<#
.SYNOPSIS
    Collect the evidence of a PentAGI assessment.

.DESCRIPTION
    The script writes one directory per run under security/pentagi/evidence and
    fills it with

      - the raw GraphQL payload of every flow of the assessment (flow-<id>.json),
      - a readable report of every flow (flow-<id>.md),
      - the diagnostic log of the FigTether assessment target (daemon.log),
      - the manifest of the run (manifest.md and manifest.json).

    The manifest states that the collected agent output is unauthenticated and
    that it does not count as a finding before a human reviewed it.

    The API token is read from the parameter or from the environment. It is
    never printed and never written into the evidence.

    The PowerShell twin of scripts/pentagi/collect-evidence.sh. It works with
    Windows PowerShell 5.1 and with PowerShell 7 or newer.

.EXAMPLE
    pwsh -File scripts/pentagi/collect-evidence.ps1 -BaseUrl https://localhost:8443 -Token $env:PENTAGI_API_TOKEN
#>

[CmdletBinding()]
param(
    [string]$BaseUrl = $env:PENTAGI_BASE_URL,
    [string]$Token = $env:PENTAGI_API_TOKEN,
    [string]$FlowsFile,
    [string]$RuntimeDir,
    [string]$OutDir,
    [int]$Timeout = 60,
    [switch]$Insecure,
    [switch]$Help
)

$ErrorActionPreference = 'Stop'

$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
if ([string]::IsNullOrWhiteSpace($FlowsFile)) {
    $FlowsFile = Join-Path $RepoRoot 'tmp\pentagi-assessment\flows.json'
}
if ([string]::IsNullOrWhiteSpace($RuntimeDir)) {
    $RuntimeDir = Join-Path $RepoRoot 'tmp\pentagi-assessment\runtime'
}
if ([string]::IsNullOrWhiteSpace($OutDir)) {
    $OutDir = Join-Path $RepoRoot 'security\pentagi\evidence'
}

# The messages that the sibling scripts print as well. Keep them in sync.
$TokenMissing = 'the PentAGI API token is missing: pass --token <token> or set PENTAGI_API_TOKEN'
$UrlMissing = 'the PentAGI base URL is missing: pass --base-url <url> or set PENTAGI_BASE_URL'

$ReviewStatement = 'This evidence is unauthenticated agent output. A PentAGI flow is written by a language model that ran tools on its own, and the collected text is not a statement of the FigTether maintainers. Nothing in this directory counts as a finding before a human reviewer has read the report, replayed the described requests against the assessment target, and confirmed that the observed answers match the behaviour of the daemon. A claim of a vulnerability is not a vulnerability until the reproduction is replayed by hand.'

function Write-Step {
    param([string]$Message)
    Write-Host "==> $Message"
}

function Write-Note {
    param([string]$Message)
    Write-Host $Message
}

function Stop-WithUsage {
    param([string]$Message)
    [Console]::Error.WriteLine("collect-evidence: $Message")
    exit 2
}

function Stop-WithError {
    param([string]$Message)
    [Console]::Error.WriteLine("collect-evidence: $Message")
    exit 1
}

# Call the PentAGI API and return the answer as text.
function Invoke-Api {
    param(
        [string]$Uri,
        [string]$Method = 'Get',
        [string]$Body,
        [hashtable]$Headers
    )
    $parameters = @{
        Uri         = $Uri
        Method      = $Method
        TimeoutSec  = $Timeout
        ErrorAction = 'Stop'
    }
    if ($null -ne $Headers) { $parameters['Headers'] = $Headers }
    if (-not [string]::IsNullOrWhiteSpace($Body)) {
        $parameters['ContentType'] = 'application/json'
        $parameters['Body'] = $Body
    }
    if ($PSVersionTable.PSVersion.Major -lt 6) {
        $parameters['UseBasicParsing'] = $true
    }
    if ($Insecure) {
        if ($PSVersionTable.PSVersion.Major -ge 6) {
            $parameters['SkipCertificateCheck'] = $true
        }
    }
    $response = Invoke-WebRequest @parameters
    return [string]$response.Content
}

function Get-StatusLabel {
    param([string]$Status)
    switch ($Status) {
        'finished' { return 'OK' }
        'failed' { return 'FAILED' }
        'running' { return 'RUNNING' }
        'waiting' { return 'WAITING' }
        default { return 'CREATED' }
    }
}

# Render the readable flow report out of the GraphQL payload.
function Format-FlowReport {
    param($Payload)
    $flow = $Payload.data.flow
    if ($null -eq $flow) {
        return "No flow report was found for this flow.`n"
    }
    $lines = New-Object System.Collections.Generic.List[string]
    $lines.Add("# $(Get-StatusLabel $flow.status) $($flow.id). $($flow.title)")
    $lines.Add('')
    $lines.Add("status: $($flow.status)")
    $lines.Add('')
    foreach ($task in @($Payload.data.tasks)) {
        $lines.Add("### $(Get-StatusLabel $task.status) $($task.id). $($task.title)")
        $lines.Add('')
        if (-not [string]::IsNullOrWhiteSpace($task.input)) {
            $lines.Add($task.input)
            $lines.Add('')
        }
        if (-not [string]::IsNullOrWhiteSpace($task.result)) {
            $lines.Add('---')
            $lines.Add('')
            $lines.Add($task.result)
            $lines.Add('')
        }
        foreach ($subtask in @($task.subtasks)) {
            $lines.Add("#### $(Get-StatusLabel $subtask.status) $($subtask.id). $($subtask.title)")
            $lines.Add('')
            if (-not [string]::IsNullOrWhiteSpace($subtask.description)) {
                $lines.Add($subtask.description)
                $lines.Add('')
            }
            if (-not [string]::IsNullOrWhiteSpace($subtask.result)) {
                $lines.Add('---')
                $lines.Add('')
                $lines.Add($subtask.result)
                $lines.Add('')
            }
        }
        $lines.Add('---')
        $lines.Add('')
    }
    return ($lines -join "`n")
}

if ($Help) {
    Write-Note 'usage: pwsh -File scripts/pentagi/collect-evidence.ps1 -BaseUrl <url> -Token <token>'
    Write-Note '                                                [-FlowsFile <file>] [-RuntimeDir <dir>]'
    Write-Note '                                                [-OutDir <dir>] [-Timeout <seconds>] [-Insecure]'
    exit 0
}

if ([string]::IsNullOrWhiteSpace($Token)) {
    Stop-WithUsage $TokenMissing
}
if ([string]::IsNullOrWhiteSpace($BaseUrl)) {
    Stop-WithUsage $UrlMissing
}
if ($BaseUrl -notmatch '^https?://') {
    Stop-WithUsage "the PentAGI base URL must start with http:// or https://: $BaseUrl"
}
$BaseUrl = $BaseUrl.TrimEnd('/')
if ($Timeout -lt 1) {
    Stop-WithUsage "the timeout must be at least one second: $Timeout"
}

$headers = @{ Authorization = "Bearer $Token" }
$query = 'query FlowReport($flowId: ID!) { flow(flowId: $flowId) { id title status createdAt updatedAt } tasks(flowId: $flowId) { id title status input result subtasks { id title status description result } } }'

$timestamp = (Get-Date).ToUniversalTime().ToString('yyyyMMddTHH:mm:ssZ').Replace(':', '')
$evidenceDir = Join-Path $OutDir $timestamp
New-Item -ItemType Directory -Force -Path $evidenceDir | Out-Null

Write-Step 'evidence'
Write-Note "base url: $BaseUrl"
Write-Note "evidence directory: $evidenceDir"

# The flows of the assessment come from the summary file of the run. When
# that file is missing, every flow of the instance is collected.
$flowIds = New-Object System.Collections.Generic.List[string]
if (Test-Path -LiteralPath $FlowsFile) {
    Write-Note "flows file: $FlowsFile"
    $summary = Get-Content -LiteralPath $FlowsFile -Raw | ConvertFrom-Json
    foreach ($flow in @($summary.flows)) {
        if ($null -ne $flow.id) { $flowIds.Add([string]$flow.id) }
    }
} else {
    Write-Note "flows file: $FlowsFile was not found, every flow of the instance is collected"
}

$listFile = Join-Path $evidenceDir 'flows-api-list.json'
try {
    $listContent = Invoke-Api -Uri "$BaseUrl/api/v1/flows" -Headers $headers
    Set-Content -LiteralPath $listFile -Value $listContent -Encoding utf8
    Write-Note "flow list: $listFile"
} catch {
    Write-Note "flow list: the API call failed: $($_.Exception.Message)"
}
if ($flowIds.Count -eq 0 -and (Test-Path -LiteralPath $listFile)) {
    $list = Get-Content -LiteralPath $listFile -Raw | ConvertFrom-Json
    foreach ($flow in @($list.flows)) {
        if ($null -ne $flow.id) { $flowIds.Add([string]$flow.id) }
    }
}

$collected = 0
foreach ($flowId in $flowIds) {
    if ([string]::IsNullOrWhiteSpace($flowId)) { continue }
    $payload = @{
        query     = $query
        variables = @{ flowId = $flowId }
    } | ConvertTo-Json -Depth 5 -Compress

    $responseFile = Join-Path $evidenceDir "flow-$flowId.json"
    try {
        $content = Invoke-Api -Uri "$BaseUrl/api/v1/graphql" -Method Post -Body $payload -Headers $headers
    } catch {
        Write-Note "flow ${flowId}: the API call failed, the report is missing"
        continue
    }
    Set-Content -LiteralPath $responseFile -Value $content -Encoding utf8
    $answer = $content | ConvertFrom-Json
    $report = Format-FlowReport -Payload $answer
    Set-Content -LiteralPath (Join-Path $evidenceDir "flow-$flowId.md") -Value $report -Encoding utf8
    Write-Note "flow ${flowId}: $(Join-Path $evidenceDir "flow-$flowId.md")"
    $collected++
}

# The diagnostic log of the assessment target.
if (Test-Path -LiteralPath $RuntimeDir) {
    foreach ($logName in @('figtether.log', 'figtether.log.1', 'daemon.stdout.log', 'daemon.stderr.log')) {
        $source = Join-Path $RuntimeDir $logName
        if (Test-Path -LiteralPath $source) {
            Copy-Item -LiteralPath $source -Destination (Join-Path $evidenceDir $logName) -Force
            Write-Note "log: $(Join-Path $evidenceDir $logName)"
        }
    }
} else {
    Write-Note "runtime directory: $RuntimeDir was not found, the daemon log is missing"
}

$collectedAt = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')

# The file list of the manifest.
$files = New-Object System.Collections.Generic.List[object]
foreach ($file in (Get-ChildItem -LiteralPath $evidenceDir -File | Sort-Object -Property Name)) {
    if ($file.Name -eq 'manifest.md' -or $file.Name -eq 'manifest.json') { continue }
    $digest = ''
    if (Get-Command Get-FileHash -ErrorAction SilentlyContinue) {
        $digest = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    }
    $files.Add([ordered]@{
        name   = $file.Name
        bytes  = $file.Length
        sha256 = $digest
    })
}

$manifestLines = New-Object System.Collections.Generic.List[string]
$manifestLines.Add('# PentAGI assessment evidence')
$manifestLines.Add('')
$manifestLines.Add("Collected at: $collectedAt")
$manifestLines.Add('')
$manifestLines.Add("PentAGI base URL: $BaseUrl")
$manifestLines.Add('')
$manifestLines.Add('Assessment target: the FigTether daemon on 127.0.0.1, assessment port only')
$manifestLines.Add('')
$manifestLines.Add("Daemon runtime directory: $RuntimeDir")
$manifestLines.Add('')
$manifestLines.Add("Flows collected: $collected")
$manifestLines.Add('')
$manifestLines.Add('## Collected files')
$manifestLines.Add('')
$manifestLines.Add('| File | Bytes | SHA-256 |')
$manifestLines.Add('| ---- | ----- | ------- |')
foreach ($file in $files) {
    $manifestLines.Add("| $($file.name) | $($file.bytes) | $($file.sha256) |")
}
$manifestLines.Add('')
$manifestLines.Add('## How to read this evidence')
$manifestLines.Add('')
$manifestLines.Add('1. `flow-<id>.md` is the report of one flow. It holds the task results and the subtask results that the agent wrote.')
$manifestLines.Add('2. `flow-<id>.json` is the raw API payload of the same flow. Use it when a line of the report looks truncated.')
$manifestLines.Add('3. `figtether.log` is the diagnostic log of the assessment target. It carries error codes, versions, and counters, never design content.')
$manifestLines.Add('4. `flows-api-list.json` is the flow list of the PentAGI instance after the assessment.')
$manifestLines.Add('')
$manifestLines.Add('## Review status')
$manifestLines.Add('')
$manifestLines.Add($ReviewStatement)
$manifestLines.Add('')
Set-Content -LiteralPath (Join-Path $evidenceDir 'manifest.md') -Value ($manifestLines -join "`n") -Encoding utf8

$manifest = [ordered]@{
    collected_at     = $collectedAt
    base_url         = $BaseUrl
    runtime_directory = $RuntimeDir
    flows_collected  = $collected
    review_statement = $ReviewStatement
    files            = $files
}
$manifest | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $evidenceDir 'manifest.json') -Encoding utf8

Write-Note "manifest: $(Join-Path $evidenceDir 'manifest.md')"
Write-Note "collected flows: $collected"
Write-Note 'the evidence is unauthenticated agent output and needs a human review'
