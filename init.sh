#!/bin/sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

git submodule update --init --recursive "$SCRIPT_DIR"
bash "$SCRIPT_DIR/scripts/get_microcode.sh"
