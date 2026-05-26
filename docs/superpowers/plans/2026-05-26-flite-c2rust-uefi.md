# Flite → c2rust → x86_64-unknown-uefi Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Transpile the flite C TTS library to Rust with c2rust, build it for `x86_64-unknown-uefi`, and run it under QEMU+OVMF to synthesize text → PCM and write a `.wav` to the EFI System Partition.

**Architecture:** Whole-project c2rust transpile of flite core + `usenglish` + `cmulex` + the compiled-in `cmu_us_slt` CG voice. A hand-written `libc_shim` Rust crate provides the C-runtime symbols c2rust references (`malloc`/`free`/`mem*`/`str*` over `std`/`core`, math over the `libm` crate, `cst_errmsg`→UEFI console, unused stdio/file/socket → stubs). A thin `uefi-app` `std` EFI binary calls the flite API. Host build is the functional gate before UEFI.

**Tech Stack:** flite (C, autotools), c2rust 0.22.1, nightly rustc 1.97, `libm` crate, Rust `std` for `x86_64-unknown-uefi` (prebuilt, no `-Zbuild-std`), QEMU 11 + OVMF.

**Spec:** `docs/superpowers/specs/2026-05-26-flite-c2rust-uefi-design.md`

---

## File Structure

All new work lives under `uefi-port/` on branch `flite-c2rust-uefi`:

- `uefi-port/compile_commands.json` — captured from the native build (generated).
- `uefi-port/flite-rs/` — c2rust output: transpiled flite as a Rust crate (generated; treated as vendored).
- `uefi-port/libc_shim/` — Rust crate: C-runtime symbols (`#[no_mangle] extern "C"` fns) over std/core/libm. The single bridge between C-isms and the environment.
- `uefi-port/uefi-app/` — Rust `[[bin]]` `std` EFI app: `fn main()` calls flite, writes WAV to ESP, prints to console.
- `uefi-port/host-test/` — Rust `[[bin]]` host harness that links `flite-rs` + `libc_shim` and asserts synthesis produces samples (Checkpoint 3 gate).
- `uefi-port/run-qemu.sh` — assembles ESP image and boots it in QEMU+OVMF.

C-source edits (minimal, in the flite tree):
- `src/utils/cst_error.c:95` — guard the unconditional `jmp_buf *cst_errjmp` under `#ifndef DIE_ON_ERROR`.

---

## Checkpoint 1: Native build + `compile_commands.json`

**Goal:** Produce a `compile_commands.json` covering exactly the C files we want to transpile (core + usenglish + cmulex + cmu_us_slt), built with `--with-audio=none -DDIE_ON_ERROR`.

**Files:**
- Modify: `src/utils/cst_error.c:95`
- Create: `uefi-port/compile_commands.json`

- [ ] **Step 1: Guard the setjmp symbol in cst_error.c**

In `src/utils/cst_error.c`, change the unconditional declaration (around line 93-96):
```c
#ifndef WASM32_WASI
jmp_buf *cst_errjmp = 0;
#endif
```
to also exclude the DIE_ON_ERROR build:
```c
#if !defined(WASM32_WASI) && !defined(DIE_ON_ERROR)
jmp_buf *cst_errjmp = 0;
#endif
```

- [ ] **Step 2: Verify the guarded file compiles under the flag**

Run: `gcc -DDIE_ON_ERROR -Iinclude -fsyntax-only src/utils/cst_error.c && echo OK`
Expected: `OK` (previously failed with `unknown type name 'jmp_buf'`).

- [ ] **Step 3: Configure flite for a clean, audio-free build**

Run:
```bash
cd /home/tait/Documents/flite
make distclean 2>/dev/null; true
./configure --with-audio=none CFLAGS="-g -O2 -fPIC -DDIE_ON_ERROR"
```
Expected: configure completes; `config/config` exists. Confirm the audio backend: `grep -i audio config/config` should show `none`.

- [ ] **Step 4: Determine how `cmu_us_slt` gets compiled in**

The CG voice lives in `lang/cmu_us_slt/` (provides `register_cmu_us_slt`). Discover the build path:
```bash
ls lang/cmu_us_slt/*.c | head
grep -rn 'register_cmu_us_slt' lang/cmu_us_slt include 2>/dev/null | head
cat config/config | grep -iE 'vox|lang|voice'
```
If the default `make` does not build `lang/cmu_us_slt`, build that subdir directly (`make -C lang/cmu_us_slt`) so its compiles are captured. Record the exact registration symbol name and signature for use in Checkpoint 3/5.

- [ ] **Step 5: Capture compile commands**

Run a clean build under compiledb:
```bash
make clean 2>/dev/null; true
~/.local/bin/compiledb make            # core lib + langs
~/.local/bin/compiledb -o compile_commands.json make -C lang/cmu_us_slt   # if not already built
```
(compiledb appends/merges by default; verify the slt files appear.)

- [ ] **Step 6: Filter to the transpile set and stage it**

Drop entries we will NOT transpile (we write our own entry / no server): everything under `main/`, `sapi/`, `wince/`, `windows/`, `tools/`, `testsuite/`, and any `cst_socket`/`cst_file_wince`/`cst_file_palmos`/`cst_mmap_win32` files. Keep: `src/**` (incl. `cst_file_stdio.c`, `cst_alloc.c`, `audio/audio_none.c`), `lang/usenglish/**`, `lang/cmulex/**`, `lang/cmu_us_slt/**`.
```bash
mkdir -p uefi-port
# produce filtered uefi-port/compile_commands.json (use jq/python to drop excluded dirs)
```
Verify: `jq '[.[].file] | length' uefi-port/compile_commands.json` is non-zero and `jq -r '.[].file' uefi-port/compile_commands.json | grep -E 'cmu_us_slt|usenglish|cmulex' | head` shows the voice/lang files.

- [ ] **Step 7: Commit**

```bash
git add src/utils/cst_error.c uefi-port/compile_commands.json
git commit -m "checkpoint 1: audio-free DIE_ON_ERROR build + compile_commands"
```

**Gate:** `uefi-port/compile_commands.json` lists core + usenglish + cmulex + cmu_us_slt `.c` files, all compiled with `-DDIE_ON_ERROR`, none from `main/`/`sapi/`/socket.

---

## Checkpoint 2: c2rust transpile

**Goal:** Turn the C set into a Rust crate.

**Files:**
- Create: `uefi-port/flite-rs/**` (generated)

- [ ] **Step 1: Run c2rust**

```bash
cd /home/tait/Documents/flite/uefi-port
c2rust transpile compile_commands.json --emit-build-files -o flite-rs 2>&1 | tee transpile.log
```
Expected: per-file `Transpiling …` lines; a `flite-rs/` crate with `Cargo.toml`, `build.rs`, `lib.rs`, and one `.rs` per `.c`. Some `WARN` lines are normal.

- [ ] **Step 2: Triage transpile errors (if any)**

If c2rust errors on specific files (e.g. huge const tables, unusual constructs), record them. For const-table or attribute issues, the usual fixes are c2rust flags (`--reduce-type-annotations`, `--translate-const-macros`) or excluding/retrying the offending file. Do NOT hand-edit generated `.rs` yet — prefer re-running with flags so the step is reproducible.

- [ ] **Step 3: Inventory the extern symbol surface**

```bash
grep -rhoE 'fn [a-z_][a-zA-Z0-9_]*\(' flite-rs/src/*.rs | sort -u > /tmp/defined.txt
grep -rhA40 'extern "C" {' flite-rs/src/*.rs | grep -oE 'fn [a-z_][a-zA-Z0-9_]*' | sort -u > /tmp/referenced.txt
comm -13 /tmp/defined.txt /tmp/referenced.txt   # roughly: symbols needing the shim
```
Save the list — this drives the `libc_shim` in Checkpoint 3.

- [ ] **Step 4: Commit**

```bash
git add uefi-port/flite-rs uefi-port/transpile.log
git commit -m "checkpoint 2: c2rust transpile of flite core + cmu_us_slt"
```

**Gate:** `flite-rs/` crate emitted; the unresolved-extern symbol list is captured.

---

## Checkpoint 3: Build + functionally validate on the HOST

**Goal:** Link `flite-rs` + `libc_shim` on `x86_64-unknown-linux-gnu` and prove synthesis works (`num_samples > 0`) **before** touching UEFI. This isolates "transpiled flite is correct" from "links for UEFI."

**Files:**
- Create: `uefi-port/libc_shim/Cargo.toml`, `uefi-port/libc_shim/src/lib.rs`
- Create: `uefi-port/host-test/Cargo.toml`, `uefi-port/host-test/src/main.rs`

- [ ] **Step 1: Make flite-rs a library crate the others can depend on**

Ensure `flite-rs/Cargo.toml` builds as a lib exposing the transpiled modules; note its crate name (e.g. `flite_rs`). Add a top-level `Cargo.toml` workspace under `uefi-port/` listing `flite-rs`, `libc_shim`, `host-test` (and later `uefi-app`).

- [ ] **Step 2: Write the libc_shim crate**

`uefi-port/libc_shim/src/lib.rs` provides `#[no_mangle] pub extern "C"` definitions for every symbol from Checkpoint 2 step 3. Core allocation pattern (self-describing, ABI-stable, works on host and UEFI via `std::alloc`):
```rust
#![allow(non_camel_case_types)]
use core::ffi::{c_void, c_char, c_int};
use std::alloc::{alloc, dealloc, Layout};

const HDR: usize = 16; // keep 16-byte alignment; store size in first 8 bytes

#[no_mangle]
pub unsafe extern "C" fn malloc(size: usize) -> *mut c_void {
    if size == 0 { return core::ptr::null_mut(); }
    let total = size + HDR;
    let p = alloc(Layout::from_size_align(total, 16).unwrap());
    if p.is_null() { return core::ptr::null_mut(); }
    (p as *mut usize).write(total);
    p.add(HDR) as *mut c_void
}
#[no_mangle]
pub unsafe extern "C" fn free(ptr: *mut c_void) {
    if ptr.is_null() { return; }
    let base = (ptr as *mut u8).sub(HDR);
    let total = (base as *mut usize).read();
    dealloc(base, Layout::from_size_align(total, 16).unwrap());
}
#[no_mangle]
pub unsafe extern "C" fn calloc(n: usize, sz: usize) -> *mut c_void {
    let total = n.saturating_mul(sz);
    let p = malloc(total);
    if !p.is_null() { core::ptr::write_bytes(p as *mut u8, 0, total); }
    p
}
#[no_mangle]
pub unsafe extern "C" fn realloc(ptr: *mut c_void, sz: usize) -> *mut c_void {
    if ptr.is_null() { return malloc(sz); }
    let old_total = ((ptr as *mut u8).sub(HDR) as *mut usize).read();
    let old = old_total - HDR;
    let np = malloc(sz);
    if !np.is_null() { core::ptr::copy_nonoverlapping(ptr as *const u8, np as *mut u8, old.min(sz)); free(ptr); }
    np
}
```
Then `mem*`/`str*` (`memcpy`/`memmove`/`memset`/`memcmp`/`strlen`/`strcmp`/`strncmp`/`strcpy`/`strncpy`/`strcat`/`strchr`/`strrchr`/`strdup`/`strstr` as the symbol list requires) implemented over `core::ptr`/slices. Add only the symbols the inventory actually shows.

- [ ] **Step 3: Add math + diagnostic shims**

Math symbols (whatever the inventory shows: `cos`/`sin`/`tan`/`atan2`/`exp`/`log`/`log10`/`pow`/`sqrt`/`floor`/`ceil`/`fabs`/`fmod`/`frexp`/`ldexp`...) wrap the `libm` crate:
```rust
#[no_mangle] pub extern "C" fn cos(x: f64) -> f64 { libm::cos(x) }
// ...one per referenced math symbol
```
Diagnostics: `cst_errmsg`/`cst_dbgmsg` are varargs (`vfprintf`-style). On host, route to `eprint!` via a minimal `vsnprintf`-into-buffer (or, if c2rust already lowered these through `vsnprintf`, just implement `vsnprintf`). Stub the unused C runtime: `exit`/`abort` → `panic!`; any `fopen`/`fread`/`fwrite`/`fprintf`/socket symbols still referenced → `unimplemented!()`/return error (they must NOT be hit by the CG-from-ROM path; if one is hit, that's a real finding).

- [ ] **Step 4: Write the host functional test**

`uefi-port/host-test/src/main.rs`:
```rust
// extern decls for the flite API we call (names/signature confirmed in CP1 step 4)
extern "C" {
    fn flite_init() -> ::std::os::raw::c_int;
    fn register_cmu_us_slt(voxdir: *const ::std::os::raw::c_char) -> *mut core::ffi::c_void; // returns cst_voice*
    fn flite_text_to_wave(text: *const ::std::os::raw::c_char, voice: *mut core::ffi::c_void) -> *mut Wave;
}
#[repr(C)] struct Wave { /* mirror cst_wave: type*, sample_rate:int, num_samples:int, num_channels:int, samples:*mut i16 */ }
fn main() {
    unsafe {
        flite_init();
        let v = register_cmu_us_slt(core::ptr::null());
        assert!(!v.is_null(), "voice registration failed");
        let txt = b"hello world\0";
        let w = flite_text_to_wave(txt.as_ptr() as *const _, v);
        assert!(!w.is_null());
        let n = (*w).num_samples; // read the real field offset
        println!("num_samples = {n}");
        assert!(n > 0, "no audio produced");
    }
}
```
(Use the real `cst_wave` layout from `include/cst_wave.h`; mirror it exactly with `#[repr(C)]`.)

- [ ] **Step 5: Build and run on host**

```bash
cd uefi-port && cargo run -p host-test 2>&1 | tail -20
```
Iterate: add any missing shim symbol the linker reports until it links, runs, and prints `num_samples = <positive>`.
Expected: links cleanly, prints a positive `num_samples`, exits 0.

- [ ] **Step 6: Commit**

```bash
git add uefi-port/libc_shim uefi-port/host-test uefi-port/Cargo.toml
git commit -m "checkpoint 3: libc_shim + transpiled flite synthesizes on host"
```

**Gate:** `cargo run -p host-test` prints a positive `num_samples` and exits 0. (If any file/socket stub is hit, fix the cause before proceeding.)

---

## Checkpoint 4: Compile for `x86_64-unknown-uefi`

**Goal:** The same `flite-rs` + `libc_shim` compile for the UEFI target.

**Files:**
- Modify: `uefi-port/libc_shim/src/lib.rs` (cfg-gate any host-only bits)
- Create: `uefi-port/uefi-app/Cargo.toml`, `uefi-port/uefi-app/src/main.rs` (stub `fn main(){}` for now)

- [ ] **Step 1: Add the UEFI app crate as the build target**

`uefi-port/uefi-app/Cargo.toml` depends on `flite-rs` and `libc_shim`. `src/main.rs` initially just `fn main() {}` to get a clean target build. (x86_64-unknown-uefi std provides normal `fn main`.)

- [ ] **Step 2: Build for the target**

```bash
cd uefi-port && cargo build -p uefi-app --target x86_64-unknown-uefi 2>&1 | tee build-uefi.log
```

- [ ] **Step 3: Resolve target-specific breakage**

Likely items: host-only `std` usage in the shim (replace `eprint!`/`println!` with cfg-gated console writes — full console output lands in CP5; for now route diagnostics to a no-op or a buffer under `#[cfg(target_os = "uefi")]`); any symbol the host resolved via the system that the UEFI std doesn't (e.g. extra `mem*`). The transpiled `flite-rs` should be target-agnostic (it's `core::ffi` based); shim is where target cfgs live. Iterate until the link succeeds.

- [ ] **Step 4: Confirm an EFI binary was produced**

```bash
file target/x86_64-unknown-uefi/debug/uefi-app.efi
```
Expected: `PE32+ executable (EFI application)`.

- [ ] **Step 5: Commit**

```bash
git add uefi-port/uefi-app uefi-port/libc_shim
git commit -m "checkpoint 4: flite-rs + shim link into an EFI binary"
```

**Gate:** `uefi-app.efi` exists and is a `PE32+ EFI application`.

---

## Checkpoint 5: Run in QEMU+OVMF (synthesize + count samples)

**Goal:** Boot the EFI app under QEMU; it synthesizes and prints sample count to the UEFI console.

**Files:**
- Modify: `uefi-port/uefi-app/src/main.rs`
- Create: `uefi-port/run-qemu.sh`

- [ ] **Step 1: Implement the synthesis path in the EFI app**

`fn main()` mirrors the host test: `flite_init()` → `register_cmu_us_slt(null)` → `flite_text_to_wave("hello world", voice)` → read `num_samples`. Print via `println!` (UEFI std maps stdout to the console) — confirm output is visible in QEMU's serial/console.

- [ ] **Step 2: Write the QEMU runner**

`uefi-port/run-qemu.sh`:
```bash
#!/usr/bin/env bash
set -euo pipefail
EFI=target/x86_64-unknown-uefi/debug/uefi-app.efi
ESP=$(mktemp -d)
mkdir -p "$ESP/EFI/BOOT"
cp "$EFI" "$ESP/EFI/BOOT/BOOTX64.EFI"
cp /usr/share/edk2/x64/OVMF_VARS.4m.fd /tmp/OVMF_VARS.fd
qemu-system-x86_64 -nographic \
  -drive if=pflash,format=raw,unit=0,readonly=on,file=/usr/share/edk2/x64/OVMF_CODE.4m.fd \
  -drive if=pflash,format=raw,unit=1,file=/tmp/OVMF_VARS.fd \
  -drive format=raw,file=fat:rw:"$ESP" \
  -net none
```
(QEMU's `fat:rw:<dir>` synthesizes a FAT ESP from the directory — no image-building needed.)

- [ ] **Step 3: Build and run**

```bash
cd uefi-port && cargo build -p uefi-app --target x86_64-unknown-uefi
bash run-qemu.sh 2>&1 | tee qemu-run.log
```
Watch the UEFI shell/console: the app should auto-launch from `EFI/BOOT/BOOTX64.EFI` and print `num_samples = <positive>`. (Add a key-wait or `qemu … -action …` / timeout so the run terminates; `-nographic` routes console to the terminal. Exit QEMU with Ctrl-A X.)
Expected: `num_samples = <positive>` visible in `qemu-run.log`.

- [ ] **Step 4: Commit**

```bash
git add uefi-port/uefi-app/src/main.rs uefi-port/run-qemu.sh uefi-port/qemu-run.log
git commit -m "checkpoint 5: flite synthesizes under QEMU+OVMF"
```

**Gate:** `qemu-run.log` shows a positive `num_samples` printed from inside the UEFI app.

---

## Checkpoint 6: Write WAV to the EFI System Partition

**Goal:** The app writes a real `.wav` to the ESP; we recover it and verify it's valid PCM.

**Files:**
- Modify: `uefi-port/uefi-app/src/main.rs`
- Modify: `uefi-port/run-qemu.sh` (persist the ESP dir so we can inspect output)

- [ ] **Step 1: Serialize the wave to a WAV byte buffer in Rust**

Read `cst_wave` fields (`sample_rate`, `num_samples`, `num_channels`, `samples: *mut i16`) and build a canonical 16-bit PCM WAV header + sample bytes into a `Vec<u8>`. (Avoid flite's `cst_wave_save` file path; do it in Rust to keep the file write on the UEFI side.)

- [ ] **Step 2: Write the buffer to the ESP**

Use std fs on the UEFI target if available (`std::fs::write("\\hello.wav", &buf)`), else the UEFI Simple File System protocol via `std::os::uefi` + the `r-efi`/`uefi` crate. Pick whichever the target actually supports — verify by writing a 1-byte probe file first.

- [ ] **Step 3: Run and recover the file**

Modify the runner to use a fixed (non-temp) `$ESP` dir so the host can read it back after QEMU exits:
```bash
ls -l "$ESP"/hello.wav
xxd "$ESP"/hello.wav | head    # confirm "RIFF"/"WAVE" magic
```
Optionally play/inspect on host: `file "$ESP"/hello.wav` → `RIFF (little-endian) data, WAVE audio, ... 16 bit, mono ...`.
Expected: a non-empty `hello.wav` with valid RIFF/WAVE header and `num_samples`×2 bytes of PCM.

- [ ] **Step 4: Final commit**

```bash
git add uefi-port/uefi-app/src/main.rs uefi-port/run-qemu.sh
git commit -m "checkpoint 6: write synthesized WAV to ESP from UEFI"
```

**Gate:** A valid `hello.wav` (RIFF/WAVE, correct sample count) is produced by the UEFI app and recovered on the host. **This is the success bar from the spec.**

---

## Notes for the implementer

- **Iterate the shim by linker error.** Don't try to predict every symbol — build, read the undefined-symbol/`extern` errors, add exactly those. The Checkpoint 2 inventory is a starting list, not exhaustive.
- **Keep `flite-rs/` generated and unedited** where possible. Reproducibility lives in the c2rust flags and the C-source guard, not in hand-edits to transpiled code. If a generated-code edit is truly unavoidable, document it.
- **A stub being hit at runtime is a finding, not a pass.** If `fopen`/socket/etc. fires during synthesis, the CG-from-ROM assumption is wrong somewhere — investigate before stubbing it to succeed.
- Use @superpowers:verification-before-completion before claiming any checkpoint gate is met — paste the actual command output.
- Use @superpowers:systematic-debugging when a checkpoint gate fails.
