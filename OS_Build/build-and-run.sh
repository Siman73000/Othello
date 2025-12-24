#!/usr/bin/env bash
set -euo pipefail

DEBUG=0
NO_QEMU=0

usage() {
  cat <<'EOF'
Usage: ./build_and_run.sh [--debug] [--no-qemu]

Options:
  --debug     Build debug (no --release)
  --no-qemu   Build artifacts but don't launch QEMU
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --debug|-d) DEBUG=1; shift ;;
    --no-qemu)  NO_QEMU=1; shift ;;
    -h|--help)  usage; exit 0 ;;
    *) echo "Unknown arg: $1"; usage; exit 2 ;;
  esac
done

log() { printf '%s\n' "$*"; }
die() { printf 'ERROR: %s\n' "$*" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }

# Cross-platform file size (GNU stat vs BSD stat)
file_size() {
  local p="$1"
  if stat -c%s "$p" >/dev/null 2>&1; then
    stat -c%s "$p"
  elif stat -f%z "$p" >/dev/null 2>&1; then
    stat -f%z "$p"
  else
    python3 - <<PY
import os,sys
print(os.path.getsize(sys.argv[1]))
PY
  fi
}

find_first_existing() {
  for p in "$@"; do
    [[ -n "${p:-}" && -f "$p" ]] && { printf '%s' "$p"; return 0; }
  done
  return 1
}

find_first_efi() {
  local dir="$1"
  [[ -d "$dir" ]] || return 1
  # POSIX-ish: find first *.efi
  local out
  out="$(find "$dir" -maxdepth 1 -type f \( -iname "*.efi" -o -iname "*.EFI" \) 2>/dev/null | head -n 1 || true)"
  [[ -n "$out" && -f "$out" ]] && { printf '%s' "$out"; return 0; }
  return 1
}

OS_UNAME="$(uname -s || true)"

install_linux_pkgs() {
  if have apt-get; then
    sudo apt-get update
    sudo apt-get install -y nasm qemu-system-x86 ovmf
    return 0
  fi
  if have dnf; then
    sudo dnf install -y nasm qemu-system-x86 edk2-ovmf
    return 0
  fi
  if have yum; then
    sudo yum install -y nasm qemu-system-x86 edk2-ovmf
    return 0
  fi
  if have pacman; then
    sudo pacman -Sy --noconfirm nasm qemu-full edk2-ovmf
    return 0
  fi
  if have apk; then
    sudo apk add --no-cache nasm qemu-system-x86_64 ovmf
    return 0
  fi
  return 1
}

install_macos_pkgs() {
  if ! have brew; then
    die "Homebrew not found. Install it, then run: brew install qemu nasm edk2-ovmf"
  fi
  brew install qemu nasm edk2-ovmf || true
}

ensure_prereqs() {
  log "================================================================================"
  log " Ensuring prerequisites (Linux/macOS): Rust, NASM, QEMU, OVMF/EDK2"
  log "================================================================================"

  if ! have rustup || ! have cargo; then
    log "==> rustup/cargo not found. Installing rustup..."
    if have curl; then
      curl -sSf https://sh.rustup.rs | sh -s -- -y
    elif have wget; then
      wget -qO- https://sh.rustup.rs | sh -s -- -y
    else
      die "Need curl or wget to install rustup. Install Rust from https://www.rust-lang.org/tools/install"
    fi
  fi

  export PATH="$HOME/.cargo/bin:$PATH"
  [[ -f "$HOME/.cargo/env" ]] && source "$HOME/.cargo/env" || true

  rustup default stable
  rustup target add x86_64-unknown-none || true
  rustup target add x86_64-unknown-uefi || true
  rustup component add llvm-tools-preview || true

  if ! have cargo-objcopy; then
    cargo install cargo-binutils
  fi
  have cargo-objcopy || die "cargo-objcopy not found after installing cargo-binutils."

  if ! have nasm; then
    log "==> NASM not found; attempting install..."
    if [[ "$OS_UNAME" == "Darwin" ]]; then
      install_macos_pkgs
    else
      install_linux_pkgs || die "Could not auto-install NASM. Install it and re-run."
    fi
  fi
  have nasm || die "NASM is still missing."

  if ! have qemu-system-x86_64; then
    log "==> QEMU not found; attempting install..."
    if [[ "$OS_UNAME" == "Darwin" ]]; then
      install_macos_pkgs
    else
      install_linux_pkgs || die "Could not auto-install QEMU/OVMF. Install qemu + ovmf/edk2-ovmf and re-run."
    fi
  fi
  have qemu-system-x86_64 || die "QEMU (qemu-system-x86_64) is missing."
}

ensure_prereqs

# ------------------------------------------------------------------------------
# Layout (run this from OS_Build folder)
# ------------------------------------------------------------------------------
OS_BUILD_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

KERNEL_ROOT="$OS_BUILD_ROOT/Rust-Kernel"
UEFI_LOADER_ROOT="$OS_BUILD_ROOT/UEFI-Loader"

BUILD_DIR="$OS_BUILD_ROOT/build"
BOOT_DIR="$BUILD_DIR/boot"
mkdir -p "$BOOT_DIR"

TARGET_TRIPLE="x86_64-unknown-none"
PROFILE="release"
[[ "$DEBUG" -eq 1 ]] && PROFILE="debug"

KERNEL_ELF="$KERNEL_ROOT/target/$TARGET_TRIPLE/$PROFILE/rust-kernel"
KERNEL_BIN="$BUILD_DIR/kernel.bin"

MBR_ASM="$OS_BUILD_ROOT/assembly/mbr_stage1.asm"
STAGE2_ASM="$OS_BUILD_ROOT/assembly/stage2.asm"

MBR_BIN="$BOOT_DIR/mbr_stage1.bin"
STAGE2_BIN="$BOOT_DIR/stage2.bin"
STAGE2_PADDED="$BOOT_DIR/stage2_padded.bin"

DISK_IMG="$BUILD_DIR/disk.img"
DISK_SIZE_MIB=64

# Keep in sync with STAGE2_SECTORS in your NASM sources
STAGE2_SECTORS=8
SECTOR_SIZE=512

log ">> OS_Build root: $OS_BUILD_ROOT"
log ">> Build profile: $PROFILE"
log ""

# ------------------------------------------------------------------------------
# 1) Build Rust kernel
# ------------------------------------------------------------------------------
log "==> Building Rust kernel ($PROFILE)..."
[[ -d "$KERNEL_ROOT" ]] || die "Rust-Kernel directory not found: $KERNEL_ROOT"

pushd "$KERNEL_ROOT" >/dev/null
if [[ "$DEBUG" -eq 1 ]]; then
  cargo build --target "$TARGET_TRIPLE"
  cargo objcopy --bin rust-kernel --target "$TARGET_TRIPLE" -- -O binary "$KERNEL_BIN"
else
  cargo build --target "$TARGET_TRIPLE" --release
  cargo objcopy --bin rust-kernel --target "$TARGET_TRIPLE" --release -- -O binary "$KERNEL_BIN"
fi
popd >/dev/null

[[ -f "$KERNEL_BIN" ]] || die "kernel.bin was not produced at $KERNEL_BIN"

KERNEL_LEN="$(file_size "$KERNEL_BIN")"
KERNEL_SECTORS=$(( (KERNEL_LEN + SECTOR_SIZE - 1) / SECTOR_SIZE ))
KERNEL_INC="$OS_BUILD_ROOT/assembly/kernel_sectors.inc"
printf '%%define KERNEL_SECTORS %s' "$KERNEL_SECTORS" > "$KERNEL_INC"

log "    kernel.bin: $KERNEL_LEN bytes ($KERNEL_SECTORS sectors)"
log "    wrote:      $KERNEL_INC"
log ""

# ------------------------------------------------------------------------------
# 2) Assemble MBR + stage2
# ------------------------------------------------------------------------------
[[ -f "$MBR_ASM" ]] || die "Missing: $MBR_ASM"
[[ -f "$STAGE2_ASM" ]] || die "Missing: $STAGE2_ASM"

log "==> Assembling MBR..."
nasm -f bin "$MBR_ASM" -o "$MBR_BIN"
MBR_SIZE="$(file_size "$MBR_BIN")"
[[ "$MBR_SIZE" -eq 512 ]] || die "MBR must be exactly 512 bytes; got $MBR_SIZE."

log "==> Assembling stage2..."
nasm -I "$OS_BUILD_ROOT/assembly/" -f bin "$STAGE2_ASM" -o "$STAGE2_BIN"

STAGE2_LEN="$(file_size "$STAGE2_BIN")"
STAGE2_SECTORS_ACTUAL=$(( (STAGE2_LEN + SECTOR_SIZE - 1) / SECTOR_SIZE ))
if [[ "$STAGE2_SECTORS_ACTUAL" -gt "$STAGE2_SECTORS" ]]; then
  die "stage2 is $STAGE2_SECTORS_ACTUAL sectors but STAGE2_SECTORS=$STAGE2_SECTORS. Increase STAGE2_SECTORS or shrink stage2."
fi

cp -f "$STAGE2_BIN" "$STAGE2_PADDED"
REM=$(( STAGE2_LEN % SECTOR_SIZE ))
if [[ "$REM" -ne 0 ]]; then
  PAD=$(( SECTOR_SIZE - REM ))
  dd if=/dev/zero bs=1 count="$PAD" 2>/dev/null >> "$STAGE2_PADDED"
fi

log "    stage2.bin: $STAGE2_LEN bytes ($STAGE2_SECTORS_ACTUAL sectors)"
log ""

# ------------------------------------------------------------------------------
# 3) Build disk image
# ------------------------------------------------------------------------------
log "==> Building disk image..."
mkdir -p "$BUILD_DIR"

if have truncate; then
  truncate -s "${DISK_SIZE_MIB}M" "$DISK_IMG"
else
  dd if=/dev/zero of="$DISK_IMG" bs=1m count="$DISK_SIZE_MIB" 2>/dev/null
fi

dd if="$MBR_BIN" of="$DISK_IMG" bs=512 seek=0 conv=notrunc 2>/dev/null
dd if="$STAGE2_PADDED" of="$DISK_IMG" bs=512 seek=1 conv=notrunc 2>/dev/null

KERNEL_START_LBA=$(( 1 + STAGE2_SECTORS ))
dd if="$KERNEL_BIN" of="$DISK_IMG" bs=512 seek="$KERNEL_START_LBA" conv=notrunc 2>/dev/null

log "    disk.img: ${DISK_SIZE_MIB} MiB"
log "    stage2 LBA: 1 (reserved ${STAGE2_SECTORS} sectors)"
log "    kernel LBA: $KERNEL_START_LBA"
log "    image:      $DISK_IMG"
log ""

# ------------------------------------------------------------------------------
# 4) Build + stage UEFI loader
# ------------------------------------------------------------------------------
log "==> Building UEFI loader..."
[[ -d "$UEFI_LOADER_ROOT" ]] || die "UEFI-Loader directory not found: $UEFI_LOADER_ROOT"

pushd "$UEFI_LOADER_ROOT" >/dev/null
if [[ "$DEBUG" -eq 1 ]]; then
  cargo build --target x86_64-unknown-uefi
else
  cargo build --target x86_64-unknown-uefi --release
fi
popd >/dev/null

EFI_BOOT_DIR="$OS_BUILD_ROOT/efi_root/EFI/BOOT"
mkdir -p "$EFI_BOOT_DIR"

# Prefer specific known names, else fall back to "first .efi in target dir"
LOADER_OUT="$(find_first_existing \
  "$UEFI_LOADER_ROOT/target/x86_64-unknown-uefi/release/othello-uefi-loader.efi" \
  "$UEFI_LOADER_ROOT/target/x86_64-unknown-uefi/release/BOOTX64.EFI" \
  "$UEFI_LOADER_ROOT/target/x86_64-unknown-uefi/debug/othello-uefi-loader.efi" \
  "$UEFI_LOADER_ROOT/target/x86_64-unknown-uefi/debug/BOOTX64.EFI" \
  || true)"

if [[ -z "$LOADER_OUT" ]]; then
  LOADER_OUT="$(find_first_efi "$UEFI_LOADER_ROOT/target/x86_64-unknown-uefi/$PROFILE" || true)"
fi
[[ -n "$LOADER_OUT" ]] || die "Could not find a .efi in $UEFI_LOADER_ROOT/target/x86_64-unknown-uefi/(release|debug)"

cp -f "$LOADER_OUT" "$EFI_BOOT_DIR/BOOTX64.EFI"

[[ -f "$KERNEL_ELF" ]] || die "Kernel ELF not found: $KERNEL_ELF"
cp -f "$KERNEL_ELF" "$OS_BUILD_ROOT/efi_root/kernel.elf"

log "    BOOTX64.EFI => $EFI_BOOT_DIR/BOOTX64.EFI"
log "    kernel.elf  => $OS_BUILD_ROOT/efi_root/kernel.elf"
log ""

if [[ "$NO_QEMU" -eq 1 ]]; then
  log "==> --no-qemu set; done."
  exit 0
fi

# ------------------------------------------------------------------------------
# 5) Launch QEMU (OVMF)
# ------------------------------------------------------------------------------
log "==> Launching QEMU..."

QEMU_EXE="${QEMU_EXE:-qemu-system-x86_64}"
have "$QEMU_EXE" || die "QEMU executable not found: $QEMU_EXE"

# You can override these:
#   OVMF_CODE=/path/to/code.fd OVMF_VARS=/path/to/vars.fd ./build_and_run.sh
OVMF_CODE="${OVMF_CODE:-}"
OVMF_VARS="${OVMF_VARS:-}"

if [[ -z "$OVMF_CODE" ]]; then
  OVMF_CODE="$(find_first_existing \
    "$OS_BUILD_ROOT/OVMF_CODE.fd" \
    "$OS_BUILD_ROOT/OVMF_CODE_4M.fd" \
    "/usr/share/OVMF/OVMF_CODE.fd" \
    "/usr/share/OVMF/OVMF_CODE_4M.fd" \
    "/usr/share/qemu/OVMF_CODE.fd" \
    "/usr/share/edk2/ovmf/OVMF_CODE.fd" \
    "/opt/homebrew/share/qemu/edk2-x86_64-code.fd" \
    "/usr/local/share/qemu/edk2-x86_64-code.fd" \
    "/opt/homebrew/share/edk2-ovmf/OVMF_CODE.fd" \
    "/usr/local/share/edk2-ovmf/OVMF_CODE.fd" \
    || true)"
fi

if [[ -z "$OVMF_VARS" ]]; then
  OVMF_VARS="$(find_first_existing \
    "$OS_BUILD_ROOT/OVMF_VARS.fd" \
    "$OS_BUILD_ROOT/OVMF_VARS_4M.fd" \
    "/usr/share/OVMF/OVMF_VARS.fd" \
    "/usr/share/OVMF/OVMF_VARS_4M.fd" \
    "/usr/share/qemu/OVMF_VARS.fd" \
    "/usr/share/edk2/ovmf/OVMF_VARS.fd" \
    "/opt/homebrew/share/qemu/edk2-x86_64-vars.fd" \
    "/usr/local/share/qemu/edk2-x86_64-vars.fd" \
    "/opt/homebrew/share/edk2-ovmf/OVMF_VARS.fd" \
    "/usr/local/share/edk2-ovmf/OVMF_VARS.fd" \
    || true)"
fi

[[ -n "$OVMF_CODE" && -f "$OVMF_CODE" ]] || die "OVMF_CODE not found. Install OVMF/edk2-ovmf or set OVMF_CODE=/path/to/code.fd"
[[ -n "$OVMF_VARS" && -f "$OVMF_VARS" ]] || die "OVMF_VARS not found. Install OVMF/edk2-ovmf or set OVMF_VARS=/path/to/vars.fd"

exec "$QEMU_EXE" \
  -machine q35 \
  -m 1024 \
  -drive "if=pflash,format=raw,readonly=on,file=$OVMF_CODE" \
  -drive "if=pflash,format=raw,file=$OVMF_VARS" \
  -drive "file=fat:rw:$OS_BUILD_ROOT/efi_root,format=raw" \
  -netdev user,id=net1 \
  -device rtl8139,netdev=net1 \
  -serial stdio \
  -no-reboot
