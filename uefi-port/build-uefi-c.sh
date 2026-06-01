#!/usr/bin/env bash
# build-uefi-c.sh — Cross-compile all flite C files to COFF x86-64 objects for UEFI
# Objects go into uefi-port/obj/<mangled>.o
# Usage: bash uefi-port/build-uefi-c.sh  (run from repo root)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"

OBJ_DIR="$SCRIPT_DIR/obj"
COMPILE_COMMANDS="$SCRIPT_DIR/compile_commands.json"

mkdir -p "$OBJ_DIR"

CLANG_FLAGS=(
    "--target=x86_64-unknown-uefi"
    "-fno-builtin"
    "-ffreestanding"
    "-mno-red-zone"
    "-fshort-wchar"
    "-DDIE_ON_ERROR"
    "-DCST_AUDIO_NONE"
    "-DCST_NO_SOCKETS"
    "-I$SCRIPT_DIR/cinclude"
    "-I$REPO_ROOT/include"
    "-I$REPO_ROOT/lang/usenglish"
    "-I$REPO_ROOT/lang/cmulex"
    "-Wno-implicit-function-declaration"
    "-Wno-int-conversion"
)

FAILED=()
SUCCESS=0

FILES=$(python3 -c "
import json, sys
with open('$COMPILE_COMMANDS') as f:
    entries = json.load(f)
for e in entries:
    print(e['file'])
")

while IFS= read -r src_file; do
    # Mangle path: strip leading / and replace / with _
    rel="${src_file#$REPO_ROOT/}"
    mangled="${rel//\//_}"
    obj="$OBJ_DIR/${mangled%.c}.o"

    if clang "${CLANG_FLAGS[@]}" -c "$src_file" -o "$obj" 2>/tmp/uefi_build_err.txt; then
        SUCCESS=$((SUCCESS + 1))
    else
        echo "FAILED: $src_file"
        cat /tmp/uefi_build_err.txt
        FAILED+=("$src_file")
    fi
done <<< "$FILES"

echo ""
echo "=== Build Summary ==="
echo "Succeeded: $SUCCESS"
echo "Failed:    ${#FAILED[@]}"

if [ ${#FAILED[@]} -gt 0 ]; then
    echo ""
    echo "Failed files:"
    for f in "${FAILED[@]}"; do
        echo "  $f"
    done
    exit 1
fi

echo "All files compiled successfully."

# Archive the objects into the static library the Rust crate links against.
# (Without this the crate links a stale archive and C-side changes are ignored.)
ARCHIVE="$SCRIPT_DIR/libflite_uefi.a"
rm -f "$ARCHIVE"
llvm-ar rcs "$ARCHIVE" "$OBJ_DIR"/*.o
echo "Archived $(llvm-ar t "$ARCHIVE" | wc -l) objects into $ARCHIVE"
