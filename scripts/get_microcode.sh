#!/bin/bash
set -euo pipefail

cleanup() {
    [ -d "${tmpdir:-}" ] && rm -rf "$tmpdir"
}
trap cleanup EXIT

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
tmpdir=$(mktemp -d "/dev/shm/ucode_update.XXXXXXXXXXXX")

echo "Working in $tmpdir"
mkdir -p "$tmpdir/ucode/GenuineIntel" "$tmpdir/ucode/AuthenticAMD"

fetch_ucode() {
    local url=$1
    local sparse_path=$2
    local target_dir=$3
    local work_dir=$(mktemp -d "$tmpdir/fetch_XXXXXX")
    
    cd "$work_dir"
    git init -q
    git remote add origin "$url"
    git config core.sparseCheckout true
    echo "$sparse_path" >> .git/info/sparse-checkout
    git pull --depth 1 origin main || git pull --depth 1 origin master
    
    find "$work_dir/$sparse_path" -type f ! -name "README*" -exec mv {} "$target_dir" \;
}

echo "--- Fetching Intel ---"
fetch_ucode "https://github.com/intel/Intel-Linux-Processor-Microcode-Data-Files.git" "intel-ucode/" "$tmpdir/ucode/GenuineIntel/"

echo "--- Fetching AMD ---"
fetch_ucode "https://kernel.googlesource.com/pub/scm/linux/kernel/git/firmware/linux-firmware.git" "amd-ucode/" "$tmpdir/ucode/AuthenticAMD/"

echo "--- Compressing ---"
export SRC_DIR="$tmpdir/ucode"
bash "$SCRIPT_DIR/internal_compress_ucode.sh"

echo "Done!"
