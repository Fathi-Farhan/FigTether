#!/usr/bin/env pwsh
<#
.SYNOPSIS
    Create one PentAGI flow per FigTether assessment prompt.

.DESCRIPTION
    The script reads every markdown file of the flow directory, extracts the
    target description of the file, and submits it to the PentAGI GraphQL API
    with the createFlow mutation. Afterwards it lists the flows of the
    instance and writes the identifiers of the created flows into a JSON
    summary file.

    The API token is read from the parameter or from the environment. It is
    never printed and never written into the summary file.

    The PowerShell twin of scripts/pentagi/run-assessment.sh. It works with
    Windows PowerShell 5.1 and with PowerShell 7 or newer.

.EXAMPLE
    pwsh -File scripts/pentagi/run-assessment.ps1 -BaseUrl https://localhost:8443 -Token $env:PENTAGI_API_TOKEN
#>

[CmdletBinding()]
param(
    [string]$BaseUrl = $env:PENTAGI_BASE_URL,
    [string]$Token = $env:PENTAGI_API_TOKEN,
    [string]$Provider = 'openai',
    [string]$FlowDir,
    [string]$OutFile,
    [int]$Timeout = 60,
    [switch]$Insecure,
    [switch]$Help
)

$ErrorActionPreference = 'Stop'

$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
if ([string]::IsNullOrWhiteSpace($FlowDir)) {
    $FlowDir = Join-Path $RepoRoot 'security\pentagi\flows'
}
if ([string]::IsNullOrWhiteSpace($OutFile)) {
    $OutFile = Join-Path $RepoRoot 'tmp\pentagi-assessment\flows.json'
}

# The messages that the shell twin prints as well. Keep both in sync.
$TokenMissing = 'the PentAGI API token is missing: pass --token <token> or set PENTAGI_API_TOKEN'
$UrlMissing = 'the PentAGI base URL is missing: pass --base-url <url> or set PENTAGI_BASE_URL'

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
    [Console]::Error.WriteLine("run-assessment: $Message")
    exit 2
}

function Stop-WithError {
    param([string]$Message)
    [Console]::Error.WriteLine("run-assessment: $Message")
    exit 1
}

# Extract the target description of a flow file. The markers are the same
# ones the shell twin reads.
function Get-TargetDescription {
    param([string]$Path)
    $inside = $false
    $buffer = New-Object System.Collections.Generic.List[string]
    foreach ($line in (Get-Content -LiteralPath $Path)) {
        if ($line -eq '<!-- BEGIN TARGET -->') { $inside = $true; continue }
        if ($line -eq '<!-- END TARGET -->') { $inside = $false; continue }
        if ($inside) { $buffer.Add($line) }
    }
    return ($buffer -join "`n")
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

if ($Help) {
    Write-Note 'usage: pwsh -File scripts/pentagi/run-assessment.ps1 -BaseUrl <url> -Token <token>'
    Write-Note '                                              [-Provider openai] [-FlowDir <path>]'
    Write-Note '                                              [-OutFile <file>] [-Timeout <seconds>] [-Insecure]'
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
if ($Provider -notmatch '^[A-Za-z0-9._-]+$') {
    Stop-WithUsage "the model provider name is not a plain name: $Provider"
}
if ($Timeout -lt 1) {
    Stop-WithUsage "the timeout must be at least one second: $Timeout"
}
if (-not (Test-Path -LiteralPath $FlowDir)) {
    Stop-WithError "the flow directory does not exist: $FlowDir"
}

$OutDirectory = Split-Path -Parent $OutFile
if (-not (Test-Path -LiteralPath $OutDirectory)) {
    New-Item -ItemType Directory -Force -Path $OutDirectory | Out-Null
}

$headers = @{ Authorization = "Bearer $Token" }
$query = 'mutation CreateFlow($provider: String!, $input: String!) { createFlow(modelProvider: $provider, input: $input) { id title status } }'
$flowQuery = 'query FlowTitle($flowId: ID!) { flow(flowId: $flowId) { id title status } }'

Write-Step 'assessment'
Write-Note "base url: $BaseUrl"
Write-Note "model provider: $Provider"
Write-Note "flow directory: $FlowDir"
Write-Note "summary file: $OutFile"

# The flow list of the instance, fetched before the creation round. It tells
# which flows of a previous run still exist.
$listFile = Join-Path $OutDirectory 'flows-api-list.json'
$liveFlows = @{}
try {
    $listContent = Invoke-Api -Uri "$BaseUrl/api/v1/flows" -Headers $headers
    Set-Content -LiteralPath $listFile -Value $listContent -Encoding utf8
    $list = $listContent | ConvertFrom-Json
    foreach ($flow in @($list.flows)) {
        if ($null -ne $flow.id) { $liveFlows[[string]$flow.id] = $flow }
    }
    Write-Note "flow list: $listFile"
} catch {
    Write-Note "flow list: the API call failed: $($_.Exception.Message)"
}

# The flows that a previous run of the same summary file created.
$previousFlows = @{}
if (Test-Path -LiteralPath $OutFile) {
    try {
        $previous = Get-Content -LiteralPath $OutFile -Raw | ConvertFrom-Json
        foreach ($flow in @($previous.flows)) {
            if ($null -ne $flow.id -and $null -ne $flow.file) {
                $previousFlows[[string]$flow.file] = [string]$flow.id
            }
        }
    } catch {
        Write-Note "summary file: the previous summary could not be read, every flow is created again"
    }
}

$flows = New-Object System.Collections.Generic.List[object]
$created = 0
$reused = 0
$failed = 0
$index = 0

foreach ($flowFile in (Get-ChildItem -LiteralPath $FlowDir -Filter '*.md' | Sort-Object -Property Name)) {
    $index++
    $name = $flowFile.Name
    $target = Get-TargetDescription -Path $flowFile.FullName
    if ([string]::IsNullOrWhiteSpace($target)) {
        Write-Note "skipped ${name}: no target description between the target markers"
        $failed++
        continue
    }

    # A flow of a previous run is reused when it still exists, so that a
    # second run of the script does not start the same flow twice.
    if ($previousFlows.ContainsKey($name) -and $liveFlows.ContainsKey($previousFlows[$name])) {
        $previousId = $previousFlows[$name]
        $titlePayload = @{
            query     = $flowQuery
            variables = @{ flowId = $previousId }
        } | ConvertTo-Json -Depth 5 -Compress
        $reusedEntry = $null
        try {
            $titleContent = Invoke-Api -Uri "$BaseUrl/api/v1/graphql" -Method Post -Body $titlePayload -Headers $headers
            $titleAnswer = $titleContent | ConvertFrom-Json
            if ($null -ne $titleAnswer.data -and $null -ne $titleAnswer.data.flow) {
                $reusedEntry = $titleAnswer.data.flow
            }
        } catch {
            $reusedEntry = $null
        }
        if ($null -ne $reusedEntry) {
            $flowTitle = $reusedEntry.title
            if ([string]::IsNullOrWhiteSpace($flowTitle)) { $flowTitle = $name }
            $flows.Add([ordered]@{
                file   = $name
                title  = $flowTitle
                id     = [string]$reusedEntry.id
                status = [string]$reusedEntry.status
            })
            Write-Note "reused $name -> flow $($reusedEntry.id) ($($reusedEntry.status))"
            $reused++
            continue
        }
    }

    $payload = @{
        query     = $query
        variables = @{
            provider = $Provider
            input    = $target
        }
    } | ConvertTo-Json -Depth 5 -Compress

    try {
        $content = Invoke-Api -Uri "$BaseUrl/api/v1/graphql" -Method Post -Body $payload -Headers $headers
    } catch {
        Write-Note "failed ${name}: the API call failed: $($_.Exception.Message)"
        $failed++
        continue
    }

    $answer = $null
    try {
        $answer = $content | ConvertFrom-Json
    } catch {
        Write-Note "failed ${name}: the answer is not JSON"
        $failed++
        continue
    }
    if ($null -eq $answer.data -or $null -eq $answer.data.createFlow -or [string]::IsNullOrWhiteSpace($answer.data.createFlow.id)) {
        Write-Note "failed ${name}: the answer carries no flow identifier"
        $failed++
        continue
    }

    $flow = $answer.data.createFlow
    $flowTitle = $flow.title
    if ([string]::IsNullOrWhiteSpace($flowTitle)) { $flowTitle = $name }
    $flows.Add([ordered]@{
        file   = $name
        title  = $flowTitle
        id     = [string]$flow.id
        status = [string]$flow.status
    })
    Write-Note "created $name -> flow $($flow.id) ($($flow.status))"
    $created++
}

# Refresh the flow list of the instance after the creation round.
try {
    $listContent = Invoke-Api -Uri "$BaseUrl/api/v1/flows" -Headers $headers
    Set-Content -LiteralPath $listFile -Value $listContent -Encoding utf8
    Write-Note "flow list: $listFile"
} catch {
    Write-Note "flow list: the API call failed: $($_.Exception.Message)"
    $failed++
}

$summary = [ordered]@{
    generated_at   = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
    base_url       = $BaseUrl
    model_provider = $Provider
    flow_directory = (Resolve-Path -LiteralPath $FlowDir).Path
    flows          = $flows
}
$summary | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $OutFile -Encoding utf8

Write-Note "created flows: $created"
Write-Note "reused flows: $reused"
Write-Note "failed flows: $failed"
Write-Note "summary file: $OutFile"

if ($flows.Count -eq 0) {
    Stop-WithError 'no flow was created, see the messages above'
}
