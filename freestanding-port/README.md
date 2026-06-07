# Flite on `x86_64-unknown-uefi`

This directory builds [flite](https://github.com/festvox/flite) into a **Rust
library** (`flite-uefi`) that runs the CMU flite TTS engine on the UEFI target —
in firmware, with no operating system. Your own UEFI binary depends on the
library, synthesizes text to PCM, and feeds the samples to its audio driver. It
is a proof-of-concept / curiosity, **not** something intended for upstream flite.

## What it does

`flite-uefi` is a library crate that registers the compiled-in `cmu_us_slt`
clustergen voice and synthesizes text to 16-bit 16 kHz mono PCM. Two binaries
demonstrate it: `examples/synth.rs` (writes `hello.wav` to the ESP, used by the
QEMU test) and `consumer-demo/` (a *separate* crate showing how to depend on the
library from your own app). Demonstrated under QEMU + OVMF.

## Using the library

Depend on it by path and call the small safe API:

```toml
# your-app/Cargo.toml
[dependencies]
flite-uefi = { path = "../path/to/uefi-port/flite-uefi" }
[profile.dev]
panic = "abort"
[profile.release]
panic = "abort"
```

```rust
let voice = flite_uefi::init().expect("voice registration");
let wave  = voice.synthesize("hello world").expect("synthesis");
let pcm: &[i16] = wave.samples();       // interleaved 16-bit signed PCM
let rate        = wave.sample_rate();    // Hz
let channels    = wave.num_channels();
// hand `pcm` to your audio driver; or `wave.to_wav()` for RIFF/WAVE bytes.
```

Build your app with nightly for the UEFI target:
`cargo build --target x86_64-unknown-uefi`. No `#![feature(...)]` is needed in
your crate — the `c_variadic` feature is internal to the library. The flite C
archive (`libflite_uefi.a`) must exist next to the library crate; `build.rs`
finds it relative to the crate, so it works as a dependency.

## How it works

This **cross-compiles flite's C directly for UEFI** and supplies the C runtime
from Rust:

```
            flite C sources                Rust std EFI app (flite-uefi/)
  clang --target=x86_64-unknown-uefi   ┌─────────────────────────────────┐
  -ffreestanding -mno-red-zone         │ main.rs   calls the flite API    │
  -fshort-wchar -DDIE_ON_ERROR         │ shim.rs   malloc→std, ctype,     │
        + cinclude/ shim headers       │           str*, math→libm,       │
                 │                     │           stdio/file stubs       │
                 ▼                     │ cprintf.rs printf family          │
        libflite_uefi.a  ───linked───▶ │ wav.rs    cst_wave → WAV bytes   │
        (COFF x86-64)                  └─────────────────────────────────┘
                                                      │ cargo build
                                                      ▼  --target x86_64-unknown-uefi
                                              flite-uefi.efi  (PE32+)
```

Key points:
- clang's `x86_64-unknown-uefi` triple emits COFF and predefines `__UEFI__`
  (not `_WIN32`), so flite takes its generic header branches. The objects link
  directly with Rust's `x86_64-unknown-uefi` output (same Win64 ABI).
- `compiler-builtins` provides `memcpy`/`memmove`/`memset` — the shim must **not**
  redefine them (doing so via `core::ptr::copy*` recurses infinitely).
- `long` is 32-bit on this target; the printf shim accounts for that.
- The voice is **compiled in as `const` data**, so synthesis touches no files.
  The stdio/file/mmap symbols are link-only stubs.

## Prerequisites

- **clang / LLVM 19+** (needs the `x86_64-unknown-uefi` target; check with
  `clang --target=x86_64-unknown-uefi -E -x c /dev/null`), plus `llvm-ar`.
- **Rust nightly** with the UEFI target:
  `rustup toolchain install nightly && rustup target add --toolchain nightly x86_64-unknown-uefi`
  (the printf shim uses the nightly `c_variadic` feature).
- **QEMU** (`qemu-system-x86_64`) and **OVMF** firmware. On Arch:
  `pacman -S qemu-full edk2-ovmf`. Override the OVMF paths for your distro, e.g.
  Debian/Ubuntu: `make qemu OVMF_CODE=/usr/share/OVMF/OVMF_CODE.fd OVMF_VARS=/usr/share/OVMF/OVMF_VARS.fd`.
- `python3` (the `coff`/`elf` targets use it to read `compile_commands.json`).

## Build & run

Everything is driven by `uefi-port/Makefile`. Run the targets from `uefi-port/`
(or from anywhere with `make -C uefi-port <target>`); `make help` lists them all.

```bash
cd uefi-port

# 1. Build flite natively once (produces the static libs used by the native
#    sanity test, and lets you regenerate compile_commands.json if needed).
make native

# 2. (Optional) sanity-check the C library + voice on the host.
make native-test                  # => num_samples=... NATIVE OK

# 3. Cross-compile flite's C to the COFF archive for UEFI.
#    (libflite_uefi.a is checked in; this regenerates it. `make elf` builds
#    the parallel ELF archive for the freestanding/GRUB path.)
make coff                         # => libflite_uefi.a

# 4. Build the demo EFI application (the `synth` example of the library).
make synth

# 5. Run it under QEMU + OVMF (prints to the console, writes hello.wav).
make qemu                         # override timeout: make qemu QEMU_TIMEOUT=60
```

`make` with no target builds the archives and the `synth` example. Builds are
incremental — only C files whose sources changed are recompiled.

To clean up: `make clean` removes all build intermediates and run artifacts
(the `obj/` and `obj-elf/` object dirs, `esp/`, the cargo `target/` dirs, logs,
and the scratch files under `/tmp`), leaving the checked-in archive in place.
`make distclean` additionally removes the generated archives and runs the
native flite `make clean`.

Expected console output:

```
FLITE-UEFI: num_samples=19760 sample_rate=16000 num_channels=1
FLITE-UEFI: SYNTHESIS OK
FLITE-UEFI: wrote "hello.wav" (39564 bytes)
FLITE-UEFI: readback "hello.wav" ok, 39564 bytes, magic="RIFF"
FLITE-UEFI: WAV WRITE OK
```

`make qemu` uses QEMU's `fat:rw:` to back the ESP with `uefi-port/esp/`, so
after the run the produced file is on the host at
`uefi-port/esp/EFI/BOOT/hello.wav`. A reference copy is checked in at
`uefi-port/hello-uefi.wav`.

## Layout

| Path | Purpose |
|------|---------|
| `Makefile` | All build/run/clean targets (`make help`). Cross-compiles the C archives, builds the EFI demo, runs QEMU, and cleans up. |
| `cinclude/` | Minimal freestanding shim headers (`stdio.h`, `stdlib.h`, …) declaring only what flite uses. |
| `compile_commands.json` | The flite C file set (core + usenglish + cmulex + cmu_us_slt), audio/socket excluded. The `coff`/`elf` targets read it for the source list. |
| `libflite_uefi.a` | Checked-in COFF archive (regenerate with `make coff`). |
| `libflite_elf.a` | ELF archive for the freestanding/GRUB path (build with `make elf`; not checked in). |
| `uefi-undefined-symbols.txt` | The external C-runtime symbols the Rust shim provides. |
| `flite-uefi/` | The Rust **library**: `lib.rs` (public API), `shim.rs` (C runtime), `cprintf.rs`, `wav.rs`, `math_bridge.c` (f64 ABI bridge), and `examples/synth.rs` (the demo). |
| `consumer-demo/` | A *separate* crate depending on `flite-uefi` — the template for your own audio-driver binary. |
| `native-test.c` | Host sanity check of the C library + voice (`make native-test`). |
| `hello-uefi.wav` | Reference output produced by the UEFI app. |

## Limitations / notes

- Only `cmu_us_slt` (a US-English clustergen voice) is compiled in; one phrase
  is synthesized. Other voices/languages would each be linked in similarly.
- No audio playback (UEFI has no standard audio protocol) — output is PCM/WAV.
- Sample counts differ slightly from a host build because the shim's `rand()`
  and `libm` differ from glibc, nudging the clustergen duration model. Both are
  valid speech.
- `cargo build` requires `uefi-port/libflite_uefi.a` to exist (run step 3 first
  if you remove the checked-in copy).
