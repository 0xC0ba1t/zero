# Builds the OS and runs it in QEMU.
#
#   .\run.ps1            UEFI boot, 1920x1080 or larger (needs OVMF, bundled with QEMU)
#   .\run.ps1 -Bios      legacy BIOS boot (the BIOS bootloader is capped at 1280x720)
#   .\run.ps1 -Release   optimized build
param([switch]$Bios, [switch]$Release)

$ErrorActionPreference = "Stop"
$cargo = Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe"
if (-not (Test-Path $cargo)) { $cargo = "cargo" }

$profile = if ($Release) { "release" } else { "debug" }
if ($Release) { & $cargo build --release } else { & $cargo build }
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$qemu = (Get-Command qemu-system-x86_64 -ErrorAction SilentlyContinue).Source
if (-not $qemu) { $qemu = "C:\Program Files\qemu\qemu-system-x86_64.exe" }
$share = Join-Path (Split-Path $qemu) "share"
$images = Join-Path $PSScriptRoot "target\x86_64-zero\$profile"

$args = @("-m", "256M", "-serial", "stdio", "-display", "gtk,zoom-to-fit=on")
if ($Bios) {
    $args += @("-drive", "format=raw,file=$images\bios.img")
} else {
    $vars = Join-Path $images "uefi-vars.fd"
    if (-not (Test-Path $vars)) { Copy-Item "$share\edk2-i386-vars.fd" $vars }
    $args += @("-drive", "if=pflash,format=raw,readonly=on,file=$share\edk2-x86_64-code.fd",
               "-drive", "if=pflash,format=raw,file=$vars",
               "-drive", "format=raw,file=$images\uefi.img")
}
& $qemu @args
