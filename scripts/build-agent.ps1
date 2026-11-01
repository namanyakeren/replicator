<#
.SYNOPSIS
    Build agent replikator (Rust) untuk Windows 7-11.

.DESCRIPTION
    Secara default memakai toolchain GNU (MinGW) karena tidak membutuhkan
    Visual Studio C++ Build Tools:
        cargo +1.77.2-x86_64-pc-windows-gnu build --release

    Untuk build MSVC (butuh Visual Studio Build Tools + Windows SDK):
        .\scripts\build-agent.ps1 -Toolchain 1.77.2-x86_64-pc-windows-msvc

.PARAMETER Toolchain
    Nama toolchain rustup. Default 1.77.2-x86_64-pc-windows-gnu.

.PARAMETER MinGWPath
    Folder berisi gcc.exe. Kosong = deteksi otomatis (PATH, WinLibs, msys64).

.PARAMETER Clean
    Jalankan cargo clean sebelum build.

.PARAMETER NoCopy
    Jangan salin hasil build ke folder dist\.

.EXAMPLE
    .\scripts\build-agent.ps1

.EXAMPLE
    .\scripts\build-agent.ps1 -Toolchain 1.77.2-x86_64-pc-windows-msvc
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
$manifest = Join-Path $root "agent\Cargo.toml"
$exe = Join-Path $root "agent\target\release\replicator-agent.exe"

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

if (-not (Test-Path $manifest)) {
    throw "Tidak menemukan $manifest. Jalankan script dari folder proyek replikator."
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
Write-Host "Manifest : $manifest"

if ($Clean) {
    Write-Host "Membersihkan hasil build lama..."
    & cargo "+$Toolchain" clean --manifest-path $manifest
    if ($LASTEXITCODE -ne 0) { throw "cargo clean gagal." }
}

$sw = [System.Diagnostics.Stopwatch]::StartNew()
& cargo "+$Toolchain" build --release --locked --manifest-path $manifest
$code = $LASTEXITCODE
$sw.Stop()

if ($code -ne 0) {
    throw "Build gagal (exit code $code)."
}
if (-not (Test-Path $exe)) {
    throw "Build selesai tetapi $exe tidak ditemukan."
}

$item = Get-Item $exe
$version = (& $exe --version) 2>$null
Write-Host ""
Write-Host "Selesai dalam $([math]::Round($sw.Elapsed.TotalSeconds, 1)) detik." -ForegroundColor Green
Write-Host "Binary  : $($item.FullName)"
Write-Host "Ukuran  : $([math]::Round($item.Length / 1MB, 2)) MB"
Write-Host "Versi   : $version"

if (-not $NoCopy) {
    $dist = Join-Path $root "dist"
    New-Item -ItemType Directory -Force -Path $dist | Out-Null
    $target = Join-Path $dist "replicator-agent.exe"
    Copy-Item -Force $item.FullName $target
    Write-Host "Disalin : $target"
}

Write-Host ""
Write-Host "Uji cepat:"
Write-Host "  .\dist\replicator-agent.exe --config .\config\agent.ini --check"
