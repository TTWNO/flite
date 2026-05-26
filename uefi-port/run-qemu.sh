#!/usr/bin/env bash
# Boot the flite-uefi EFI app under QEMU + OVMF. Captures console output.
# A persistent ESP dir (uefi-port/esp) lets the host inspect files the app
# writes (CP-E). QEMU is killed by an external timeout (the app spins forever
# after printing so its output stays on the console).
set -euo pipefail
cd "$(dirname "$0")"

EFI=flite-uefi/target/x86_64-unknown-uefi/debug/flite-uefi.efi
ESP=esp
TIMEOUT="${1:-30}"

[ -f "$EFI" ] || { echo "missing $EFI — build first"; exit 1; }

rm -rf "$ESP"
mkdir -p "$ESP/EFI/BOOT"
cp "$EFI" "$ESP/EFI/BOOT/BOOTX64.EFI"

# OVMF needs a writable copy of the variable store.
cp /usr/share/edk2/x64/OVMF_VARS.4m.fd /tmp/flite_OVMF_VARS.fd

echo "=== booting QEMU (timeout ${TIMEOUT}s) ==="
timeout "${TIMEOUT}" qemu-system-x86_64 \
  -machine q35 -m 256 -nographic \
  -drive if=pflash,format=raw,unit=0,readonly=on,file=/usr/share/edk2/x64/OVMF_CODE.4m.fd \
  -drive if=pflash,format=raw,unit=1,file=/tmp/flite_OVMF_VARS.fd \
  -drive format=raw,file=fat:rw:"$ESP" \
  -net none 2>&1 || true
echo
echo "=== QEMU exited ==="
