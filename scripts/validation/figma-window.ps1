param(
    [ValidateSet('Inspect', 'Capture', 'Click', 'Drag', 'Keys', 'Invoke', 'SetValue', 'Position')][string]$Action = 'Inspect',
    [string]$Output,
    [int]$X = 0,
    [int]$Y = 0,
    [int]$ToX = 0,
    [int]$ToY = 0,
    [string]$Keys,
    [string]$Name,
    [string]$Value,
    [switch]$UseDefaultDesktopDiscovery,
    [ValidateRange(1, 2000)][int]$InspectLimit = 240
)

# Desktop-test helper. Targets the Figma window only, never a whole-screen capture.
# Captures can contain design data. Keep them in the ignored validation directory.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class FigTetherDesktop {
    private delegate bool WindowCallback(IntPtr hwnd, IntPtr data);
    [StructLayout(LayoutKind.Sequential)] private struct Rect { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll", CharSet=CharSet.Unicode, SetLastError=true)] private static extern IntPtr OpenDesktop(string name, uint flags, bool inherit, uint access);
    [DllImport("user32.dll")] private static extern bool CloseDesktop(IntPtr desktop);
    [DllImport("user32.dll", SetLastError=true)] private static extern bool EnumDesktopWindows(IntPtr desktop, WindowCallback callback, IntPtr data);
    [DllImport("user32.dll")] private static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")] private static extern bool GetWindowRect(IntPtr hwnd, out Rect rect);
    public static IntPtr FindDefaultFigmaWindow(int[] processIds) {
        // Enumerate only the caller's Default desktop with read-object access.
        // No desktop switching, input hooks or access-control changes are used.
        IntPtr desktop = OpenDesktop("Default", 0, false, 0x0001);
        if (desktop == IntPtr.Zero) throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
        IntPtr found = IntPtr.Zero;
        long largest = 0;
        try {
            WindowCallback callback = delegate(IntPtr hwnd, IntPtr data) {
                uint owner;
                GetWindowThreadProcessId(hwnd, out owner);
                if (Array.IndexOf(processIds, (int)owner) < 0 || !IsWindowVisible(hwnd)) return true;
                Rect rect;
                if (GetWindowRect(hwnd, out rect)) {
                    long area = (long)Math.Max(0, rect.Right - rect.Left) * Math.Max(0, rect.Bottom - rect.Top);
                    if (area > largest) { largest = area; found = hwnd; }
                }
                return true;
            };
            if (!EnumDesktopWindows(desktop, callback, IntPtr.Zero)) throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
        } finally { CloseDesktop(desktop); }
        return found;
    }
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hwnd, IntPtr after, int x, int y, int width, int height, uint flags);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
    [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
    [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint first, uint second, bool attach);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr SendMessage(IntPtr hwnd, uint msg, IntPtr wParam, string text);
    [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr hwnd, uint msg, IntPtr wParam, IntPtr lParam);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdc, uint flags);
    // UI Automation reports physical screen coordinates, including on scaled displays.
    [DllImport("user32.dll", EntryPoint="SetPhysicalCursorPos")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
}
'@

[FigTetherDesktop]::SetProcessDPIAware() | Out-Null

$figmaProcesses = @(Get-Process Figma -ErrorAction SilentlyContinue)
$process = $figmaProcesses |
    Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
$windowHandle = if ($process) { $process.MainWindowHandle } else { [IntPtr]::Zero }
if ($windowHandle -eq [IntPtr]::Zero -or $UseDefaultDesktopDiscovery) {
    $windowHandle = [FigTetherDesktop]::FindDefaultFigmaWindow([int[]]@($figmaProcesses | ForEach-Object { $_.Id }))
    if ($windowHandle -ne [IntPtr]::Zero) {
        [uint32]$ownerProcessId = 0
        [FigTetherDesktop]::GetWindowThreadProcessId($windowHandle, [ref]$ownerProcessId) | Out-Null
        $process = Get-Process -Id $ownerProcessId
    }
}
if ($windowHandle -eq [IntPtr]::Zero) { throw 'No accessible Figma Desktop window is open on the Default desktop.' }
$window = [System.Windows.Automation.AutomationElement]::FromHandle($windowHandle)
[FigTetherDesktop]::ShowWindow($windowHandle, 9) | Out-Null
if ($Action -in @('Click', 'Drag', 'Keys')) {
    [FigTetherDesktop]::SetWindowPos($windowHandle, [IntPtr](-1), 0, 0, 0, 0, 3) | Out-Null
    [FigTetherDesktop]::SetWindowPos($windowHandle, [IntPtr](-2), 0, 0, 0, 0, 3) | Out-Null
}
[FigTetherDesktop]::SetForegroundWindow($windowHandle) | Out-Null
Start-Sleep -Milliseconds 250
$rectangle = $window.Current.BoundingRectangle

switch ($Action) {
    'Position' {
        [FigTetherDesktop]::SetWindowPos($windowHandle, [IntPtr]::Zero, 0, 0, 1416, 908, 0x40) | Out-Null
        Start-Sleep -Milliseconds 300
    }
    'Inspect' {
        [pscustomobject]@{ ProcessId = $process.Id; X = $rectangle.X; Y = $rectangle.Y; Width = $rectangle.Width; Height = $rectangle.Height } | Format-List
        $window.FindAll([System.Windows.Automation.TreeScope]::Descendants,
            [System.Windows.Automation.Condition]::TrueCondition) |
            Select-Object -First $InspectLimit | ForEach-Object {
                [pscustomobject]@{ Name = $_.Current.Name; Type = $_.Current.ControlType.ProgrammaticName; Id = $_.Current.AutomationId; Bounds = $_.Current.BoundingRectangle }
            } | Format-List
    }
    'Capture' {
        if (-not $Output) { throw 'Capture needs -Output in an ignored/local directory.' }
        $parent = Split-Path -Parent $Output
        if ($parent) { New-Item -ItemType Directory -Force -Path $parent | Out-Null }
        $bitmap = New-Object System.Drawing.Bitmap([int]$rectangle.Width, [int]$rectangle.Height)
        $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
        try {
            $hdc = $graphics.GetHdc()
            try { $success = [FigTetherDesktop]::PrintWindow($windowHandle, $hdc, 2) }
            finally { $graphics.ReleaseHdc($hdc) }
            if (-not $success) { throw 'Figma window capture failed.' }
            $bitmap.Save([System.IO.Path]::GetFullPath($Output), [System.Drawing.Imaging.ImageFormat]::Png)
        }
        finally { $graphics.Dispose(); $bitmap.Dispose() }
        Write-Output 'Figma window captured locally.'
    }
    'Click' {
        if ($X -lt 0 -or $Y -lt 0 -or $X -ge $rectangle.Width -or $Y -ge $rectangle.Height) {
            throw 'Coordinates must lie inside the Figma window.'
        }
        [FigTetherDesktop]::SetCursorPos([int]$rectangle.X + $X, [int]$rectangle.Y + $Y) | Out-Null
        [FigTetherDesktop]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero)
        [FigTetherDesktop]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 300
    }
    'Drag' {
        foreach ($point in @(@($X, $Y), @($ToX, $ToY))) {
            if ($point[0] -lt 0 -or $point[1] -lt 0 -or $point[0] -ge $rectangle.Width -or $point[1] -ge $rectangle.Height) { throw 'Drag must stay inside Figma.' }
        }
        [FigTetherDesktop]::SetCursorPos([int]$rectangle.X + $X, [int]$rectangle.Y + $Y) | Out-Null
        [FigTetherDesktop]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 100
        [FigTetherDesktop]::SetCursorPos([int]$rectangle.X + $ToX, [int]$rectangle.Y + $ToY) | Out-Null
        Start-Sleep -Milliseconds 100
        [FigTetherDesktop]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 300
    }
    'Keys' {
        if (-not $Keys) { throw 'Keys needs -Keys using SendKeys notation.' }
        $foreground = [FigTetherDesktop]::GetForegroundWindow()
        [uint32]$foregroundProcess = 0
        $foregroundThread = [FigTetherDesktop]::GetWindowThreadProcessId($foreground, [ref]$foregroundProcess)
        $currentThread = [FigTetherDesktop]::GetCurrentThreadId()
        if ($foregroundProcess -ne $process.Id) {
            [FigTetherDesktop]::AttachThreadInput($currentThread, $foregroundThread, $true) | Out-Null
            try { [FigTetherDesktop]::SetForegroundWindow($windowHandle) | Out-Null }
            finally { [FigTetherDesktop]::AttachThreadInput($currentThread, $foregroundThread, $false) | Out-Null }
        }
        $shell = New-Object -ComObject WScript.Shell
        if (-not $shell.AppActivate($process.Id)) { throw 'Could not activate Figma.' }
        Start-Sleep -Milliseconds 300
        [uint32]$actualProcess = 0
        [FigTetherDesktop]::GetWindowThreadProcessId([FigTetherDesktop]::GetForegroundWindow(), [ref]$actualProcess) | Out-Null
        if ($actualProcess -ne $process.Id) { throw 'Figma is not the foreground window; refusing keyboard input.' }
        $shell.SendKeys($Keys)
        Start-Sleep -Milliseconds 500
    }
    'Invoke' {
        $condition = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, $Name)
        $matches = $window.FindAll([System.Windows.Automation.TreeScope]::Descendants, $condition)
        $element = $matches | Where-Object { @($_.GetSupportedPatterns() | Where-Object { $_.ProgrammaticName -match 'Invoke|ExpandCollapse|SelectionItem|Toggle' }).Count -gt 0 -or $_.Current.ClassName -eq 'Button' } | Select-Object -First 1
        if (-not $element) { $element = $matches | Select-Object -First 1 }
        if (-not $element) { throw "Figma control not found: $Name" }
        $pattern = $null
        if ($element.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
            $pattern.Invoke()
        } elseif ($element.TryGetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern, [ref]$pattern)) {
            if ($pattern.Current.ExpandCollapseState -eq [System.Windows.Automation.ExpandCollapseState]::Expanded) { $pattern.Collapse() }
            else { $pattern.Expand() }
        } elseif ($element.TryGetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern, [ref]$pattern)) {
            $pattern.Select()
        } elseif ($element.TryGetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern, [ref]$pattern)) {
            $pattern.Toggle()
        } elseif ($element.Current.ClassName -eq 'Button' -and $element.Current.NativeWindowHandle -ne 0) {
            [FigTetherDesktop]::SendMessage([IntPtr]$element.Current.NativeWindowHandle, 0xF5, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
        } else { throw "Figma control has no invoke or expand pattern: $Name" }
        Start-Sleep -Milliseconds 500
    }
    'SetValue' {
        $condition = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, $Name)
        $matches = $window.FindAll([System.Windows.Automation.TreeScope]::Descendants, $condition)
        $element = $matches | Where-Object { $_.Current.ControlType -eq [System.Windows.Automation.ControlType]::Edit } | Select-Object -First 1
        if (-not $element) { $element = $matches | Select-Object -First 1 }
        if (-not $element) { throw "Figma input not found: $Name" }
        $pattern = $null
        if ($element.TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern, [ref]$pattern)) {
            $pattern.SetValue($Value)
        } elseif ($Name -eq 'File name:') {
            $edit = $window.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition) |
                Where-Object { $_.Current.ClassName -eq 'Edit' -and $_.Current.AutomationId -eq '1148' } | Select-Object -First 1
            if (-not $edit) { throw 'No native file picker edit field.' }
            [FigTetherDesktop]::SendMessage([IntPtr]$edit.Current.NativeWindowHandle, 0xC, [IntPtr]::Zero, $Value) | Out-Null
        } else { throw "Figma input has no value pattern: $Name" }
        Start-Sleep -Milliseconds 500
    }
}
