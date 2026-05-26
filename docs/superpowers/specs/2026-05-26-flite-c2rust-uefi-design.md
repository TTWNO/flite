# Flite → c2rust → x86_64-unknown-uefi — Design

Date: 2026-05-26

## Goal

Transpile the flite C text-to-speech library to Rust with **c2rust**, build it for the
**`x86_64-unknown-uefi`** Rust target, and demonstrate it **running under QEMU + OVMF**:
synthesize text to PCM samples in memory and write a `.wav` to the EFI System Partition.

There is no audio hardware in UEFI, so "output" = PCM/WAV buffer, not playback.

## Decisions (from brainstorming)

- **Success bar:** Runs in QEMU/OVMF (not just compiles).
- **Voice:** A clustergen (CG) voice — `cmu_us_slt` — **compiled in as `const` data**
  (from `lang/cmu_us_slt/*.c`), so no runtime file I/O for the voice.
- **Runtime deps:** Use the UEFI target's **prebuilt `std`** (provides a global allocator
  backed by Boot Services `AllocatePool`); use the **`libm` crate** for math functions.
- **Error handling:** Compile flite with `-DDIE_ON_ERROR`. This makes `cst_error()` expand
  to `abort()` and removes `#include <setjmp.h>` / `longjmp` from `cst_error.h` entirely. No
  setjmp shim needed. **One small source edit is required:** `src/utils/cst_error.c:95`
  unconditionally declares `jmp_buf *cst_errjmp = 0;`, which fails to compile once the header
  no longer pulls in `<setjmp.h>`. Guard that line under `#ifndef DIE_ON_ERROR` (mirroring the
  existing `WASM32_WASI` guard). This is a tracked task at checkpoint 1, not free.

## Environment (verified)

- Output location: a new in-repo subdir **`uefi-port/`** (holds `flite-rs/` transpiled crate,
  `libc_shim`, `uefi-app`) on branch `flite-c2rust-uefi`.
- QEMU 11.0.0 present (`qemu-system-x86_64`); OVMF firmware at
  `/usr/share/edk2/x64/OVMF_CODE.4m.fd` + `OVMF_VARS.4m.fd` (split) — no installs needed.
- `compiledb` present (`~/.local/bin/compiledb`) for capturing `compile_commands.json`.

## Verified facts

- Toolchain present: c2rust 0.22.1, nightly rustc 1.97, `x86_64-unknown-uefi` target installed.
- `std` IS prebuilt for `x86_64-unknown-uefi` on this toolchain (`libstd-*.rlib` present) →
  no `-Zbuild-std` required.
- c2rust emits bare `extern "C"` decls for `malloc`/`free`/`memset`/`strlen`/`cos`/… (it does
  NOT depend on the `libc` crate) → we must *provide* these symbols at link time.
- libc usage is centralized: `src/utils/cst_alloc.c` (alloc) and `cst_file_*.c` (file I/O).
- `cst_error()` is a macro; with `-DDIE_ON_ERROR` it is just `abort()`. The `cst_errjmp`
  catch path is only used by server/main code we are not building, BUT `cst_error.c:95`
  declares `jmp_buf *cst_errjmp = 0;` unconditionally → needs a `#ifndef DIE_ON_ERROR` guard
  (verified: compile fails with the flag, succeeds without).
- `cst_errmsg`/`cst_dbgmsg` use `vfprintf(stderr, fmt, args)` (`cst_error.c:104`) — a real
  varargs path. The shim must route this through a `vsnprintf`-style formatter to the UEFI
  console, not a plain string write.

## Architecture (Approach A: whole-project transpile + std-backed libc shim)

### Pipeline
1. **Native build for capture.** `./configure --with-audio=none` (drops alsa/pulse/socket
   audio backends), build under `bear`/`compiledb` to produce a real `compile_commands.json`
   covering: core lib + `lang/usenglish` + `lang/cmulex` + `lang/cmu_us_slt`. Build flite
   with `-DDIE_ON_ERROR`.
2. **Transpile.** `c2rust transpile compile_commands.json --emit-build-files -o flite-rs/`
   → a Rust crate of the whole compiled set.
3. **libc shim** (hand-written Rust, linked alongside transpiled code):
   - `malloc/calloc/realloc/free` → `std::alloc` with the standard size-header trick.
   - `memcpy/memmove/memset/memcmp`, `strlen/strcmp/strcpy/strcat/strchr/strdup/...`
     → `core`/`alloc` implementations.
   - math (`cos/sin/pow/exp/log/sqrt/floor/ceil/fabs/atan2/...`) → `libm` crate wrappers.
   - `stdio`/file/`socket`/`exit` → stubs (panic/no-op; not exercised by CG-from-ROM path).
   - `abort` → panic; `cst_errmsg`/`cst_dbgmsg` → write to UEFI console.
4. **UEFI entry crate** (`std` EFI app): `flite_init()` → `register_cmu_us_slt(NULL)` →
   `flite_text_to_wave("hello world", voice)` → `cst_wave` PCM.
   (Signature: `cst_wave *flite_text_to_wave(const char *text, cst_voice *voice)` — text first.) Write `.wav` to the ESP via
   UEFI Simple File System protocol; print sample count to the UEFI console.
5. **Build & run.** `cargo build --target x86_64-unknown-uefi`; assemble an ESP image
   (`BOOTX64.EFI`); run under QEMU + OVMF; capture console output and the produced `.wav`.

### Risk-retiring checkpoints (fail fast)
1. Native `--with-audio=none` build + `compile_commands.json` generated.
2. c2rust transpile completes; crate emitted.
3. Transpiled crate **compiles for the host** (resolve shim symbols on a normal target first).
4. Transpiled crate **compiles for `x86_64-unknown-uefi`** (every extern symbol resolved).
5. Tiny "synthesize 1 word, count samples > 0" **runs in QEMU**.
6. WAV written to ESP and recovered as evidence.

## Components / boundaries
- `flite-rs/` — c2rust output (transpiled flite + compiled-in voice). Treated as generated.
- `libc_shim` — Rust module: C runtime symbols over std/libm/core. The single bridge between
  transpiled C-isms and the UEFI environment.
- `uefi-app` — thin EFI binary: calls flite API, handles ESP file output + console.

## Key uncertainties to surface during execution
- Full extern-symbol surface after transpile (may reveal extra libc calls needing stubs).
- Whether the CG voice's large `const` tables transpile/compile cleanly.
- Any `vsnprintf`/varargs usage in `cst_errmsg` — may need a minimal formatting stub.

## Out of scope
- Audio playback (no UEFI audio protocol).
- Voice loading from `.flitevox` files at runtime.
- Server/socket mode, the `flite` CLI, multiple voices/languages.
