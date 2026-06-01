# Known issues

## RESOLVED: near-silent / wrong audio on UEFI — f64 ABI mismatch

**Symptom (fixed):** synthesis ran and wrote a valid `hello.wav`, but the audio
was near-silent (peak ~47 of ±32767) and the duration was wrong (17840 vs the
correct 19760 samples). Reproduced in QEMU — it was never hardware-specific.

**Root cause:** Rust's `x86_64-unknown-uefi` target passes/returns `f64` in
**integer registers** (it avoids SSE, because UEFI firmware doesn't guarantee
XMM state is preserved). clang — which compiles flite — uses **XMM0** for
`double` per the standard MS x64 ABI. So every `double`-typed call from flite's
C into the Rust shim (`sqrt`, `exp`, `log`, `pow`, `sin`, `fmod`, `ceil`,
`fabs`, and `atof`) exchanged its value through the wrong register file and got
garbage. This corrupted two things at once:
- **Amplitude:** MLSA/MLPG synthesis math (`exp`/`log`/`sqrt`/…) returned junk →
  near-silent output.
- **Duration:** `atof` (used by `val_float` on the `cg_break` string feature)
  returned junk → a duration-CART branch flipped → wrong frame count.

It was a Heisenbug under printf-debugging because any instrumentation that
re-walked the relation graph (`item_as`/`path_to_item`) had side effects that
masked it; the fix was found by feeding MLPG a fixed synthetic input (isolating
it from the relation graph) and disassembling the shim, which showed
`cstm_sqrt` reading its argument from `%rcx` instead of `%xmm0`.

There is **no clang flag** to avoid this: SSE2 is part of the x86_64 baseline,
so `-msoft-float`/`-mno-sse` are ignored (verified — args still arrive in
`%xmm0`). x86_64 has no non-SSE float-argument register (the x87 stack isn't
used for args in 64-bit mode).

**Fix:** a tiny bit-transport bridge. `uefi-port/math_bridge.c` (compiled by
clang, so it receives `double` in XMM0 natively) reinterprets each `double` as a
`u64` and forwards it to the Rust shim as an *integer* (`rust_*_bits`,
`rust_atof_bits`) — integer args/returns DO match between clang and Rust on this
target — then reinterprets the `u64` result back to `double`. flite's
`<math.h>`/`<stdlib.h>` `#define` `sqrt`/`exp`/…/`atof` to the `cstm_*` bridge
entry points. Hardware FP is kept everywhere; only the ABI boundary is bridged.

**Result:** UEFI now produces 19760 samples, peak 21058, 98% non-zero —
matching the host build (peak 21077). Reference output: `uefi-port/hello-uefi.wav`.

### Latent note (not exercised by the demo)
The `printf`-family `%f` path (`cprintf.rs`, varargs) reads `f64` from a
`VaList`; varargs floats may hit the same XMM-vs-integer issue. The compiled-in
voice path doesn't format floats, so it isn't triggered, but number-heavy text
output would need the same care.

---
## Fixes made while investigating (resolved)

- **`build-uefi-c.sh` never archived.** It only compiled objects; the committed
  `libflite_uefi.a` was stale, so C-side changes were silently *not* linked. It
  now runs `llvm-ar`; `build.rs` has `cargo:rerun-if-changed` on the archive.
- **`RAND_MAX`** was 32767 but the shim's `rand()` returns 0..2³¹; flite's
  mixed-excitation `rand() > RAND_MAX/2.0` was always true. Set to 2147483647.
- **Stall/lock after "WAV WRITE OK" on real hardware.** `main` spun forever (a
  QEMU convenience); it now returns EFI_SUCCESS to the firmware.
