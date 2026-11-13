<#
.SYNOPSIS
    Makes Daily Ships appear shortly after you log on to Windows.

.DESCRIPTION
    Copies daily-ships.exe to %LOCALAPPDATA%\Programs\DailyShips and registers a
    per-user scheduled task ("Daily Ships") that starts it 20 seconds after log on.
    The task runs only for the current user, with normal (non-elevated) rights,
    and only while you are logged on. Nothing keeps running afterwards: the app
    shows its popup, closes after about 30 seconds and the process exits.

    Run scripts\uninstall-startup.ps1 to undo everything this script does.

.PARAMETER ExePath
    Path to daily-ships.exe. Defaults to ..\target\release\daily-ships.exe
    (the cargo release build), then ..\dist\daily-ships.exe (the prebuilt
    copy), then a copy next to this script.

.PARAMETER DelaySeconds
    Seconds to wait after log on before the popup appears. Default: 20.

.PARAMETER NoCopy
    Start the executable from where it is instead of copying it to
    %LOCALAPPDATA%\Programs\DailyShips. Do not move or delete it afterwards.

.PARAMETER UseRunKey
    Do not use Task Scheduler. Instead add an entry to the current user's
    "Run" registry key (HKCU\Software\Microsoft\Windows\CurrentVersion\Run)
    that starts the app with "--delay <seconds>". Use this only if Windows
    refuses to let your account create the scheduled task.

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts\install-startup.ps1

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts\install-startup.ps1 -DelaySeconds 45

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts\install-startup.ps1 -WhatIf
#>
[CmdletBinding(SupportsShouldProcess = $true)]
param(
    [string]$ExePath,
    [ValidateRange(0, 600)]
    [int]$DelaySeconds = 20,
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
        Write-Host "  Program : $target"
        Write-Host "  Remove  : scripts\uninstall-startup.ps1"
    }
    exit 0
}

# --- 3b. Default: scheduled task at log on ------------------------------------
if ($PSCmdlet.ShouldProcess("Scheduled task '$TaskName' for $user",
        "Register (at log on, delay $DelaySeconds s, run '$target')")) {
    try {
        $action  = New-ScheduledTaskAction -Execute $target -WorkingDirectory (Split-Path -Parent $target)
        $trigger = New-ScheduledTaskTrigger -AtLogOn -User $user
        if ($DelaySeconds -gt 0) {
            $trigger.Delay = "PT${DelaySeconds}S"   # ISO 8601 duration
        }
        # Interactive + Limited: runs only while this user is logged on, never elevated.
        $principal = New-ScheduledTaskPrincipal -UserId $user -LogonType Interactive -RunLevel Limited
        $settings  = New-ScheduledTaskSettingsSet `
            -AllowStartIfOnBatteries `
            -DontStopIfGoingOnBatteries `
            -MultipleInstances IgnoreNew `
            -ExecutionTimeLimit (New-TimeSpan -Minutes 10)

        Register-ScheduledTask -TaskName $TaskName `
            -Description 'Shows one historical ship in a small popup shortly after log on, then exits.' `
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
    Write-Host "  Task    : $TaskName (current user only: $user)"
    Write-Host "  Program : $target"
    Write-Host "  Try now : Start-ScheduledTask -TaskName '$TaskName'"
    Write-Host "  Remove  : scripts\uninstall-startup.ps1"
}
