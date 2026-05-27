# Known issues

## Synthesis is near-silent on UEFI (LLP64 data-model bug in flite) — OPEN

**Symptom:** The app runs and writes a well-formed `hello.wav`, but the audio is
near-silent: peak amplitude ~47 out of ±32767 (vs ~21000 on a host build of the
same code). **This reproduces in QEMU — it is NOT hardware-specific.** (An earlier
"WAV WRITE OK" check only validated the RIFF header, not the audio body.)

**Root cause (traced, not yet fixed):** flite assumes the LP64 data model
(`long` is 64-bit). The `x86_64-unknown-uefi` target is **LLP64** (`long` is
**32-bit**, pointers 64-bit). That divergence changes flite's behavior:

1. Building flite's C with clang for the **host** (LP64) + glibc → correct, loud
   audio (peak 21077). Building the *same source* for **UEFI** (LLP64) + our Rust
   shim → near-silent (peak 47). Ruled out as causes: libm (bit-identical to
   glibc), our `math.h` decls, allocator zeroing (`cst_alloc`→`calloc`, all
   MLPG/vector allocs too), `cst_val` layout (identical on both, 64-bit-pointer
   branch), optimization/UB (`-O0` identical), and the SSE FP environment (MXCSR
   already `0x1f80`).
2. The amplitude loss is in the **clustergen MLPG** trajectory smoother: the
   pre-MLPG energy coefficient `c[0]` max is correct (7.12 on both), but post-MLPG
   it is 5.56 (host) vs 1.45 (UEFI) — MLPG over-flattens the spectrum.
3. MLPG over-flattens because it runs on a **different frame/duration sequence**
   (17840 vs 19760 samples). The duration difference comes from a duration-CART
   branch that flips at the `lisp_cg_break` feature
   (`…R:Syllable.nn.lisp_cg_break`): `cg_break()` returns "4" (utterance-final) on
   host but "0" (word-internal) on UEFI, because `item_next()` on a syllable
   differs — i.e. **the utterance relation (syllable) structure is built
   differently** on UEFI.
4. So the defect is upstream, in flite's text-analysis / HRG construction, where
   some `long`-typed value (or `long`-dependent computation) behaves differently
   at 32-bit. The exact source line is not yet pinned.

**To finish the fix:** find the LP64 assumption in the tokenization / lexicon /
syllabification / feature code (`src/synth`, `src/hrg`, `src/utils`, `lang/…`) —
e.g. a value stored in a `long` that needs 64 bits, or `sizeof(long)` used as a
field width — and use a fixed-width type. Reproduce by compiling those files for
a 32-bit-`long` model. A host build with clang is the known-good reference.

**Mitigation idea (not implemented):** the pre-MLPG cluster means are correct
(loud); running synthesis with `do_mlpg` disabled would produce louder, if
rougher, audio. The CART/duration divergence would remain.

---
## Fixes made while investigating (resolved)

- **`build-uefi-c.sh` did not archive.** It only compiled objects; the committed
  `libflite_uefi.a` was never regenerated, so C-side changes were silently *not*
  linked. It now runs `llvm-ar` at the end. `build.rs` also gained
  `cargo:rerun-if-changed` on the archive so cargo relinks when it changes.
- **`RAND_MAX` was 32767** in `cinclude/stdlib.h` but the shim's `rand()` returns
  0..2³¹−1; flite's mixed-excitation `rand() > RAND_MAX/2.0` was therefore always
  true. Set `RAND_MAX` to 2147483647 to match. (Real bug; not the cause of the
  silence.)
- **Stall/lock after "WAV WRITE OK" on real hardware.** `main` spun forever
  (a QEMU convenience); it now returns EFI_SUCCESS to the firmware. (Committed.)
