# Flite on x86_64-unknown-uefi via C cross-compile (supersedes the c2rust plan)

> **For agentic workers:** Use superpowers:subagent-driven-development or executing-plans. Steps use `- [ ]`.

**Goal:** Run flite TTS on `x86_64-unknown-uefi` under QEMU+OVMF: synthesize text → PCM, write a `.wav` to the EFI System Partition.

**Why this supersedes the c2rust plan:** c2rust hangs (unbounded memory) on flite's synthesis-core files — see spec "PIVOT" section + memory `c2rust-hangs-on-flite`. Instead we cross-compile flite's C for UEFI and back its runtime with Rust.

**Architecture:** `clang --target=x86_64-unknown-uefi -ffreestanding -mno-red-zone -fshort-wchar` compiles flite's C set (core + usenglish + cmulex + compiled-in `cmu_us_slt` CG voice) to COFF objects, using minimal shim headers in `uefi-port/cinclude/`. A Rust `[[bin]]` `std` EFI app links those objects (via the `cc` crate in build.rs) and provides flite's `extern "C"` libc symbols: alloc→`std`, math→`libm`, mem/str→`core`, stdio/file/socket→stubs, `cst_errmsg`→UEFI console. The app calls `flite_init` + `register_cmu_us_slt` + `flite_text_to_wave`, then writes a WAV to the ESP.

**Verified:** clang `x86_64-unknown-uefi` triple → COFF, predefines `__UEFI__` not `_WIN32`. `cmu_us_slt` compiles in as const data; `register_cmu_us_slt(const char*)→cst_voice*`; `flite_text_to_wave(const char *text, cst_voice *voice)`. QEMU 11 + OVMF at `/usr/share/edk2/x64/`.

---

## File Structure (under `uefi-port/`)
- `compile_commands.json` — file list (from CP1, done).
- `cinclude/` — minimal freestanding shim headers (`stdio.h`, `stdlib.h`, `string.h`, `math.h`, `ctype.h`, `unistd.h`, …) declaring only what flite uses.
- `flite-uefi/` — Rust `[[bin]]` EFI app: `build.rs` (compiles flite C via `cc`), `src/shim.rs` (libc symbols), `src/wav.rs` (cst_wave→WAV bytes), `src/main.rs` (synthesis + ESP write).
- `run-qemu.sh` — assemble ESP, boot QEMU+OVMF.

---

## Checkpoint A: Native sanity — flite C + cmu_us_slt synthesizes
**Goal:** Prove the C library + compiled-in CG voice produce audio *before* any UEFI work, so later failures are isolated to UEFI/linking.

- [ ] **A1** Write `uefi-port/native-test.c`: `flite_init(); v=register_cmu_us_slt(NULL); w=flite_text_to_wave("hello world", v); printf("num_samples=%d sr=%d\n", w->num_samples, w->sample_rate); assert(w->num_samples>0);`
- [ ] **A2** Compile/link natively against the already-built flite libs + voice (`build/x86_64-linux-gnu/...` from CP1; link `libflite`, `libflite_cmu_us_slt`, `libflite_usenglish`, `libflite_cmulex` or the combined lib). Use normal gcc, normal libc.
- [ ] **A3** Run it: expect `num_samples=<positive>`. Optionally write/inspect a wav.
- [ ] **A4** Commit `native-test.c` + a short `native-test.sh`.

**Gate:** native binary prints positive `num_samples`. (If not, the C/voice build is wrong — fix before UEFI.)

## Checkpoint B: Cross-compile flite C → COFF for UEFI (shim headers)
**Goal:** Every file in `compile_commands.json` compiles to a COFF object with the UEFI clang triple.

- [ ] **B1** Create `uefi-port/cinclude/` with minimal headers. Drive iteratively by clang errors. Needed (from the file-set scan): `stdio.h` (FILE typedef + fopen/fclose/fread/fwrite/fseek/ftell/fgetc/feof/fprintf/printf/snprintf/vsnprintf/vfprintf decls), `stdlib.h` (malloc/calloc/realloc/free/abort/exit/atoi/atof/strtod/qsort), `string.h` (mem*/str*), `math.h` (exp/log/log10/sqrt/sin/cos/tan/atan/atan2/pow/floor/ceil/fabs/fmod/frexp/ldexp), `ctype.h` (toupper/tolower/isspace/isalpha/isdigit + macros), `unistd.h` (minimal). Use clang's freestanding stddef/stdint/stdarg/limits/float.
- [ ] **B2** Compile each entry: `clang --target=x86_64-unknown-uefi -ffreestanding -mno-red-zone -fshort-wchar -DDIE_ON_ERROR -Iuefi-port/cinclude -Iinclude -Ilang/usenglish -Ilang/cmulex <file> -c -o <obj>`. Script it over `compile_commands.json`. Fix headers until all compile. Exclude any file whose `#ifdef` pulls in windows/socket (shouldn't trigger under `__UEFI__`, but verify).
- [ ] **B3** Archive objects into `libflite_uefi.a` (llvm-ar). Confirm `llvm-objdump -f` shows `coff-x86-64`.
- [ ] **B4** Dump the undefined-symbol surface: `llvm-nm libflite_uefi.a | grep ' U '` → the list the Rust shim must provide. Save it.
- [ ] **B5** Commit `cinclude/`, the build script, the symbol list.

**Gate:** `libflite_uefi.a` built, all-COFF; undefined-symbol list captured.

## Checkpoint C: libc_shim (Rust) — provide flite's C runtime
**Goal:** Implement every symbol from B4.

- [ ] **C1** `flite-uefi/src/shim.rs`: `#[no_mangle] extern "C"` for alloc (malloc/calloc/realloc/free over `std::alloc`, self-describing size header), mem*/str* (over `core`), math (over `libm`), ctype, atoi/atof/strtod, qsort. stdio/file → stubs (fopen→null, etc.; not exercised by const-voice path). `printf`/`fprintf`/`vfprintf`/`cst_errmsg` → format to a buffer and write to UEFI console (`vsnprintf`-style). `abort`/`exit`→`panic!`.
- [ ] **C2** Build `flite-uefi` for the target (with a stub `main`) so the linker resolves flite ↔ shim. Iterate by undefined-symbol/linker error until it links.

**Gate:** `cargo build --target x86_64-unknown-uefi` links flite objects + shim with no undefined symbols (stub main).

## Checkpoint D: Synthesize in the EFI app + run in QEMU
- [ ] **D1** `src/main.rs`: declare the flite API extern; `flite_init()`, `register_cmu_us_slt(null)`, `flite_text_to_wave("hello world", v)`, read `cst_wave` (mirror layout from `include/cst_wave.h`), `println!("num_samples={}", n)`.
- [ ] **D2** `run-qemu.sh`: copy `*.efi`→`ESP/EFI/BOOT/BOOTX64.EFI`, boot `qemu-system-x86_64 -nographic` with OVMF_CODE/VARS pflash + `-drive format=raw,file=fat:rw:$ESP`.
- [ ] **D3** Run; capture `num_samples=<positive>` from the console.

**Gate:** `qemu-run.log` shows positive `num_samples` from inside the UEFI app.

## Checkpoint E: Write WAV to the ESP + verify
- [ ] **E1** `src/wav.rs`: build a 16-bit PCM WAV `Vec<u8>` from `cst_wave` (sample_rate, num_samples, num_channels, samples:*mut i16).
- [ ] **E2** Write to ESP (std `fs::write` if the uefi target supports it, else UEFI Simple File System protocol; probe with a 1-byte file first). Use a fixed ESP dir in `run-qemu.sh` so the host can read it back.
- [ ] **E3** Recover: `xxd hello.wav | head` shows `RIFF`/`WAVE`; `file hello.wav` → WAVE audio with expected sample count.

**Gate:** valid `hello.wav` produced by the UEFI app and recovered on host. **Success bar.**

---
## Notes
- Iterate shim headers (B) and shim symbols (C) by compiler/linker error — don't pre-guess exhaustively.
- A stub being *hit at runtime* (fopen/socket) is a finding — the const-voice path shouldn't need them.
- Verify each gate with real command output (@superpowers:verification-before-completion).
