#!/bin/sh

set -e

RUNING_DIR="$(pwd)"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$RUNING_DIR" || return

sh "$SCRIPT_DIR/internal_init_script.sh"

EFI_PATH=$1
shift

mkfifo "$WORKSPACE_ROOT/serial_pipe.in" "$WORKSPACE_ROOT/serial_pipe.out"

"$WORKSPACE_ROOT/bin/log_viewer" < "$WORKSPACE_ROOT/serial_pipe.out" &
VIEWER_PID=$!

mkdir -p "$WORKSPACE_ROOT/esp/EFI/BOOT"
cp "$EFI_PATH" "$WORKSPACE_ROOT/esp/EFI/BOOT/BOOTx64.EFI"
cp -r "$WORKSPACE_ROOT/contents" "$WORKSPACE_ROOT/esp/EFI/BOOT"

qemu-system-x86_64 \
  -bios /usr/share/ovmf/x64/OVMF.4m.fd \
  -drive file=fat:rw:"$WORKSPACE_ROOT/esp",format=raw \
  -serial pipe:"$WORKSPACE_ROOT/serial_pipe" \
  -device virtio-gpu-pci \
  -display gtk,gl=on \
  -machine q35 -m 5G -enable-kvm -cpu host \
  -no-reboot -no-shutdown -smp 2 \
  "$@"

wait $VIEWER_PID

rm -f "$WORKSPACE_ROOT/serial_pipe.in" "$WORKSPACE_ROOT/serial_pipe.out"