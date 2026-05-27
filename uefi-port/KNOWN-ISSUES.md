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
4. The duration difference comes from flite's **item/relation graph** (HRG): the
   `nn` (next-next) navigation in the **Syllable** relation returns a different
   item on UEFI. Verified by step-by-step tracing:
   - The utterance structure is *identical* on both (10 segments, 3 syllables,
     2 words, phones `[pau hh ax l ow w er l d pau]`, syllable sizes [2,2,4]).
   - Per-syllable `cg_break` is identical: [0, 1, 4].
   - For the diverging segstate (`hh_81`, syllable 0), `R:…R:Syllable` lands in
     relation `"Syllable"` on **both**, and the syllable's own break is 0 on both.
   - But `item_next(item_next(syl0))` in the Syllable relation yields syllable 2
     (break **4**) on host and a break-**0** item on UEFI — i.e. the Syllable
     relation's `n`-pointer chain reached via the feature path is **inconsistent
     with the same relation's clean head-walk** (which has 3 distinct items on
     both). This is a deterministic linkage corruption of the relation graph that
     only appears at `long`=32.
5. **AddressSanitizer on the host build is clean** (no overflow/UAF), so the bug
   does not manifest as an out-of-bounds access at `long`=64 — it is specific to
   the 32-bit-`long` (LLP64) data model and is invisible to host tooling.

6. **It is a layout-sensitive Heisenbug.** Probing perturbs it: calling
   `item_as`/`path_to_item` from instrumentation lazily creates/caches relation
   mappings and makes the divergence disappear at the probe point, while the
   *stable, instrumentation-independent* symptoms (num_samples 17840 vs 19760;
   peak amplitude ~47 vs ~21000) persist. Individually verified **identical** on
   host and UEFI and therefore NOT the cause: the shim `atof` (`atof("4")`=4 on
   both), `"…".parse::<f64>()` (`Ok(4.0)` on UEFI), `cg_break`'s branch logic
   (branch 4 on both when observed non-invasively), the utterance structure, and
   the const-val table. The corruption only manifests in the *un-probed* control
   flow — classic memory corruption whose effect depends on heap/object layout,
   which differs at `long`=32.

### Update (additional experiments, all negative)

- **`long` width is NOT the cause.** Widened every `long`→`long long` (64-bit on
  all x86_64 ABIs) across the CG/MLPG/text/structure files (`cst_vc.{c,h}`,
  `cst_mlpg.{c,h}`, `cst_mlsa.{c,h}`, `cst_tokenstream.c`, `us_text.c`, …) and
  rebuilt: **no change** (num_samples 17840, peak 48). So this is not the LP64↔
  LLP64 `long` issue. Reverted.
- **No masked declarations:** compiling without `-Wno-implicit-function-declaration`
  / `-Wno-int-conversion` produces zero implicit-declaration or pointer-truncation
  warnings — nothing is being silently mis-typed.
- **Localized to MLPG:** with MLPG *disabled* (forced non-MLPG path), host=215 and
  UEFI=507 (both quiet, same ballpark); with MLPG *enabled*, host=21077 but
  UEFI=48. So MLPG is essential for loudness and is exactly where UEFI diverges —
  but it is fed a different frame sequence because **durations also differ**
  (17840 vs 19760), which traces back to the same Heisenbug-class relation/feature
  divergence during duration prediction. Disabling MLPG is therefore not a usable
  mitigation (host is quiet without it too).

**To finish the fix:** the defect is a deterministic, LLP64-specific corruption
of the HRG item/relation `n`/`p` linkage built during text analysis
(`src/hrg/cst_{item,relation,utterance}.c`, `src/synth/cst_ffeatures.c`,
`src/utils/cst_features.c`). Since host ASan can't see it, the practical next
step is to **attach gdb to QEMU** (`-s -S`, target `x86_64-unknown-uefi`) and
inspect the Syllable relation's `n` pointers right after utterance construction,
or audit those files for any `long`/`sizeof(long)`/pointer-through-`long` use in
the linkage. A clang **host** build is the known-good reference.

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
