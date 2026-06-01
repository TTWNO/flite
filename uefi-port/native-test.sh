#!/usr/bin/env bash
# CP-A: sanity-check flite C + compiled-in cmu_us_slt voice on the host.
set -euo pipefail
cd "$(dirname "$0")/.."
L=build/x86_64-linux-gnu/lib
gcc -Iinclude uefi-port/native-test.c \
  $L/libflite_cmu_us_slt.a $L/libflite_usenglish.a $L/libflite_cmulex.a $L/libflite.a \
  -lm -o /tmp/flite-native-test
/tmp/flite-native-test
