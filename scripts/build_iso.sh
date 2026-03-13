#!/bin/bash

set -e

RUNING_DIR="$(pwd)"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$RUNING_DIR" || return

bash "$SCRIPT_DIR/internal_init_script.sh"


ISO_ROOT="$WORKSPACE_ROOT/esp"
BOOT_DIR="$ISO_ROOT/EFI/BOOT"
rm -rf "$ISO_ROOT"
mkdir -p "$BOOT_DIR"

cp "$WORKSPACE_ROOT/target/x86_64-unknown-uefi/release/test_os_v2.efi" "$BOOT_DIR/BOOTx64.EFI"

if [ -d "$WORKSPACE_ROOT/contents" ]; then
  cp -r ./contents "$BOOT_DIR/"
fi

EFI_IMG="$WORKSPACE_ROOT/efiboot.img"
rm -f "$EFI_IMG"

BLOCK_COUNT=$(du -sk "$WORKSPACE_ROOT/contents" | awk '{print $1}')

BLOCK_COUNT=$((BLOCK_COUNT + 5120))

mkfs.msdos -C "$EFI_IMG" "$BLOCK_COUNT"

mmd -i "$EFI_IMG" ::/EFI
mmd -i "$EFI_IMG" ::/EFI/BOOT
mcopy -i "$EFI_IMG" "$BOOT_DIR/BOOTx64.EFI" ::/EFI/BOOT/
mcopy -s -i "$EFI_IMG" "$BOOT_DIR/contents" ::/EFI/BOOT/

ISO_PATH="$WORKSPACE_ROOT/test_os.iso"
xorriso -as mkisofs \
  -R -f \
  -e "$(basename "$EFI_IMG")" \
  -no-emul-boot \
  -isohybrid-gpt-basdat \
  -o "$ISO_PATH" \
  "$ISO_ROOT" -graft-points "$(basename "$EFI_IMG")=$EFI_IMG"

rm -f "$EFI_IMG"
