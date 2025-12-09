param(
    [switch]$Debug
)

$ErrorActionPreference = "Stop"

# Optionally prepend QEMU install folder to PATH (common default install location)
$possibleQemuDir = "C:\Program Files\qemu"
if (Test-Path $possibleQemuDir -PathType Container -ErrorAction SilentlyContinue) {
    if (-not $env:PATH.ToLower().Contains("\qemu")) {
        $env:PATH = "$possibleQemuDir;$env:PATH"
    }
}

# ------------------------------------------------------------------------------
# Global config
# ------------------------------------------------------------------------------

$targetTriple = "x86_64-unknown-none"
$script:QemuExePath = $null

function New-Directory([string]$Path) {
    if (-not (Test-Path $Path)) {
        New-Item -ItemType Directory -Path $Path | Out-Null
    }
}

function Ensure-Prereqs {
    Write-Host "================================================================================"
    Write-Host " Ensuring prerequisites: Rust + tools, NASM, QEMU"
    Write-Host "================================================================================"

    # -----------------------------
    # Rust / cargo / rustup
    # -----------------------------
    Write-Host "==> Checking Rust toolchain (rustup + cargo)..."

    $rustupCmd = Get-Command rustup -ErrorAction SilentlyContinue
    $cargoCmd  = Get-Command cargo  -ErrorAction SilentlyContinue

    if (-not $rustupCmd -or -not $cargoCmd) {
        Write-Host "    rustup and/or cargo not found on PATH."

        $wingetCmd = Get-Command winget -ErrorAction SilentlyContinue
        if ($wingetCmd) {
            Write-Host "    Attempting to install Rustup via winget..."
            & winget install --id Rustlang.Rustup -e --source winget
            Write-Host "    If the Rust installer ran, restart this PowerShell session after it finishes."
        }
        else {
            Write-Host "    winget is not available."
            Write-Host "    Please install Rust manually from:"
            Write-Host "      https://www.rust-lang.org/tools/install"
            throw "Rust toolchain missing."
        }

        # Refresh commands after install
        $rustupCmd = Get-Command rustup -ErrorAction SilentlyContinue
        $cargoCmd  = Get-Command cargo  -ErrorAction SilentlyContinue
        if (-not $rustupCmd -or -not $cargoCmd) {
            throw "rustup/cargo still not found after attempted install. Restart the shell or install Rust manually."
        }
    }

    Write-Host "    Rust + cargo OK."

    # Ensure this session sees ~/.cargo/bin
    $cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
    if ((Test-Path $cargoBin) -and (-not $env:PATH.ToLower().Contains("\.cargo\bin"))) {
        $env:PATH += ";$cargoBin"
        Write-Host "    Added $cargoBin to PATH for this session."
    }

    # -----------------------------
    # Configure Rust toolchain
    # -----------------------------
    Write-Host "==> Configuring Rust target + tools..."

    Write-Host "    rustup default stable"
    & rustup default stable
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to set 'rustup default stable'. Run it manually, then re-run this script."
    }

    Write-Host "    rustup target add $targetTriple"
    & rustup target add $targetTriple
    if ($LASTEXITCODE -ne 0) {
        Write-Host "    rustup target add returned exit code $LASTEXITCODE (target may already be installed)."
    }

    Write-Host "    rustup component add llvm-tools-preview"
    & rustup component add llvm-tools-preview
    if ($LASTEXITCODE -ne 0) {
        Write-Host "    rustup component add returned exit code $LASTEXITCODE (component may already be installed)."
    }

    # -----------------------------
    # cargo-binutils (cargo-objcopy)
    # -----------------------------
    Write-Host "==> Checking cargo-binutils / cargo-objcopy..."
    $objcopyCmd = Get-Command cargo-objcopy -ErrorAction SilentlyContinue
    if (-not $objcopyCmd) {
        Write-Host "    cargo-objcopy not found; installing cargo-binutils..."
        & cargo install cargo-binutils
        if ($LASTEXITCODE -ne 0) {
            throw "Failed to install cargo-binutils (cargo install cargo-binutils)."
        }

        $objcopyCmd = Get-Command cargo-objcopy -ErrorAction SilentlyContinue
        if (-not $objcopyCmd) {
            throw "cargo-objcopy still not found after installing cargo-binutils."
        }
    }
    Write-Host "    cargo-objcopy OK."

    # -----------------------------
    # NASM
    # -----------------------------
    Write-Host "==> Checking NASM..."
    $nasmCmd = Get-Command nasm -ErrorAction SilentlyContinue
    if (-not $nasmCmd) {
        Write-Host "    NASM not found. Attempting install via winget (NASM.NASM)..."
        $wingetCmd = Get-Command winget -ErrorAction SilentlyContinue
        if ($wingetCmd) {
            & winget install --id NASM.NASM -e --source winget
        }
        else {
            throw "NASM not found and winget is unavailable. Install NASM manually and ensure 'nasm.exe' is on PATH."
        }

        $nasmCmd = Get-Command nasm -ErrorAction SilentlyContinue
        if (-not $nasmCmd) {
            throw "NASM still not found after attempted install. Install manually and re-run."
        }
    }
    Write-Host "    NASM found at $($nasmCmd.Source)"

    # -----------------------------
    # QEMU (qemu-system-x86_64)
    # -----------------------------
    Write-Host "==> Checking QEMU (qemu-system-x86_64)..."
    $qemuCmd = Get-Command qemu-system-x86_64 -ErrorAction SilentlyContinue
    if (-not $qemuCmd) {
        Write-Host "    qemu-system-x86_64 not found."
        Write-Host "    Please install QEMU manually from:"
        Write-Host "      https://www.qemu.org/download/"
        Write-Host "    Make sure 'qemu-system-x86_64.exe' ends up on your PATH."
        throw "QEMU (qemu-system-x86_64) is not installed."
    }

    $script:QemuExePath = $qemuCmd.Source
    Write-Host "    qemu-system-x86_64 found at $($qemuCmd.Source)"
    Write-Host ""
}

# Run prerequisite checks / setup
Ensure-Prereqs

# ------------------------------------------------------------------------------
# Paths / config (assumes you run this from OS_Build)
# ------------------------------------------------------------------------------

$osBuildRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$kernelRoot  = Join-Path $osBuildRoot "Rust-Kernel"
$buildDir    = Join-Path $osBuildRoot "build"
$bootDir     = Join-Path $buildDir   "boot"

New-Directory $buildDir
New-Directory $bootDir

$profile      = if ($Debug) { "debug" } else { "release" }

$kernelElf = Join-Path $kernelRoot "target\$targetTriple\$profile\rust-kernel"
$kernelBin = Join-Path $buildDir   "kernel.bin"

$mbrAsm    = Join-Path $osBuildRoot "assembly\mbr_stage1.asm"
$stage2Asm = Join-Path $osBuildRoot "assembly\stage2.asm"

$mbrBin    = Join-Path $bootDir "mbr_stage1.bin"
$stage2Bin = Join-Path $bootDir "stage2.bin"

$diskImg   = Join-Path $buildDir "disk.img"
$diskSizeMiB = 64

# Keep in sync with STAGE2_SECTORS in BOTH mbr_stage1.asm and stage2.asm
$STAGE2_SECTORS = 8

Write-Host ">> OS_Build root: $osBuildRoot"
Write-Host ">> Kernel root:   $kernelRoot"
Write-Host ">> Build dir:     $buildDir"
Write-Host ">> Boot dir:      $bootDir"
Write-Host ">> Disk image:    $diskImg"
Write-Host ""

# ------------------------------------------------------------------------------
# 1. Build Rust kernel
# ------------------------------------------------------------------------------

Write-Host "==> Building Rust kernel ($profile)..."

Push-Location $kernelRoot

$cargoArgs = @("build", "--target", $targetTriple)
if (-not $Debug) { $cargoArgs += "--release" }

cargo @cargoArgs

if ($LASTEXITCODE -ne 0) {
    Pop-Location
    throw "cargo build failed; aborting build-and-run."
}

Write-Host "==> Converting kernel ELF to flat binary..."
if ($Debug) {
    cargo objcopy --bin rust-kernel --target $targetTriple -- -O binary $kernelBin
} else {
    cargo objcopy --bin rust-kernel --target $targetTriple --release -- -O binary $kernelBin
}
if ($LASTEXITCODE -ne 0) {
    Pop-Location
    throw "cargo objcopy failed; aborting build-and-run."
}

Pop-Location

if (-not (Test-Path $kernelBin)) {
    throw "kernel.bin was not produced at $kernelBin"
}

Write-Host "==> Kernel binary: $kernelBin"
Write-Host ""

# ------------------------------------------------------------------------------
# 2. Assemble stage 1 (MBR)
# ------------------------------------------------------------------------------

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
    throw "MBR must be exactly 512 bytes, but got $($mbBytes.Length) bytes. Check size/boot signature in mbr_stage1.asm."
}

# ------------------------------------------------------------------------------
# 3. Assemble stage 2
# ------------------------------------------------------------------------------

if (-not (Test-Path $stage2Asm)) {
    throw "Cannot find stage2.asm at $stage2Asm"
}

Write-Host "==> Assembling stage 2..."
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

# pad to full sector
if ($stage2Data.Length % $sectorSize -ne 0) {
    $pad = New-Object byte[] ($sectorSize - ($stage2Data.Length % $sectorSize))
    $stage2Data = $stage2Data + $pad
}

Write-Host "    Stage2 size: $($stage2Data.Length) bytes ($stage2SectorsActual sectors)"
Write-Host ""

Write-Host "==> Verifying sizes..."
$kernelLen = (Get-Item $kernelBin).Length
Write-Host "    Kernel size: $kernelLen bytes ($([Math]::Ceiling($kernelLen / $sectorSize)) sectors)"
Write-Host ""

# ------------------------------------------------------------------------------
# 4. Build disk image: [MBR][Stage2][Kernel]
# ------------------------------------------------------------------------------

Write-Host "==> Building disk image..."

$diskBytes = [int64]$diskSizeMiB * 1024 * 1024
$fs = [System.IO.File]::Open(
    $diskImg,
    [System.IO.FileMode]::Create,
    [System.IO.FileAccess]::ReadWrite,
    [System.IO.FileShare]::None
)
$fs.SetLength($diskBytes)

# LBA 0: MBR
$fs.Position = 0
$fs.Write($mbBytes, 0, $mbBytes.Length)

# LBA 1..N: stage2 (we reserve STAGE2_SECTORS even though actual may be smaller)
$fs.Position = 1 * $sectorSize
$fs.Write($stage2Data, 0, $stage2Data.Length)
# The remaining reserved sectors (if any) will remain zero-filled.

# Kernel at LBA = 1 + STAGE2_SECTORS (must match KERNEL_LBA_START in stage2.asm)
$kernelData = [System.IO.File]::ReadAllBytes($kernelBin)
$kernelStartLba = 1 + $STAGE2_SECTORS
$fs.Position = $kernelStartLba * $sectorSize
$fs.Write($kernelData, 0, $kernelData.Length)

$fs.Flush()
$fs.Close()

Write-Host "    Disk size:   $diskSizeMiB MiB"
Write-Host "    Stage2 LBA:  1 .. $(1 + $stage2SectorsActual - 1)   (reserved: 1 .. $(1 + $STAGE2_SECTORS - 1))"
Write-Host "    Kernel LBA:  $kernelStartLba .. (depends on kernel size)"
Write-Host ""
Write-Host "==> Disk image ready: $diskImg"
Write-Host ""

# ------------------------------------------------------------------------------
# 5. Host display info (for logging)
# ------------------------------------------------------------------------------

try {
    Add-Type -AssemblyName System.Windows.Forms -ErrorAction Stop
    $screen = [System.Windows.Forms.Screen]::PrimaryScreen
    $hostWidth  = $screen.Bounds.Width
    $hostHeight = $screen.Bounds.Height
    Write-Host "Host primary display: ${hostWidth}x${hostHeight}"
} catch {
    Write-Host "Could not load System.Windows.Forms to detect host resolution (running headless / non-Windows?)."
}

Write-Host ""

# ------------------------------------------------------------------------------
# 6. Launch QEMU
# ------------------------------------------------------------------------------

Write-Host "==> Launching QEMU with network device (RTL8139) and windowed display..."

if (-not $script:QemuExePath) {
    $script:QemuExePath = "qemu-system-x86_64.exe"
}

$qemuArgs = @(
    "-m", "512M",
    "-machine", "pc,accel=tcg",
    "-drive", "format=raw,file=$diskImg,if=ide",
    "-boot", "c",
    "-serial", "stdio",
    "-device", "rtl8139,netdev=n0",
    "-netdev", "user,id=n0",
    "-vga", "std",
    # SDL display with GL; you can add '-full-screen' as another arg if desired
    "-display", "sdl"
)

& $script:QemuExePath @qemuArgs
