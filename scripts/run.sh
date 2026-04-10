#!/bin/bash

set -e

RUNING_DIR="$(pwd)"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$RUNING_DIR" || return

bash "$SCRIPT_DIR/internal_init_script.sh"

EFI_PATH=$1
shift

CODE=$(find /usr/share/edk2/ovmf /usr/share/OVMF /usr/share/qemu /usr/share /usr/local/share /usr/lib -maxdepth 5 -name "OVMF_CODE*.fd" 2>/dev/null | grep -iE "x64|ovmf" | grep -v sec | head -n 1)
VARS_SRC=$(find /usr/share/edk2/ovmf /usr/share/OVMF /usr/share/qemu /usr/share /usr/local/share /usr/lib -maxdepth 5 -name "OVMF_VARS*.fd" 2>/dev/null | grep -iE "x64|ovmf" | head -n 1)

MEM=5120

VARS_TMP=$(mktemp /tmp/ovmf_vars.XXXXXX.fd)
cp "$VARS_SRC" "$VARS_TMP"

PIPE_TMP=$(mktemp /tmp/pipe_tmp_XXXXXX -d)

trap "rm -rf $PIPE_TMP" EXIT

mkfifo "$PIPE_TMP/serial_pipe.in" "$PIPE_TMP/serial_pipe.out"
"$WORKSPACE_ROOT/bin/log_viewer" < "$PIPE_TMP/serial_pipe.out" &
VIEWER_PID=$!

mkdir -p "$WORKSPACE_ROOT/esp/EFI/BOOT"
cp "$EFI_PATH" "$WORKSPACE_ROOT/esp/EFI/BOOT/BOOTx64.EFI"
if [ -d "$WORKSPACE_ROOT/contents" ]; then
    cp -r "$WORKSPACE_ROOT/contents" "$WORKSPACE_ROOT/esp/EFI/BOOT"
fi

echo "$CODE, $VARS_TMP"

qemu-system-x86_64 \
  -enable-kvm \
  -cpu host,migratable=no,+invtsc \
  -m "$MEM" \
  -smp 12,sockets=1,cores=6,threads=2 \
  -machine q35 \
  -drive if=pflash,format=raw,readonly=on,file="$CODE" \
  -drive if=pflash,format=raw,file="$VARS_TMP" \
  \
  -serial pipe:"$PIPE_TMP/serial_pipe" \
  -device virtio-gpu-pci \
  -display gtk,zoom-to-fit=on \
  -rtc base=localtime,clock=host \
  -global kvm-pit.lost_tick_policy=delay \
  \
  -drive file=fat:rw:"$WORKSPACE_ROOT/esp",format=raw,if=none,id=virtio_disk \
  -device virtio-blk-pci,drive=virtio_disk \
  \
  -s \
  \
  -no-reboot \
  -no-shutdown &

QEMU_PID=$!
sleep 1

gdb-multiarch -ex "target remote :1234"

wait $QEMU_PID
wait $VIEWER_PID
rm -f "$VARS_TMP"