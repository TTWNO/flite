# Flite on `x86_64-unknown-uefi`

This directory builds [flite](https://github.com/festvox/flite) into a **UEFI
application** that synthesizes text to PCM and writes a `.wav` to the EFI System
Partition — running in firmware, with no operating system. It is a
proof-of-concept / curiosity, **not** something intended for upstream flite.

## What it does

`flite-uefi.efi` registers the compiled-in `cmu_us_slt` clustergen voice,
synthesizes `"hello world"` to 16-bit 16 kHz mono PCM, prints the sample count
to the UEFI console, and writes `hello.wav` to the ESP (verified by reading it
back in-firmware). Demonstrated under QEMU + OVMF.

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

# 4. Build the EFI application.
cd uefi-port/flite-uefi
cargo +nightly build --target x86_64-unknown-uefi
cd ../..

# 5. Run it under QEMU + OVMF (prints to the console, writes hello.wav).
bash uefi-port/run-qemu.sh        # optional arg: timeout seconds (default 30)
```

Expected console output:

```
FLITE-UEFI: num_samples=17840 sample_rate=16000 num_channels=1
FLITE-UEFI: SYNTHESIS OK
FLITE-UEFI: wrote "hello.wav" (35724 bytes)
FLITE-UEFI: readback "hello.wav" ok, 35724 bytes, magic="RIFF"
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
| `flite-uefi/` | The Rust `std` EFI app: `shim.rs`, `cprintf.rs`, `wav.rs`, `main.rs`. |
| `run-qemu.sh` | Assembles an ESP and boots the app in QEMU + OVMF. |
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
