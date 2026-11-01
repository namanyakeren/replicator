<#
.SYNOPSIS
    Pasang agent replikator sebagai Scheduled Task yang berjalan saat Windows
    menyala dan otomatis dijalankan ulang bila prosesnya berhenti.

.DESCRIPTION
    Task dibuat memakai schtasks.exe + XML agar kompatibel dengan Windows 7
    (modul ScheduledTasks baru tersedia mulai Windows 8).

    Default:
      - berjalan sebagai SYSTEM (tidak perlu login)
      - trigger saat boot + delay 30 detik
      - restart otomatis tiap 1 menit bila proses gagal
      - tanpa batas waktu eksekusi

    Jalankan PowerShell sebagai Administrator.

.PARAMETER ExePath
    Path replicator-agent.exe. Default: dist\replicator-agent.exe, lalu
    agent\target\release\replicator-agent.exe.

.PARAMETER ConfigPath
    Path config agent. Default: config\agent.ini, lalu config.ini lama.

.PARAMETER TaskName
    Nama task. Default "Replicator Agent".

.PARAMETER NoStart
    Jangan langsung menjalankan task setelah dipasang.

.EXAMPLE
    .\scripts\install-task.ps1

.EXAMPLE
    .\scripts\install-task.ps1 -ExePath C:\replicator\dist\replicator-agent.exe -ConfigPath C:\replicator\config.ini
#>
[CmdletBinding()]
param(
    [string]$ExePath = "",
    [string]$ConfigPath = "",
    [string]$TaskName = "Replicator Agent",
    [switch]$NoStart
)

$ErrorActionPreference = "Stop"
$scriptDir = if ($PSScriptRoot) { $PSScriptRoot } else { Split-Path -Parent $MyInvocation.MyCommand.Definition }
$root = Split-Path -Parent $scriptDir

$principal = New-Object Security.Principal.WindowsPrincipal(
    [Security.Principal.WindowsIdentity]::GetCurrent()
)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw "Jalankan PowerShell sebagai Administrator (klik kanan > Run as administrator)."
}

if (-not $ExePath) {
    $candidates = @(
        (Join-Path $root "dist\replicator-agent.exe"),
        (Join-Path $root "agent\target\release\replicator-agent.exe")
    )
    foreach ($candidate in $candidates) {
        if (Test-Path $candidate) { $ExePath = $candidate; break }
    }
}
if (-not $ExePath -or -not (Test-Path $ExePath)) {
    throw "replicator-agent.exe tidak ditemukan. Build dulu dengan .\scripts\build-agent.ps1 atau isi -ExePath."
}
$ExePath = (Resolve-Path $ExePath).Path

if (-not $ConfigPath) {
    $newConfig = Join-Path $root "config\agent.ini"
    $legacyConfig = Join-Path $root "config.ini"
    if (Test-Path $newConfig) {
        $ConfigPath = $newConfig
    } elseif (Test-Path $legacyConfig) {
        $ConfigPath = $legacyConfig
    } else {
        $ConfigPath = $newConfig
    }
}
if (-not (Test-Path $ConfigPath)) {
    throw "Config agent tidak ditemukan: $ConfigPath. Salin config\agent.example.ini menjadi config\agent.ini."
}
$ConfigPath = (Resolve-Path $ConfigPath).Path

$workingDir = Split-Path -Parent $ConfigPath
$arguments = '--config "' + $ConfigPath + '"'

function XmlEscape([string]$value) {
    return [System.Security.SecurityElement]::Escape($value)
}

$xml = @"
<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>Replikator database MySQL: sinkronisasi master ke backup.</Description>
    <Author>replikator</Author>
  </RegistrationInfo>
  <Triggers>
    <BootTrigger>
      <Enabled>true</Enabled>
      <Delay>PT30S</Delay>
    </BootTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <UserId>S-1-5-18</UserId>
      <LogonType>ServiceAccount</LogonType>
      <RunLevel>HighestAvailable</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>true</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <IdleSettings>
      <StopOnIdleEnd>false</StopOnIdleEnd>
      <RestartOnIdle>false</RestartOnIdle>
    </IdleSettings>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <RunOnlyIfIdle>false</RunOnlyIfIdle>
    <WakeToRun>false</WakeToRun>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>7</Priority>
    <RestartOnFailure>
      <Interval>PT1M</Interval>
      <Count>999</Count>
    </RestartOnFailure>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>$(XmlEscape $ExePath)</Command>
      <Arguments>$(XmlEscape $arguments)</Arguments>
      <WorkingDirectory>$(XmlEscape $workingDir)</WorkingDirectory>
    </Exec>
  </Actions>
</Task>
"@

$tmp = Join-Path $env:TEMP ("replicator-task-" + [Guid]::NewGuid().ToString("N") + ".xml")
try {
    $xml | Out-File -FilePath $tmp -Encoding Unicode

    & schtasks.exe /End /TN $TaskName 2>$null | Out-Null
    & schtasks.exe /Create /TN $TaskName /XML $tmp /F | Out-Host
    if ($LASTEXITCODE -ne 0) {
        throw "schtasks /Create gagal (exit code $LASTEXITCODE)."
    }
} finally {
    Remove-Item -Force -ErrorAction SilentlyContinue $tmp
}

Write-Host ""
Write-Host "Task '$TaskName' terpasang." -ForegroundColor Green
Write-Host "  EXE    : $ExePath"
Write-Host "  Config : $ConfigPath"
Write-Host "  Jalan  : saat boot (delay 30 dtk) sebagai SYSTEM"
Write-Host ""
Write-Host "Perintah berguna:"
Write-Host "  schtasks /Run   /TN `"$TaskName`"     # jalankan sekarang"
Write-Host "  schtasks /End   /TN `"$TaskName`"     # hentikan"
Write-Host "  schtasks /Query /TN `"$TaskName`" /V /FO LIST"
Write-Host "  .\scripts\uninstall-task.ps1           # hapus task"

if (-not $NoStart) {
    & schtasks.exe /Run /TN $TaskName | Out-Host
    if ($LASTEXITCODE -ne 0) {
        Write-Host "Task terpasang tetapi gagal dijalankan. Coba jalankan manual:" -ForegroundColor Yellow
        Write-Host "  `"$ExePath`" --config `"$ConfigPath`""
    }
}
