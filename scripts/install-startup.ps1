<#
.SYNOPSIS
    Makes Daily Ships appear shortly after you log on to Windows and then
    again at intervals through the day.

.DESCRIPTION
    Copies daily-ships.exe to %LOCALAPPDATA%\Programs\DailyShips and registers a
    per-user scheduled task ("Daily Ships") that starts it 20 seconds after log on
    and then about once an hour, at any time of the day, for as long as you are
    logged on. The task runs only for the current user, with normal
    (non-elevated) rights. Nothing keeps running in between: each time, the app
    shows its popup, closes after about 30 seconds and the process exits.

    The app itself skips a run (exits immediately, showing nothing) while a
    full-screen application is in front or the PC is locked, and for the rest
    of the day after you click "Don't show today".

    Run scripts\uninstall-startup.ps1 to undo everything this script does.

.PARAMETER ExePath
    Path to daily-ships.exe. Defaults to ..\target\release\daily-ships.exe
    (the cargo release build), then ..\dist\daily-ships.exe (the prebuilt
    copy), then a copy next to this script.

.PARAMETER DelaySeconds
    Seconds to wait after log on before the popup appears. Default: 20.

.PARAMETER IntervalMinutes
    Minutes between popups during the day. Default: 60. Use 0 to show the
    popup only once, after log on. Each repeat is started up to
    RandomDelayMinutes late so that it does not arrive on the dot.

.PARAMETER RandomDelayMinutes
    Upper limit of the random extra wait added to each repeat. Default: 10.
    It is capped at half the interval. Use 0 for exact timing.

.PARAMETER NoCopy
    Start the executable from where it is instead of copying it to
    %LOCALAPPDATA%\Programs\DailyShips. Do not move or delete it afterwards.

.PARAMETER UseRunKey
    Do not use Task Scheduler. Instead add an entry to the current user's
    "Run" registry key (HKCU\Software\Microsoft\Windows\CurrentVersion\Run)
    that starts the app with "--delay <seconds>". Use this only if Windows
    refuses to let your account create the scheduled task. The Run key can
    only start the app at log on: there are no repeats during the day.

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts\install-startup.ps1

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts\install-startup.ps1 -DelaySeconds 45

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts\install-startup.ps1 -IntervalMinutes 30

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts\install-startup.ps1 -WhatIf
#>
[CmdletBinding(SupportsShouldProcess = $true)]
param(
    [string]$ExePath,
    [ValidateRange(0, 600)]
    [int]$DelaySeconds = 20,
    [ValidateRange(0, 1440)]
    [int]$IntervalMinutes = 60,
    [ValidateRange(0, 720)]
    [int]$RandomDelayMinutes = 10,
    [switch]$NoCopy,
    [switch]$UseRunKey
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$TaskName     = 'Daily Ships'
$RunKeyPath   = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$RunValueName = 'DailyShips'
$InstallDir   = Join-Path $env:LOCALAPPDATA 'Programs\DailyShips'

function Fail([string]$Message) {
    Write-Host "ERROR: $Message" -ForegroundColor Red
    exit 1
}

# --- 1. Find the executable -------------------------------------------------
if (-not $ExePath) {
    $candidates = @(
        (Join-Path $PSScriptRoot '..\target\release\daily-ships.exe'),
        (Join-Path $PSScriptRoot '..\dist\daily-ships.exe'),
        (Join-Path $PSScriptRoot 'daily-ships.exe'),
        (Join-Path $PSScriptRoot '..\daily-ships.exe')
    )
    $ExePath = $candidates | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
    if (-not $ExePath) {
        Fail ("daily-ships.exe was not found. Build it first with 'cargo build --release', " +
              "or pass its location with -ExePath.")
    }
}
if (-not (Test-Path -LiteralPath $ExePath -PathType Leaf)) {
    Fail "The file '$ExePath' does not exist."
}
$ExePath = (Resolve-Path -LiteralPath $ExePath).Path

# --- 2. Copy it somewhere stable ---------------------------------------------
$target = $ExePath
if (-not $NoCopy) {
    $target = Join-Path $InstallDir 'daily-ships.exe'
    if ($target -ne $ExePath -and $PSCmdlet.ShouldProcess($target, 'Copy daily-ships.exe')) {
        try {
            New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
            Copy-Item -LiteralPath $ExePath -Destination $target -Force
        }
        catch {
            Fail ("Could not copy the executable to '$target': $($_.Exception.Message) " +
                  "If the popup is open right now, close it and run this script again.")
        }
    }
}

$user = [System.Security.Principal.WindowsIdentity]::GetCurrent().Name

# --- 3a. Fallback: per-user Run key ------------------------------------------
if ($UseRunKey) {
    $command = '"{0}" --delay {1}' -f $target, $DelaySeconds
    if ($PSCmdlet.ShouldProcess("$RunKeyPath\$RunValueName", "Set to: $command")) {
        try {
            if (-not (Test-Path $RunKeyPath)) { New-Item -Path $RunKeyPath -Force | Out-Null }
            Set-ItemProperty -Path $RunKeyPath -Name $RunValueName -Value $command -Type String
            # Avoid two popups if the scheduled task exists from an earlier install.
            if (Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue) {
                Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false
            }
        }
        catch {
            Fail "Could not write the Run key entry: $($_.Exception.Message)"
        }
        Write-Host "Installed (Run key). Daily Ships will appear about $DelaySeconds seconds after you log on."
        Write-Host "  Note    : the Run key cannot repeat during the day; -IntervalMinutes is ignored."
        Write-Host "  Program : $target"
        Write-Host "  Remove  : scripts\uninstall-startup.ps1"
    }
    exit 0
}

# --- 3b. Default: scheduled task at log on, repeating through the day ---------
if ($IntervalMinutes -gt 0 -and $IntervalMinutes -lt 5) {
    Fail 'IntervalMinutes must be 0 (log on only) or at least 5.'
}
$random = [Math]::Min($RandomDelayMinutes, [Math]::Floor($IntervalMinutes / 2))
$when = if ($IntervalMinutes -gt 0) { "at log on, then every $IntervalMinutes min" } else { 'at log on only' }

if ($PSCmdlet.ShouldProcess("Scheduled task '$TaskName' for $user",
        "Register ($when, delay $DelaySeconds s, run '$target')")) {
    try {
        $action  = New-ScheduledTaskAction -Execute $target -WorkingDirectory (Split-Path -Parent $target)
        $logon   = New-ScheduledTaskTrigger -AtLogOn -User $user
        if ($DelaySeconds -gt 0) {
            $logon.Delay = "PT${DelaySeconds}S"   # ISO 8601 duration
        }
        $trigger = @($logon)
        if ($IntervalMinutes -gt 0) {
            # Repeats forever, counted from midnight today, so popups can
            # arrive at any time of the day or night while you are logged on.
            $repeatArgs = @{
                Once               = $true
                At                 = (Get-Date).Date
                RepetitionInterval = (New-TimeSpan -Minutes $IntervalMinutes)
            }
            if ($random -gt 0) { $repeatArgs.RandomDelay = (New-TimeSpan -Minutes $random) }
            $trigger += New-ScheduledTaskTrigger @repeatArgs
        }
        # Interactive + Limited: runs only while this user is logged on, never elevated.
        $principal = New-ScheduledTaskPrincipal -UserId $user -LogonType Interactive -RunLevel Limited
        $settings  = New-ScheduledTaskSettingsSet `
            -AllowStartIfOnBatteries `
            -DontStopIfGoingOnBatteries `
            -MultipleInstances IgnoreNew `
            -ExecutionTimeLimit (New-TimeSpan -Minutes 10)

        Register-ScheduledTask -TaskName $TaskName `
            -Description 'Shows one Second World War warship in a small popup after log on and through the day, then exits.' `
            -Action $action -Trigger $trigger -Principal $principal -Settings $settings -Force | Out-Null

        # Avoid two popups if the Run key fallback was used earlier.
        if (Get-ItemProperty -Path $RunKeyPath -Name $RunValueName -ErrorAction SilentlyContinue) {
            Remove-ItemProperty -Path $RunKeyPath -Name $RunValueName
        }
    }
    catch {
        Write-Host "ERROR: Windows did not allow the scheduled task to be created:" -ForegroundColor Red
        Write-Host "       $($_.Exception.Message)" -ForegroundColor Red
        Write-Host ''
        Write-Host 'Some Windows configurations only let administrators create log-on tasks.'
        Write-Host 'You have two options:'
        Write-Host '  1. Run this script again from an elevated ("Run as administrator") PowerShell.'
        Write-Host '     The task is still created for your account only and still runs without'
        Write-Host '     administrator rights.'
        Write-Host '  2. Run this script again with -UseRunKey, which needs no special rights.'
        exit 1
    }

    Write-Host "Installed. Daily Ships will appear about $DelaySeconds seconds after you log on."
    if ($IntervalMinutes -gt 0) {
        Write-Host "  Repeats : about every $IntervalMinutes minutes, day and night, while you are logged on"
        Write-Host "            (skipped while a full-screen app is in front or the PC is locked)"
    }
    Write-Host "  Task    : $TaskName (current user only: $user)"
    Write-Host "  Program : $target"
    Write-Host "  Try now : Start-ScheduledTask -TaskName '$TaskName'"
    Write-Host "  Remove  : scripts\uninstall-startup.ps1"
}
