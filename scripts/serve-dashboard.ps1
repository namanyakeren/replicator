<#
.SYNOPSIS
    Jalankan dashboard PHP memakai web server bawaan PHP (untuk uji coba).

.DESCRIPTION
    Web server bawaan PHP hanya untuk pengujian atau jaringan internal kecil.
    Untuk produksi gunakan IIS (FastCGI) atau Apache. Document root harus
    folder dashboard\ agar config.ini di folder induk tidak bisa diunduh.

.PARAMETER Port
    Port HTTP. Default 8080.

.PARAMETER Address
    Alamat bind. Default 0.0.0.0 (bisa diakses dari komputer lain).

.PARAMETER PhpPath
    Path php.exe. Kosong = cari di PATH, C:\php\php.exe, C:\php74\php.exe.

.EXAMPLE
    .\scripts\serve-dashboard.ps1

.EXAMPLE
    .\scripts\serve-dashboard.ps1 -Port 9090 -Address 127.0.0.1
#>
[CmdletBinding()]
param(
    [int]$Port = 8080,
    [string]$Address = "0.0.0.0",
    [string]$PhpPath = ""
)

$ErrorActionPreference = "Stop"
$scriptDir = if ($PSScriptRoot) { $PSScriptRoot } else { Split-Path -Parent $MyInvocation.MyCommand.Definition }
$root = Split-Path -Parent $scriptDir
$docroot = Join-Path $root "dashboard"

if (-not (Test-Path (Join-Path $docroot "index.php"))) {
    throw "Folder dashboard tidak ditemukan di $docroot."
}

if (-not $PhpPath) {
    $php = Get-Command php.exe -ErrorAction SilentlyContinue
    if ($php) {
        $PhpPath = $php.Source
    } else {
        $candidates = @("C:\php\php.exe", "C:\php74\php.exe", "C:\php80\php.exe")
        foreach ($candidate in $candidates) {
            if (Test-Path $candidate) { $PhpPath = $candidate; break }
        }
    }
}
if (-not $PhpPath -or -not (Test-Path $PhpPath)) {
    throw "php.exe tidak ditemukan. Isi -PhpPath, misalnya -PhpPath C:\php\php.exe"
}

$modules = (& $PhpPath -m) 2>$null
if (-not ($modules | Select-String -SimpleMatch "pdo_mysql")) {
    Write-Host "PERINGATAN: ekstensi pdo_mysql tidak aktif di PHP ini." -ForegroundColor Yellow
    Write-Host "Aktifkan 'extension=pdo_mysql' di php.ini." -ForegroundColor Yellow
    Write-Host ""
}
if (Test-Path (Join-Path $root "config\dashboard.ini")) {
    Write-Host "Config  : $(Join-Path $root 'config\dashboard.ini')"
} elseif (Test-Path (Join-Path $root "config.ini")) {
    Write-Host "Config  : $(Join-Path $root 'config.ini') (mode lama single-site)"
} else {
    Write-Host "PERINGATAN: config\dashboard.ini tidak ditemukan." -ForegroundColor Yellow
    Write-Host "Salin config\dashboard.example.ini menjadi config\dashboard.ini." -ForegroundColor Yellow
}

Write-Host "PHP     : $PhpPath"
Write-Host "Docroot : $docroot"
Write-Host "URL     : http://localhost:$Port/"
if ($Address -eq "0.0.0.0") {
    Write-Host "          (juga bisa diakses lewat IP komputer ini)"
}
Write-Host ""
Write-Host "Tekan Ctrl+C untuk berhenti." -ForegroundColor DarkGray
Write-Host ""

& $PhpPath -S "${Address}:${Port}" -t $docroot
