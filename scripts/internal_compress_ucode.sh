#!/bin/bash
set -euo pipefail

: "${SRC_DIR:?SRC_DIR environment variable is not set}"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DEST_DIR="$WORKSPACE_ROOT/contents/ucode"

mkdir -p "$DEST_DIR"

find "$SRC_DIR" -type f -print0 | while IRC= read -r -d '' f; do
    rel_path="${f#$SRC_DIR/}"
    out="$DEST_DIR/$rel_path.z"

    mkdir -p "$(dirname "$out")"

    python3 - "$f" "$out" <<'EOF'
import zlib
import sys
import struct

src_path = sys.argv[1]
dst_path = sys.argv[2]

with open(src_path, 'rb') as f_in:
    data = f_in.read()
    orig_size = len(data)
    
    # Raw Deflate (wbits=-15)
    compressor = zlib.compressobj(9, zlib.DEFLATED, -15)
    compressed_data = compressor.compress(data) + compressor.flush()

with open(dst_path, 'wb') as f_out:
    # 先頭に元のサイズをリトルエンディアン 4byte (Unsigned Long) で書き込む
    f_out.write(struct.pack('<I', orig_size))
    f_out.write(compressed_data)
EOF

    echo "Compressed: $rel_path -> $rel_path.z"
done
