<#
.SYNOPSIS
    Build agent + installer Replicator menjadi dua file siap pakai di dist\.

.DESCRIPTION
    Script ini menjalankan dua build berurutan:
        1. replicator-agent.exe
        2. replicator-setup.exe (agent disematkan ke dalam installer)

    Hasil akhir di folder dist\:
        - replicator-setup.exe   -> satu file untuk pengguna akhir (klik dua kali)
        - replicator-agent.exe   -> binary agent (opsional, untuk update manual)

    Installer membutuhkan replicator-agent.exe yang sudah dibangun. Karena itu
    agent selalu dibangun lebih dulu.

.PARAMETER Toolchain
    Nama toolchain rustup. Default 1.77.2-x86_64-pc-windows-gnu.

.PARAMETER MinGWPath
    Folder berisi gcc.exe. Kosong = deteksi otomatis (PATH, WinLibs, msys64).

.PARAMETER Clean
    Jalankan cargo clean pada agent dan installer sebelum build.

.PARAMETER NoCopy
    Jangan salin hasil build ke folder dist\.

.EXAMPLE
    .\scripts\build-installer.ps1

.EXAMPLE
    .\scripts\build-installer.ps1 -Clean
#>
[CmdletBinding()]
param(
    [string]$Toolchain = "1.77.2-x86_64-pc-windows-gnu",
    [string]$MinGWPath = "",
    [switch]$Clean,
    [switch]$NoCopy
)

$ErrorActionPreference = "Stop"
$scriptDir = if ($PSScriptRoot) { $PSScriptRoot } else { Split-Path -Parent $MyInvocation.MyCommand.Definition }
$root = Split-Path -Parent $scriptDir
$agentManifest = Join-Path $root "agent\Cargo.toml"
$installerManifest = Join-Path $root "installer\Cargo.toml"
$agentExe = Join-Path $root "agent\target\release\replicator-agent.exe"
$setupExe = Join-Path $root "installer\target\release\replicator-setup.exe"

function Find-MinGW {
    param([string]$Hint)

    if ($Hint) {
        if (Test-Path (Join-Path $Hint "gcc.exe")) {
            return (Resolve-Path $Hint).Path
        }
        throw "MinGWPath tidak berisi gcc.exe: $Hint"
    }

    $gcc = Get-Command gcc.exe -ErrorAction SilentlyContinue
    if ($gcc) {
        return (Split-Path -Parent $gcc.Source)
    }

    $patterns = @(
        (Join-Path $env:LOCALAPPDATA "Microsoft\WinGet\Packages\BrechtSanders.WinLibs*\mingw64\bin"),
        (Join-Path $env:LOCALAPPDATA "Microsoft\WinGet\Packages\*WinLibs*\mingw64\bin"),
        "C:\mingw64\bin",
        "C:\msys64\mingw64\bin"
    )
    foreach ($pattern in $patterns) {
        $found = Get-Item $pattern -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($found -and (Test-Path (Join-Path $found.FullName "gcc.exe"))) {
            return $found.FullName
        }
    }
    return $null
}

foreach ($manifest in @($agentManifest, $installerManifest)) {
    if (-not (Test-Path $manifest)) {
        throw "Tidak menemukan $manifest. Jalankan script dari folder proyek replikator."
    }
}

$cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
if (Test-Path (Join-Path $cargoBin "cargo.exe")) {
    $env:Path = "$cargoBin;$env:Path"
}

$cargo = Get-Command cargo.exe -ErrorAction SilentlyContinue
if (-not $cargo) {
    throw "cargo tidak ditemukan. Pasang Rust lewat https://rustup.rs lalu buka terminal baru."
}

if ($Toolchain -like "*windows-gnu*") {
    $mingw = Find-MinGW -Hint $MinGWPath
    if (-not $mingw) {
        throw ("MinGW (gcc.exe) tidak ditemukan. Pasang WinLibs, atau jalankan:" +
            "`n  winget install BrechtSanders.WinLibs.POSIX.MSVCRT" +
            "`nLalu ulangi script ini.")
    }
    $env:Path = "$mingw;$env:Path"
    Write-Host "MinGW    : $mingw"
}

$installed = (& rustup toolchain list) 2>$null
if ($LASTEXITCODE -eq 0 -and -not ($installed | Select-String -SimpleMatch $Toolchain)) {
    Write-Host ""
    Write-Host "Toolchain '$Toolchain' belum terpasang. Jalankan:" -ForegroundColor Yellow
    Write-Host "  rustup toolchain install $Toolchain --profile minimal" -ForegroundColor Yellow
    if ($Toolchain -like "*windows-gnu*") {
        Write-Host "  rustup target add x86_64-pc-windows-gnu --toolchain $Toolchain" -ForegroundColor Yellow
    }
    Write-Host ""
    throw "Toolchain $Toolchain tidak tersedia."
}

Write-Host "Toolchain: $Toolchain"

$sw = [System.Diagnostics.Stopwatch]::StartNew()

if ($Clean) {
    Write-Host "Membersihkan hasil build agent..."
    & cargo "+$Toolchain" clean --manifest-path $agentManifest
    if ($LASTEXITCODE -ne 0) { throw "cargo clean agent gagal." }
    Write-Host "Membersihkan hasil build installer..."
    & cargo "+$Toolchain" clean --manifest-path $installerManifest
    if ($LASTEXITCODE -ne 0) { throw "cargo clean installer gagal." }
}

Write-Host ""
Write-Host "== 1/2 Build agent ==" -ForegroundColor Cyan
& cargo "+$Toolchain" build --release --locked --manifest-path $agentManifest
if ($LASTEXITCODE -ne 0) {
    throw "Build agent gagal (exit code $LASTEXITCODE)."
}
if (-not (Test-Path $agentExe)) {
    throw "Build agent selesai tetapi $agentExe tidak ditemukan."
}

$dist = Join-Path $root "dist"
if (-not $NoCopy) {
    # Salin agent lebih dulu supaya installer menyematkan binary yang baru.
    New-Item -ItemType Directory -Force -Path $dist | Out-Null
    Copy-Item -Force $agentExe (Join-Path $dist "replicator-agent.exe")
}

Write-Host ""
Write-Host "== 2/2 Build installer (agent disematkan) ==" -ForegroundColor Cyan
& cargo "+$Toolchain" build --release --locked --manifest-path $installerManifest
if ($LASTEXITCODE -ne 0) {
    throw "Build installer gagal (exit code $LASTEXITCODE)."
}
if (-not (Test-Path $setupExe)) {
    throw "Build installer selesai tetapi $setupExe tidak ditemukan."
}

$sw.Stop()

$agent = Get-Item $agentExe
$setup = Get-Item $setupExe
$agentVersion = (& $agentExe --version) 2>$null

Write-Host ""
Write-Host "Selesai dalam $([math]::Round($sw.Elapsed.TotalSeconds, 1)) detik." -ForegroundColor Green
Write-Host ""
Write-Host "Agent"
Write-Host "  File   : $($agent.FullName)"
Write-Host "  Ukuran : $([math]::Round($agent.Length / 1MB, 2)) MB"
Write-Host "  Versi  : $agentVersion"
Write-Host "Installer"
Write-Host "  File   : $($setup.FullName)"
Write-Host "  Ukuran : $([math]::Round($setup.Length / 1MB, 2)) MB"

if (-not $NoCopy) {
    $dist = Join-Path $root "dist"
    New-Item -ItemType Directory -Force -Path $dist | Out-Null
    $agentTarget = Join-Path $dist "replicator-agent.exe"
    $setupTarget = Join-Path $dist "replicator-setup.exe"
    Copy-Item -Force $agent.FullName $agentTarget
    Copy-Item -Force $setup.FullName $setupTarget

    Write-Host ""
    Write-Host "Disalin ke dist\:" -ForegroundColor Green
    foreach ($file in @($agentTarget, $setupTarget)) {
        $item = Get-Item $file
        $hash = (Get-FileHash -Algorithm SHA256 $file).Hash
        Write-Host ("  {0}  {1,8:N2} MB  SHA256 {2}" -f $item.Name, ($item.Length / 1MB), $hash)
    }
}

Write-Host ""
Write-Host "Yang dibagikan ke pengguna akhir hanya: dist\replicator-setup.exe"
