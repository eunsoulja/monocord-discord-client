<#
.SYNOPSIS
    Installs Monocord (Discord client) for the current user.

.DESCRIPTION
    Copies Monocord.exe to %LOCALAPPDATA%\Programs\Monocord, creates Start
    Menu and (optionally) Desktop shortcuts, and registers an uninstaller.
    No admin rights required. Requires the WebView2 Runtime (preinstalled on
    Windows 10/11; the script warns if it is missing).

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File install.ps1
    powershell -ExecutionPolicy Bypass -File install.ps1 -DesktopShortcut
#>
[CmdletBinding()]
param(
    # Path to Monocord.exe. Defaults to the release build in this repo,
    # falling back to the debug build.
    [string]$ExePath,
    # Also create a shortcut on the Desktop.
    [switch]$DesktopShortcut
)

$ErrorActionPreference = 'Stop'

# -- Locate the executable ---------------------------------------------------
$repoRoot = Split-Path -Parent $PSScriptRoot
if (-not $ExePath) {
    $candidates = @(
        (Join-Path $repoRoot 'target\release\Monocord.exe'),
        (Join-Path $repoRoot 'target\debug\Monocord.exe')
    )
    $ExePath = $candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $ExePath -or -not (Test-Path $ExePath)) {
    Write-Error "Monocord.exe not found. Build it first: cargo build --release -p Monocord`nOr pass -ExePath <path-to-exe>."
}
$ExePath = (Resolve-Path $ExePath).Path
Write-Host "Installing from: $ExePath"

# -- WebView2 runtime check ---------------------------------------------------
$wv2 = Get-ItemProperty 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}',
                       'HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}' -ErrorAction SilentlyContinue
if (-not $wv2) {
    Write-Warning 'WebView2 Runtime not detected. Monocord needs it — install from https://developer.microsoft.com/microsoft-edge/webview2/ if the app fails to start.'
}

# -- Install directory ---------------------------------------------------------
$installDir = Join-Path $env:LOCALAPPDATA 'Programs\Monocord'
New-Item -ItemType Directory -Force -Path $installDir | Out-Null
$destExe = Join-Path $installDir 'Monocord.exe'
Copy-Item $ExePath $destExe -Force
Write-Host "Installed to: $destExe"

# -- Uninstaller ----------------------------------------------------------------
$uninstallBat = Join-Path $installDir 'uninstall.cmd'
@"
@echo off
choice /c YN /m "Uninstall Monocord? This removes %installDir% and your shortcuts."
if errorlevel 2 exit /b 0
del /f /q "%~dp0Monocord.exe" 2>nul
del /f /q "%~dp0uninstall.cmd" 2>nul
rmdir /s /q "%~dp0" 2>nul
exit /b 0
"@ | Set-Content -Path $uninstallBat -Encoding ASCII

# -- Shortcuts ------------------------------------------------------------------
$shell = New-Object -ComObject WScript.Shell

$startMenuDir = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs'
$startMenuLnk = Join-Path $startMenuDir 'Monocord.lnk'
$sc = $shell.CreateShortcut($startMenuLnk)
$sc.TargetPath = $destExe
$sc.WorkingDirectory = $installDir
$sc.Description = 'Monocord — Discord client with plugins and themes'
$sc.Save()
Write-Host "Start Menu shortcut: $startMenuLnk"

$uninstallLnk = Join-Path $startMenuDir 'Uninstall Monocord.lnk'
$sc = $shell.CreateShortcut($uninstallLnk)
$sc.TargetPath = $uninstallBat
$sc.WorkingDirectory = $installDir
$sc.Description = 'Uninstall Monocord'
$sc.Save()

if ($DesktopShortcut) {
    $desktopLnk = Join-Path ([Environment]::GetFolderPath('Desktop')) 'Monocord.lnk'
    $sc = $shell.CreateShortcut($desktopLnk)
    $sc.TargetPath = $destExe
    $sc.WorkingDirectory = $installDir
    $sc.Description = 'Monocord — Discord client'
    $sc.Save()
    Write-Host "Desktop shortcut: $desktopLnk"
}

Write-Host ''
Write-Host 'Monocord installed successfully.'
Write-Host "Launch it from the Start Menu or: $destExe"
Write-Host 'Your Discord login and plugins live in %APPDATA%\monocord\monocord\data — they are not touched by uninstall.'
