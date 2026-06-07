# Developer Guide

This describes the process of updating, patching, and upstreaming new features into this version of `flite/`.

## Add a voice/featureset already supported by Flite

1. Add its path to an entre in `freestanding-port/compile_commands.json)
2. `make coff && make elf` (make for both x86 UEFI and x86 freestanding)
3. Determine if there is a new symbol to fill in by running: `TODO`
4. If there is, add it to `flite-freestanding/src/shim.rs` and to `ueffi-undefined-symbols.txt`

## Expose a new C-compatible API

1. Declare the C funciton in the `extern "C"` block in `flite-freestanding/src/lib.rs`
2. Add a save wrapper mothed that executes on `CstWave`, `CstVoice` or some other flite-derived struct.

## Add a missing `libc`-derived function

1. Add `#[no_mangle] pub unsafe extern "C" fn <name>(...) -> ...` in `shim.rs`
2. Add its declaration to the `cinclude/*.h` with the appropriate header namme filled in.
3. *NOTE: do not define `memcpy/memmove/memset` as they are defined by the `compiler_builtins` module available on all Rust targets.

## Fix a missing symbol issue

1. Run `llvm-nm libflite_coff.a | grep <sym>` to see if it's `U` (undefined), `T` (defined).
2. Compare with `uefi-undefined-symbols.txt`.

## Integrate it with an audio driver to use speakers for synthesis

See our current freestanding audio implementations:

- Intel HDA: https://github.com/accessible-firmware/hda_freestanding/
- ...more coming soon.
