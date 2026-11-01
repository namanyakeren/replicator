<#
.SYNOPSIS
    Hentikan dan hapus Scheduled Task agent replikator.

.PARAMETER TaskName
    Nama task. Default "Replicator Agent".

.EXAMPLE
    .\scripts\uninstall-task.ps1
#>
[CmdletBinding()]
param(
    [string]$TaskName = "Replicator Agent"
)

$ErrorActionPreference = "Stop"
$principal = New-Object Security.Principal.WindowsPrincipal(
    [Security.Principal.WindowsIdentity]::GetCurrent()
)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw "Jalankan PowerShell sebagai Administrator (klik kanan > Run as administrator)."
}

& schtasks.exe /End /TN $TaskName 2>$null | Out-Null
& schtasks.exe /Delete /TN $TaskName /F | Out-Host
if ($LASTEXITCODE -ne 0) {
    Write-Host "Task '$TaskName' tidak ada atau gagal dihapus." -ForegroundColor Yellow
    exit 1
}

Write-Host ""
Write-Host "Task '$TaskName' dihapus. File agent dan config tidak diubah." -ForegroundColor Green
