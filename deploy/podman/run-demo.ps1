<#
.SYNOPSIS
    Jalankan demo dashboard multi-site Replicator di Podman.

.DESCRIPTION
    Membuat jaringan, container MariaDB demo, mengimpor data tiga schema
    status, lalu menjalankan dashboard PHP. Dashboard dimulai TANPA site:
    site ditambahkan lewat menu "Site" dengan mengisi IP master dan IP slave.
    Default: http://localhost:8089/

.PARAMETER Port
    Port host untuk dashboard. Default 8089.

.PARAMETER Rebuild
    Paksa build ulang image dashboard.

.PARAMETER Keep
    Jangan hapus container demo yang sudah ada sebelum dijalankan ulang.
    Berguna untuk inspeksi; `podman run` akan gagal bila nama masih dipakai.

.EXAMPLE
    .\deploy\podman\run-demo.ps1

.EXAMPLE
    .\deploy\podman\run-demo.ps1 -Port 8090 -Rebuild
#>
[CmdletBinding()]
param(
    [int]$Port = 8089,
    [switch]$Rebuild,
    [switch]$Keep
)

$ErrorActionPreference = "Stop"
$scriptDir = if ($PSScriptRoot) { $PSScriptRoot } else { Split-Path -Parent $MyInvocation.MyCommand.Definition }
$root = Split-Path -Parent (Split-Path -Parent $scriptDir)
$dashboardImage = "replicator-dashboard:demo"
$network = "rep-demo-net"
$dbName = "rep-demo-db"
$dashName = "rep-demo-dashboard"
$tickName = "rep-demo-heartbeat"
$dbRoot = "DemoRoot2026!"

function Invoke-Podman {
    param([Parameter(Mandatory = $true)][string[]]$PodmanArgs)
    $previous = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        & podman @PodmanArgs
        $code = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $previous
    }
    if ($code -ne 0) {
        throw "podman $($PodmanArgs -join ' ') gagal (exit $code)."
    }
}

# PowerShell 5.1 mengubah stderr proses native menjadi ErrorRecord; dengan
# ErrorActionPreference = Stop itu mematikan skrip. Probe di bawah memang boleh
# gagal, jadi error preference diturunkan sementara.
function Invoke-PodmanQuiet {
    param([Parameter(Mandatory = $true)][string[]]$PodmanArgs)
    $previous = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        & podman @PodmanArgs *>&1 | Out-Null
        return $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $previous
    }
}

function Test-DbReady {
    $code = Invoke-PodmanQuiet @('exec', $dbName, 'mariadb', '-uroot', "-p$dbRoot", '-e', 'SELECT 1')
    return $code -eq 0
}

function Remove-DemoContainer {
    param([string]$Name)
    if ((Invoke-PodmanQuiet @('container', 'exists', $Name)) -eq 0) {
        Invoke-PodmanQuiet @('rm', '-f', $Name) | Out-Null
    }
}

Write-Host "== Replicator Podman demo ==" -ForegroundColor Cyan

if (-not (Get-Command podman -ErrorAction SilentlyContinue)) {
    throw "podman tidak ditemukan di PATH."
}

$machine = (& podman machine list --format "{{.Name}} {{.Running}}" 2>&1) -join "`n"
if ($machine -notmatch "true") {
    Write-Host "Menjalankan Podman machine..." -ForegroundColor Yellow
    & podman machine start
    if ($LASTEXITCODE -ne 0) {
        throw "Podman machine gagal dijalankan."
    }
}

if (-not $Keep) {
    Remove-DemoContainer $dashName
    Remove-DemoContainer $tickName
    Remove-DemoContainer $dbName
}

$netExists = Invoke-PodmanQuiet @('network', 'exists', $network)
if ($netExists -ne 0) {
    Invoke-Podman @('network', 'create', $network) | Out-Null
}

$imageExists = Invoke-PodmanQuiet @('image', 'exists', $dashboardImage)
if ($Rebuild -or $imageExists -ne 0) {
    Write-Host "Build image dashboard..." -ForegroundColor Yellow
    Invoke-Podman @('build', '-t', $dashboardImage, '-f', (Join-Path $scriptDir "Containerfile"), $root)
}

Write-Host "Menjalankan MariaDB demo..." -ForegroundColor Yellow
Invoke-Podman @('run', '-d', '--name', $dbName, '--network', $network,
    '--network-alias', 'db',
    '-e', "MARIADB_ROOT_PASSWORD=$dbRoot", '-e', 'MARIADB_DATABASE=mysql',
    'docker.io/library/mariadb:10.11') | Out-Null

Write-Host "Menunggu MariaDB siap..." -ForegroundColor Yellow
$ready = $false
for ($attempt = 1; $attempt -le 60; $attempt++) {
    # Jangan pakai `mariadb-admin ping`: perintah itu tetap exit 0 saat akses
    # ditolak, sehingga bisa lolos ketika server sementara inisialisasi masih jalan.
    if (Test-DbReady) {
        $ready = $true
        break
    }
    Start-Sleep -Seconds 2
}
if (-not $ready) {
    & podman logs --tail 40 $dbName 2>&1 | ForEach-Object { Write-Host $_ }
    throw "MariaDB tidak siap setelah 120 detik."
}

Write-Host "Mengimpor data demo multi-site..." -ForegroundColor Yellow
$sql = Join-Path $scriptDir "demo.sql"
$imported = $false
for ($attempt = 1; $attempt -le 3; $attempt++) {
    $previous = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        Get-Content -Raw $sql | & podman exec -i $dbName mariadb "-uroot" "-p$dbRoot" 2>&1 |
            ForEach-Object { Write-Host $_ }
        $code = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $previous
    }
    if ($code -eq 0) {
        $imported = $true
        break
    }
    Start-Sleep -Seconds 3
}
if (-not $imported) {
    throw "Impor demo.sql gagal."
}

Write-Host "Menjalankan heartbeat demo..." -ForegroundColor Yellow
$heartbeat = Join-Path $scriptDir "heartbeat.sh"
Invoke-Podman @('run', '-d', '--name', $tickName, '--network', $network,
    '-e', "MARIADB_ROOT_PASSWORD=$dbRoot",
    '-v', "${heartbeat}:/heartbeat.sh:ro",
    '--entrypoint', 'sh', 'docker.io/library/mariadb:10.11', '/heartbeat.sh') | Out-Null

Write-Host "Menjalankan dashboard..." -ForegroundColor Yellow
Invoke-Podman @('run', '-d', '--name', $dashName, '--network', $network, '-p', "${Port}:80", $dashboardImage) | Out-Null

$url = "http://localhost:$Port/"
$up = $false
for ($attempt = 1; $attempt -le 30; $attempt++) {
    try {
        $response = Invoke-WebRequest -Uri $url -UseBasicParsing -TimeoutSec 3
        if ($response.StatusCode -eq 200) {
            $up = $true
            break
        }
    } catch {
        Start-Sleep -Seconds 1
    }
}

Write-Host ""
if ($up) {
    Write-Host "Dashboard siap: $url" -ForegroundColor Green
} else {
    Write-Host "Container berjalan tetapi HTTP belum merespons. Cek: podman logs $dashName" -ForegroundColor Yellow
}
Write-Host "Registry  : dashboard mulai tanpa site. Buka menu 'Site' lalu isi IP master/slave."
Write-Host "Contoh    : Jakarta  192.168.101.11 / 192.168.101.12  -> schema _rep_jakarta"
Write-Host "            Bandung  192.168.102.11 / 192.168.102.12  -> schema _rep_bandung"
Write-Host "            Surabaya 192.168.103.11 / 192.168.103.12  -> schema _rep_surabaya (stale)"
Write-Host "            Medan    192.169.101.11 / 192.169.101.12  -> schema _rep_medan (offline)"
Write-Host "File site : sites.ini di dalam container (bertahan saat stop/start, hilang saat container dibuat ulang)."
Write-Host "Heartbeat : container $tickName memperbarui Jakarta/Bandung tiap 15 detik."
Write-Host "Hentikan  : podman rm -f $dashName $tickName $dbName"
