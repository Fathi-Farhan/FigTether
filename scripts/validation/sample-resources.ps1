param([string]$Output = 'benchmarks/results/desktop-2026-10-20/resources.raw.jsonl')
$ErrorActionPreference = 'Stop'
$outputDirectory = Split-Path -Parent $Output
New-Item -ItemType Directory -Force -Path $outputDirectory | Out-Null
$stopFile = Join-Path $outputDirectory 'sampling.stop'
if (Test-Path -LiteralPath $stopFile) { Remove-Item -LiteralPath $stopFile }
while (-not (Test-Path -LiteralPath $stopFile)) {
    $daemonProcess = Get-Process figtether -ErrorAction SilentlyContinue | Where-Object { $_.Path -like '*release*' -and $_.MainWindowHandle -eq 0 } | Sort-Object StartTime | Select-Object -First 1
    $editorProcesses = @(Get-Process Figma -ErrorAction SilentlyContinue)
    $phaseFile = Join-Path $outputDirectory 'phase.txt'
    $phase = if (Test-Path -LiteralPath $phaseFile) { (Get-Content -LiteralPath $phaseFile -Raw).Trim() } else { 'idle' }
    if ($daemonProcess) {
        [pscustomobject]@{
            timestamp = [DateTime]::UtcNow.ToString('o'); phase = $phase;
            daemon_pid = $daemonProcess.Id; daemon_working_set_bytes = $daemonProcess.WorkingSet64;
            daemon_private_bytes = $daemonProcess.PrivateMemorySize64; daemon_cpu_seconds = $daemonProcess.TotalProcessorTime.TotalSeconds;
            figma_processes = $editorProcesses.Count; figma_working_set_sum_bytes = ($editorProcesses | Measure-Object WorkingSet64 -Sum).Sum
        } | ConvertTo-Json -Compress | Add-Content -LiteralPath $Output
    }
    Start-Sleep -Seconds 1
}
Write-Output 'Resource sampling stopped.'
