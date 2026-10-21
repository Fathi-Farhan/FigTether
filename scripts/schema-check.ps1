#!/usr/bin/env pwsh
<#
.SYNOPSIS
    Compare the generated JSON schemas with the committed ones.

.DESCRIPTION
    The PowerShell twin of scripts/schema-check.sh. The generator is a Cargo
    example that follows this contract:

        cargo run -p <package> --example <name> -- <output-directory>

    The check fails when a committed schema differs from the generated one,
    when a committed schema is missing from the generated set, or when a
    generated schema is not committed yet. Missing generators and empty schema
    sets also fail; required contract checks cannot be skipped.

.EXAMPLE
    pwsh -File scripts/schema-check.ps1
#>

$ErrorActionPreference = 'Stop'

$Root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Set-Location $Root

$Example = if ($env:FIGTETHER_SCHEMA_EXAMPLE) { $env:FIGTETHER_SCHEMA_EXAMPLE } else { 'export_schemas' }
$SchemaDir = if ($env:FIGTETHER_SCHEMA_DIR) { $env:FIGTETHER_SCHEMA_DIR } else { 'schemas' }
$Package = $env:FIGTETHER_SCHEMA_PACKAGE

$candidates = @(
    "crates/model/examples/$Example.rs",
    "crates/protocol/examples/$Example.rs",
    "crates/cli/examples/$Example.rs",
    "crates/core/examples/$Example.rs"
)

$found = $null
foreach ($candidate in $candidates) {
    if (Test-Path $candidate) {
        $found = $candidate
        break
    }
}

if ($null -eq $found) {
    Write-Host "schema check: failed, no schema generator example named $Example was found"
    Write-Host "schema check: expected one of crates/{model,protocol,cli,core}/examples/$Example.rs"
    exit 1
}

Write-Host "schema check: generator $found"

# The workspace package is derived from the crate that owns the example, so the
# check keeps working when the example moves to another crate.
if (-not $Package) {
    $crate = ($found -split '/')[1]
    if ($crate -like 'figtether-*') {
        $Package = $crate
    }
}

if (-not (Test-Path $SchemaDir)) {
    Write-Error "schema check: failed, the committed schema directory $SchemaDir does not exist"
    exit 1
}

$temp = Join-Path ([System.IO.Path]::GetTempPath()) ("figtether-schema-" + [System.Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $temp | Out-Null
$tempBase = (Resolve-Path -LiteralPath ([System.IO.Path]::GetTempPath())).Path.TrimEnd('\', '/')
$temp = (Resolve-Path -LiteralPath $temp).Path
if (-not $temp.StartsWith($tempBase + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw 'Schema temporary directory is outside the expected temporary root'
}

try {
    $arguments = @('run')
    if ($Package) {
        $arguments += @('--package', $Package)
    }
    $arguments += @('--example', $Example, '--locked', '--', $temp)

    & cargo @arguments
    if ($LASTEXITCODE -ne 0) {
        throw "the schema generator failed with exit code $LASTEXITCODE"
    }

    $status = 0
    $schemaRoot = (Resolve-Path $SchemaDir).Path

    $committed = Get-ChildItem -Path $SchemaDir -Recurse -File -Filter '*.schema.json' | ForEach-Object {
        $_.FullName.Substring($schemaRoot.Length + 1)
    } | Sort-Object

    if (@($committed).Count -eq 0) {
        throw 'schema check: no committed schemas were found'
    }
    if (@(Get-ChildItem -LiteralPath $temp -Recurse -File -Filter '*.schema.json').Count -eq 0) {
        throw 'schema check: the generator produced no schemas'
    }

    foreach ($relative in $committed) {
        $generated = Join-Path $temp $relative
        if (-not (Test-Path $generated)) {
            Write-Error "schema check: $relative is committed but was not generated"
            $status = 1
            continue
        }
        $left = (Get-FileHash -Path (Join-Path $SchemaDir $relative) -Algorithm SHA256).Hash
        $right = (Get-FileHash -Path $generated -Algorithm SHA256).Hash
        if ($left -ne $right) {
            Write-Error "schema check: $relative differs from the generated schema"
            $status = 1
        }
    }

    $produced = Get-ChildItem -Path $temp -Recurse -File -Filter '*.schema.json' | ForEach-Object {
        $_.FullName.Substring($temp.Length + 1)
    } | Sort-Object

    foreach ($relative in $produced) {
        if (-not (Test-Path (Join-Path $SchemaDir $relative))) {
            Write-Error "schema check: $relative was generated but is not committed"
            $status = 1
        }
    }

    if ($status -ne 0) {
        Write-Error 'schema check: failed, regenerate the schemas and commit the result'
        exit 1
    }

    Write-Host 'schema check: the committed schemas match the generated ones'
}
finally {
    Remove-Item -LiteralPath $temp -Recurse -Force -ErrorAction SilentlyContinue
}
