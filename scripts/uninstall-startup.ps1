<#
.SYNOPSIS
    Stops Daily Ships from starting at log on.

.DESCRIPTION
    Removes the "Daily Ships" scheduled task and the Run-key entry (whichever
    install-startup.ps1 created). By default the copied program and your saved
    state are left in place.

.PARAMETER RemoveFiles
    Also delete the program copy in %LOCALAPPDATA%\Programs\DailyShips.

.PARAMETER RemoveData
    Also delete the saved state and error log in %LOCALAPPDATA%\DailyShips.

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts\uninstall-startup.ps1

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts\uninstall-startup.ps1 -RemoveFiles -RemoveData
#>
[CmdletBinding(SupportsShouldProcess = $true)]
param(
    [switch]$RemoveFiles,
    [switch]$RemoveData
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$TaskName     = 'Daily Ships'
$RunKeyPath   = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$RunValueName = 'DailyShips'
$InstallDir   = Join-Path $env:LOCALAPPDATA 'Programs\DailyShips'
$DataDir      = Join-Path $env:LOCALAPPDATA 'DailyShips'

$failed = $false

# Scheduled task
if (Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue) {
    if ($PSCmdlet.ShouldProcess("Scheduled task '$TaskName'", 'Remove')) {
        try {
            Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false
            Write-Host "Removed the scheduled task '$TaskName'."
        }
        catch {
            $failed = $true
            Write-Host "ERROR: Could not remove the scheduled task: $($_.Exception.Message)" -ForegroundColor Red
            Write-Host '       If it was created from an elevated PowerShell, remove it from one as well.'
        }
    }
}
else {
    Write-Host "No scheduled task named '$TaskName' was found."
}

# Run key entry (fallback install method)
if (Get-ItemProperty -Path $RunKeyPath -Name $RunValueName -ErrorAction SilentlyContinue) {
    if ($PSCmdlet.ShouldProcess("$RunKeyPath\$RunValueName", 'Remove')) {
        try {
            Remove-ItemProperty -Path $RunKeyPath -Name $RunValueName
            Write-Host 'Removed the Run key entry.'
        }
        catch {
            $failed = $true
            Write-Host "ERROR: Could not remove the Run key entry: $($_.Exception.Message)" -ForegroundColor Red
        }
    }
}

# Optional clean-up
$folders = @()
if ($RemoveFiles) { $folders += $InstallDir }
if ($RemoveData)  { $folders += $DataDir }
foreach ($folder in $folders) {
    if (Test-Path -LiteralPath $folder) {
        if ($PSCmdlet.ShouldProcess($folder, 'Delete folder')) {
            try {
                Remove-Item -LiteralPath $folder -Recurse -Force
                Write-Host "Deleted $folder"
            }
            catch {
                $failed = $true
                Write-Host "ERROR: Could not delete '$folder': $($_.Exception.Message)" -ForegroundColor Red
                Write-Host '       If the popup is open right now, close it and try again.'
            }
        }
    }
}

if ($failed) { exit 1 }
Write-Host 'Daily Ships will no longer start at log on.'
