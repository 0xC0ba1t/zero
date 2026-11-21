# Builds the OS and runs it in QEMU.
#
#   .\run.ps1            UEFI boot, 1920x1080 or larger (needs OVMF, bundled with QEMU)
#   .\run.ps1 -Bios      legacy BIOS boot (the BIOS bootloader is capped at 1280x720)
#   .\run.ps1 -Release   optimized build
param([switch]$Bios, [switch]$Release, [switch]$Whpx, [string]$Res = "")

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

# Default: QEMU's multi-threaded software CPU, which is the configuration this OS is tuned and tested on.
# -Whpx tries the Windows Hypervisor Platform instead; it is experimental here (with the interrupt
# controller emulated in user space every timer tick and port access is slow, so it can be slower).
$accel = if ($Whpx) { @("-accel", "whpx,kernel-irqchip=off", "-accel", "tcg,thread=multi") } else { @("-accel", "tcg,thread=multi") }
$args = $accel + @("-m", "256M", "-smp", "4", "-serial", "stdio", "-display", "gtk,zoom-to-fit=on,grab-on-hover=on",
          "-audiodev", "dsound,id=snd0", "-device", "AC97,audiodev=snd0")
if ($Bios) {
    $args += @("-drive", "format=raw,file=$images\bios.img")
} else {
    $vars = Join-Path $images "uefi-vars.fd"
    if (-not (Test-Path $vars)) { Copy-Item "$share\edk2-i386-vars.fd" $vars }
    $args += @("-drive", "if=pflash,format=raw,readonly=on,file=$share\edk2-x86_64-code.fd",
               "-drive", "if=pflash,format=raw,file=$vars",
               "-drive", "format=raw,file=$images\uefi.img")
}
if ($Res -match "^(\d+)x(\d+)$") { $args += @("-vga", "none", "-device", "VGA,xres=$($Matches[1]),yres=$($Matches[2])") }
& $qemu @args
