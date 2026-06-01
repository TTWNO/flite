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

## How it works (and why not c2rust)

The original goal was to transpile flite to Rust with **c2rust**. That hit a
wall: the c2rust build available here hangs with unbounded memory on flite's
type-heavy core (`cst_features`/`cst_item`/`cst_cg`/…). See the design spec
under `docs/superpowers/specs/` for the root-cause writeup.

So instead this **cross-compiles flite's C directly for UEFI** and supplies the
C runtime from Rust:

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
  `pacman -S qemu-full edk2-ovmf`. Adjust the OVMF paths in `run-qemu.sh` for
  your distro (Debian/Ubuntu: `/usr/share/OVMF/OVMF_CODE.fd` etc.).
- `python3` (used by `build-uefi-c.sh` to read `compile_commands.json`).

## Build & run

All commands are run **from the flite repo root**.

```bash
# 1. Build flite natively once (produces the static libs used by the native
#    sanity test, and lets you regenerate compile_commands.json if needed).
./configure --with-audio=none CFLAGS="-g -O2 -fPIC -DDIE_ON_ERROR"
make
make -C lang/cmu_us_slt        # the clustergen voice

# 2. (Optional) sanity-check the C library + voice on the host.
bash uefi-port/native-test.sh   # => num_samples=... NATIVE OK

# 3. Cross-compile flite's C to a COFF archive for UEFI.
#    (uefi-port/libflite_uefi.a is checked in; this regenerates it.)
bash uefi-port/build-uefi-c.sh  # => uefi-port/libflite_uefi.a

# 4. Build the demo EFI application (the `synth` example of the library).
cd uefi-port/flite-uefi
cargo +nightly build --example synth --target x86_64-unknown-uefi
cd ../..

# 5. Run it under QEMU + OVMF (prints to the console, writes hello.wav).
bash uefi-port/run-qemu.sh        # optional arg: timeout seconds (default 30)
```

Expected console output:

```
FLITE-UEFI: num_samples=19760 sample_rate=16000 num_channels=1
FLITE-UEFI: SYNTHESIS OK
FLITE-UEFI: wrote "hello.wav" (39564 bytes)
FLITE-UEFI: readback "hello.wav" ok, 39564 bytes, magic="RIFF"
FLITE-UEFI: WAV WRITE OK
```

`run-qemu.sh` uses QEMU's `fat:rw:` to back the ESP with `uefi-port/esp/`, so
after the run the produced file is on the host at
`uefi-port/esp/EFI/BOOT/hello.wav`. A reference copy is checked in at
`uefi-port/hello-uefi.wav`.

## Layout

| Path | Purpose |
|------|---------|
| `cinclude/` | Minimal freestanding shim headers (`stdio.h`, `stdlib.h`, …) declaring only what flite uses. |
| `build-uefi-c.sh` | Cross-compiles every file in `compile_commands.json` to COFF and archives `libflite_uefi.a`. |
| `compile_commands.json` | The flite C file set (core + usenglish + cmulex + cmu_us_slt), audio/socket excluded. |
| `libflite_uefi.a` | Checked-in COFF archive (regenerate with `build-uefi-c.sh`). |
| `uefi-undefined-symbols.txt` | The external C-runtime symbols the Rust shim provides. |
| `flite-uefi/` | The Rust **library**: `lib.rs` (public API), `shim.rs` (C runtime), `cprintf.rs`, `wav.rs`, `math_bridge.c` (f64 ABI bridge), and `examples/synth.rs` (the demo). |
| `consumer-demo/` | A *separate* crate depending on `flite-uefi` — the template for your own audio-driver binary. |
| `run-qemu.sh` | Assembles an ESP and boots the `synth` example in QEMU + OVMF. |
| `native-test.{c,sh}` | Host sanity check of the C library + voice. |
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
