param(
    [switch]$Debug
)

$ErrorActionPreference = "Stop"

function New-Directory([string]$Path) {
    if (-not (Test-Path $Path)) {
        New-Item -ItemType Directory -Path $Path | Out-Null
    }
}

# ------------------------------------------------------------------------------------
# Paths / config (assumes you run this from OS_Build)
# ------------------------------------------------------------------------------------
$osBuildRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$kernelRoot  = Join-Path $osBuildRoot "Rust-Kernel"
$buildDir    = Join-Path $osBuildRoot "build"
$bootDir     = Join-Path $buildDir   "boot"

New-Directory $buildDir
New-Directory $bootDir

$targetTriple = "x86_64-unknown-none"
$profile      = if ($Debug) { "debug" } else { "release" }

$kernelElf = Join-Path $kernelRoot "target\$targetTriple\$profile\rust-kernel"
$kernelBin = Join-Path $buildDir   "kernel.bin"
$mbrAsm    = Join-Path $osBuildRoot "assembly\mbr_stage1.asm"
$stage2Asm = Join-Path $osBuildRoot "assembly\stage2.asm"

$mbrBin    = Join-Path $bootDir "mbr_stage1.bin"
$stage2Bin = Join-Path $bootDir "stage2.bin"
(Get-Item $stage2Bin).Length
$diskImg   = Join-Path $buildDir "disk.img"
$diskSizeMiB = 64

# IMPORTANT: keep this in sync with STAGE2_SECTORS in BOTH mbr_stage1.asm and stage2.asm
# Right now your stage2.asm has STAGE2_SECTORS equ 4, but mbr_stage1.asm has 8.
# Pick one value (4 or 8), set it in BOTH .asm files, and set it here to match.
$STAGE2_SECTORS = 8

Write-Host ">> OS_Build root: $osBuildRoot"
Write-Host ">> Kernel root:   $kernelRoot"
Write-Host ">> Build dir:     $buildDir"
Write-Host ">> Boot dir:      $bootDir"
Write-Host ">> Disk image:    $diskImg"
Write-Host ""

# ------------------------------------------------------------------------------------
# 1. Build Rust kernel
# ------------------------------------------------------------------------------------
Write-Host "==> Building Rust kernel ($profile)..."

Push-Location $kernelRoot

$cargoArgs = @("build", "--target", $targetTriple)
if (-not $Debug) { $cargoArgs += "--release" }

cargo @cargoArgs

# Need cargo-binutils + llvm-tools for this:
#   rustup component add llvm-tools-preview
#   cargo install cargo-binutils
Write-Host "==> Converting kernel ELF to flat binary..."
if (-not (Get-Command cargo-objcopy -ErrorAction SilentlyContinue)) {
    # cargo objcopy is invoked as `cargo objcopy`, not cargo-objcopy, but this is a quick sanity check.
    Write-Warning "cargo-binutils/objcopy not detected on PATH. Make sure you've run:"
    Write-Warning "  rustup component add llvm-tools-preview"
    Write-Warning "  cargo install cargo-binutils"
}

# This produces a raw 64-bit binary that stage2 jumps into at KERNEL_LOAD_PHYS (0x0002_0000)
if ($Debug) {
    cargo objcopy --bin rust-kernel --target $targetTriple -- -O binary $kernelBin
} else {
    cargo objcopy --bin rust-kernel --target $targetTriple --release -- -O binary $kernelBin
}

Pop-Location

if (-not (Test-Path $kernelBin)) {
    throw "kernel.bin was not produced at $kernelBin"
}

Write-Host "==> Kernel binary: $kernelBin"
Write-Host ""

# ------------------------------------------------------------------------------------
# 2. Assemble stage 1 (MBR)
# ------------------------------------------------------------------------------------
if (-not (Test-Path $mbrAsm)) {
    throw "Cannot find mbr_stage1.asm at $mbrAsm"
}

Write-Host "==> Assembling MBR (stage 1)..."
nasm -f bin $mbrAsm -o $mbrBin

if (-not (Test-Path $mbrBin)) {
    throw "mbr_stage1.bin not created at $mbrBin"
}

$mbBytes = [System.IO.File]::ReadAllBytes($mbrBin)
if ($mbBytes.Length -ne 512) {
    throw "MBR must be exactly 512 bytes, but got $($mbBytes.Length) bytes. Check times/boot signature in mbr_stage1.asm."
}

# ------------------------------------------------------------------------------------
# 3. Assemble stage 2
# ------------------------------------------------------------------------------------
if (-not (Test-Path $stage2Asm)) {
    throw "Cannot find stage2.asm at $stage2Asm"
}

Write-Host "==> Assembling stage 2..."

# This assumes stage2.asm is self-contained for -f bin.
# If you split 64-bit setup into separate files with `extern` + `global`,
# you'll instead want:
#   nasm -f elf32 stage2.asm -o stage2.o
#   ld -m elf_i386 -T stage2.ld -o stage2.elf stage2.o <other .o>
#   objcopy -O binary stage2.elf stage2.bin
nasm -f bin $stage2Asm -o $stage2Bin

if (-not (Test-Path $stage2Bin)) {
    throw "stage2.bin not created at $stage2Bin"
}

$stage2Data = [System.IO.File]::ReadAllBytes($stage2Bin)
$sectorSize = 512
$stage2SectorsActual = [int][Math]::Ceiling($stage2Data.Length / $sectorSize)

if ($stage2SectorsActual -gt $STAGE2_SECTORS) {
    throw "stage2.bin is $stage2SectorsActual sectors but STAGE2_SECTORS=$STAGE2_SECTORS. Increase STAGE2_SECTORS in asm + script, or shrink stage2."
}

# Pad stage2 to full sectors
if ($stage2Data.Length % $sectorSize -ne 0) {
    $pad = New-Object byte[] ($sectorSize - ($stage2Data.Length % $sectorSize))
    $stage2Data = $stage2Data + $pad
}

Write-Host "    Stage2 size: $($stage2Data.Length) bytes ($stage2SectorsActual sectors)"
Write-Host ""

Write-Host "==> Verifying sizes..."
(Get-Item $kernelBin).Length
Write-Host "Kernel size: $([Math]::Ceiling((Get-Item $kernelBin).Length / $sectorSize)) sectors"
Write-Host "Stage2 bin size: "
(Get-Item $stage2Bin).Length
# and see the "Stage2 size: ... bytes (...) sectors" output from the script


# ------------------------------------------------------------------------------------
# 4. Build disk image: [MBR][Stage2][Kernel]
# ------------------------------------------------------------------------------------
Write-Host "==> Building disk image..."

# Create zero-filled disk
$diskBytes = [int64]$diskSizeMiB * 1024 * 1024
$fs = [System.IO.File]::Open($diskImg,
    [System.IO.FileMode]::Create,
    [System.IO.FileAccess]::ReadWrite,
    [System.IO.FileShare]::None)
$fs.SetLength($diskBytes)

# LBA 0: MBR
$fs.Position = 0
$fs.Write($mbBytes, 0, $mbBytes.Length)

# LBA 1..N: stage2
$fs.Position = 1 * $sectorSize
$fs.Write($stage2Data, 0, $stage2Data.Length)

# Kernel: immediately after the reserved stage2 region
# We place kernel starting at LBA = 1 + STAGE2_SECTORS, regardless of actual stage2 size,
# to match KERNEL_LBA_START = 1 + STAGE2_SECTORS in stage2.asm.
$kernelData = [System.IO.File]::ReadAllBytes($kernelBin)
$kernelStartLba = 1 + $STAGE2_SECTORS
$fs.Position = $kernelStartLba * $sectorSize
$fs.Write($kernelData, 0, $kernelData.Length)

$fs.Flush()
$fs.Close()

Write-Host "    Disk size:   $diskSizeMiB MiB"
Write-Host "    Stage2 LBA:  1 .. $(1 + $stage2SectorsActual - 1)"
Write-Host "    Kernel LBA:  $kernelStartLba .. (depends on kernel size)"
Write-Host ""
Write-Host "==> Disk image ready: $diskImg"
Write-Host ""

# ------------------------------------------------------------------------------------
# 5. Launch QEMU
# ------------------------------------------------------------------------------------
Write-Host "==> Launching QEMU..."
$qemuArgs = @(
    "-m", "512M",
    "-drive", "format=raw,file=$diskImg",
    "-serial", "stdio",
    "-boot", "c"
)

# If you have multiple QEMU installs, adjust the executable here
qemu-system-x86_64.exe @qemuArgs
