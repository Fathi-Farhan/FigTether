param([string]$Output = 'tmp/desktop-validation/figtether-website.png')
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName System.Windows.Forms
Add-Type -TypeDefinition @'
using System.Runtime.InteropServices;
public static class FigmaClipboard {
    [DllImport("user32.dll")] public static extern uint GetClipboardSequenceNumber();
}
'@
$figmaProcess = Get-Process Figma | Where-Object MainWindowHandle -ne 0 | Select-Object -First 1
if (-not $figmaProcess) { throw 'Figma is not open.' }
$figmaWindow = [System.Windows.Automation.AutomationElement]::FromHandle($figmaProcess.MainWindowHandle)
function Invoke-FigmaControl([string]$ControlName) {
    $condition = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, $ControlName)
    $candidates = $figmaWindow.FindAll([System.Windows.Automation.TreeScope]::Descendants, $condition)
    if ($candidates.Count -eq 0) {
        $candidates = $figmaWindow.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition) | Where-Object { $_.Current.Name.StartsWith("$ControlName ") }
    }
    foreach ($control in $candidates) {
        $pattern = $null
        if ($control.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern,[ref]$pattern)) { $pattern.Invoke(); Start-Sleep -Milliseconds 750; return }
        if ($control.TryGetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern,[ref]$pattern)) {
            if ($pattern.Current.ExpandCollapseState -ne [System.Windows.Automation.ExpandCollapseState]::Expanded) { $pattern.Expand() }
            Start-Sleep -Milliseconds 750; return
        }
        if ($control.TryGetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern,[ref]$pattern)) { $pattern.Toggle(); Start-Sleep -Milliseconds 750; return }
    }
    throw "No actionable Figma control: $ControlName"
}
# All operations stay in one UIA session; no foreground keyboard input is used.
Invoke-FigmaControl 'Main menu'
Invoke-FigmaControl 'Edit'
Invoke-FigmaControl 'Select all'
Invoke-FigmaControl 'Main menu'
Invoke-FigmaControl 'Edit'
Invoke-FigmaControl 'Copy as'
$before = [FigmaClipboard]::GetClipboardSequenceNumber()
Invoke-FigmaControl 'Copy as PNG'
Start-Sleep -Seconds 2
if ([FigmaClipboard]::GetClipboardSequenceNumber() -eq $before) { throw 'Figma did not replace the clipboard; no image saved.' }
$designImage = [System.Windows.Forms.Clipboard]::GetImage()
if (-not $designImage) { throw 'Figma did not produce a clipboard image.' }
try {
    if ($designImage.Width -notin @(1440,2880) -or $designImage.Height -ne ($designImage.Width / 1440 * 1640)) { throw 'Clipboard image dimensions do not match the website frame; no image saved.' }
    $designImage.Save([System.IO.Path]::GetFullPath($Output),[System.Drawing.Imaging.ImageFormat]::Png)
    Write-Output "Figma native PNG saved: $($designImage.Width)x$($designImage.Height)"
} finally { $designImage.Dispose() }

