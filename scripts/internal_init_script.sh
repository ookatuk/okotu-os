#!/bin/bash

set -e

RUNING_DIR="$(pwd)"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$RUNING_DIR" || return


rm -f "$WORKSPACE_ROOT/serial_pipe.in" "$WORKSPACE_ROOT/serial_pipe.out"
rm -rf "$WORKSPACE_ROOT/esp"
