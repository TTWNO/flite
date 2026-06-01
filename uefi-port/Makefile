# Makefile for the flite UEFI/freestanding port.
#
# Replaces the loose build-*.sh scripts with named targets. Run from this
# directory (uefi-port/) or from anywhere with `make -C uefi-port <target>`.
#
# Common targets (see `make help`):
#   make            # build both C archives + the synth EFI example
#   make coff       # cross-compile flite C -> COFF archive (libflite_uefi.a)
#   make elf        # cross-compile flite C -> ELF  archive (libflite_elf.a)
#   make synth      # build the synth.efi demo (UEFI target)
#   make qemu       # boot synth.efi under QEMU + OVMF
#   make native     # build flite natively (host static libs)
#   make native-test# host sanity check of the C library + voice
#   make clean      # remove all build intermediates and run artifacts
#   make distclean  # clean + remove the generated archives + native build

# --- Layout -----------------------------------------------------------------
# Resolve directories from the Makefile's own location so the targets work
# regardless of the caller's $PWD.
MAKEFILE_DIR := $(abspath $(dir $(lastword $(MAKEFILE_LIST))))
REPO_ROOT    := $(abspath $(MAKEFILE_DIR)/..)

OBJ_DIR      := $(MAKEFILE_DIR)/obj
ELF_OBJ_DIR  := $(MAKEFILE_DIR)/obj-elf
COMPILE_COMMANDS := $(MAKEFILE_DIR)/compile_commands.json

COFF_ARCHIVE := $(MAKEFILE_DIR)/libflite_uefi.a
ELF_ARCHIVE  := $(MAKEFILE_DIR)/libflite_elf.a

# --- Tools ------------------------------------------------------------------
CLANG   ?= clang
COFF_AR ?= llvm-ar           # COFF archives: use the LLVM archiver
ELF_AR  ?= ar                # ELF archives: GNU ar is fine
CARGO   ?= cargo
NIGHTLY ?= +nightly
PYTHON  ?= python3

UEFI_TARGET := x86_64-unknown-uefi

# OVMF firmware paths (Arch defaults; override for your distro, e.g. Debian:
#   make qemu OVMF_CODE=/usr/share/OVMF/OVMF_CODE.fd \
#             OVMF_VARS=/usr/share/OVMF/OVMF_VARS.fd)
OVMF_CODE ?= /usr/share/edk2/x64/OVMF_CODE.4m.fd
OVMF_VARS ?= /usr/share/edk2/x64/OVMF_VARS.4m.fd
QEMU_TIMEOUT ?= 30

# Native host build outputs (consumed by native-test).
NATIVE_LIBDIR := $(REPO_ROOT)/build/x86_64-linux-gnu/lib

# --- Compiler flags ---------------------------------------------------------
# Common flags shared by the COFF (UEFI) and ELF (freestanding) cross-builds.
# See build-uefi-c.sh / build-uefi-c-elf.sh history for the rationale behind
# each one; the per-target differences are spelled out below.
COMMON_FLAGS := \
	-fno-builtin \
	-ffreestanding \
	-mno-red-zone \
	-DDIE_ON_ERROR \
	-DCST_AUDIO_NONE \
	-DCST_NO_SOCKETS \
	-I$(MAKEFILE_DIR)/cinclude \
	-I$(REPO_ROOT)/include \
	-I$(REPO_ROOT)/lang/usenglish \
	-I$(REPO_ROOT)/lang/cmulex \
	-Wno-implicit-function-declaration \
	-Wno-int-conversion

# COFF / UEFI: clang's UEFI triple emits COFF and uses 16-bit wchar_t.
COFF_FLAGS := --target=$(UEFI_TARGET) -fshort-wchar $(COMMON_FLAGS)

# ELF / freestanding (GRUB module path): emits ELF objects.
#   -mcmodel=large     R_X86_64_64 relocations GRUB's loader accepts.
#   -fno-stack-protector  match GRUB (no __stack_chk_fail in its kernel).
#   -fno-pic           GRUB resolves relocations itself; no GOT/PLT.
# Default 32-bit wchar_t (no -fshort-wchar) is fine here.
ELF_FLAGS := --target=x86_64-pc-linux-gnu -mcmodel=large \
	-fno-stack-protector -fno-pic $(COMMON_FLAGS)

# --- Source / object lists --------------------------------------------------
# The flite C file set lives in compile_commands.json (absolute paths under
# the repo root). Derive object names by mangling the repo-relative path:
# src/audio/audio.c -> src_audio_audio.o (so flat obj dirs don't collide).
SRCS := $(shell $(PYTHON) -c "import json; print('\n'.join(e['file'] for e in json.load(open('$(COMPILE_COMMANDS)'))))")

# src -> mangled stem (repo-relative path, '/' -> '_', drop .c)
mangle = $(patsubst %.c,%,$(subst /,_,$(patsubst $(REPO_ROOT)/%,%,$(1))))

COFF_OBJS := $(foreach s,$(SRCS),$(OBJ_DIR)/$(call mangle,$(s)).o)
ELF_OBJS  := $(foreach s,$(SRCS),$(ELF_OBJ_DIR)/$(call mangle,$(s)).o)

# --- Default ----------------------------------------------------------------
.DEFAULT_GOAL := all

.PHONY: all archives coff elf synth consumer-demo qemu run \
        native native-test clean clean-archives distclean help

all: archives synth

archives: coff elf

coff: $(COFF_ARCHIVE)
elf:  $(ELF_ARCHIVE)

# --- COFF (UEFI) C build ----------------------------------------------------
$(OBJ_DIR) $(ELF_OBJ_DIR):
	mkdir -p $@

# Generate a per-source compile rule for each archive. Explicit rules (rather
# than a pattern rule) are needed because the mangled object name doesn't share
# a directory layout with the source tree.
define COFF_RULE
$(OBJ_DIR)/$(call mangle,$(1)).o: $(1) | $(OBJ_DIR)
	$$(CLANG) $$(COFF_FLAGS) -c $$< -o $$@
endef
$(foreach s,$(SRCS),$(eval $(call COFF_RULE,$(s))))

$(COFF_ARCHIVE): $(COFF_OBJS)
	rm -f $@
	$(COFF_AR) rcs $@ $(COFF_OBJS)
	@echo "Archived $$($(COFF_AR) t $@ | wc -l) objects into $@"

# --- ELF (freestanding) C build ---------------------------------------------
define ELF_RULE
$(ELF_OBJ_DIR)/$(call mangle,$(1)).o: $(1) | $(ELF_OBJ_DIR)
	$$(CLANG) $$(ELF_FLAGS) -c $$< -o $$@
endef
$(foreach s,$(SRCS),$(eval $(call ELF_RULE,$(s))))

$(ELF_ARCHIVE): $(ELF_OBJS)
	rm -f $@
	$(ELF_AR) rcs $@ $(ELF_OBJS)
	@echo "Archived $$($(ELF_AR) t $@ | wc -l) objects into $@"

# --- Rust builds ------------------------------------------------------------
# The synth example links the COFF archive; consumer-demo is a separate crate.
synth: $(COFF_ARCHIVE)
	$(CARGO) $(NIGHTLY) build --manifest-path $(MAKEFILE_DIR)/flite-uefi/Cargo.toml \
		--example synth --target $(UEFI_TARGET)

consumer-demo: $(COFF_ARCHIVE)
	$(CARGO) $(NIGHTLY) build --manifest-path $(MAKEFILE_DIR)/consumer-demo/Cargo.toml \
		--target $(UEFI_TARGET)

# --- QEMU run ---------------------------------------------------------------
EFI_APP := $(MAKEFILE_DIR)/flite-uefi/target/$(UEFI_TARGET)/debug/examples/synth.efi
ESP_DIR := $(MAKEFILE_DIR)/esp
QEMU_VARS := /tmp/flite_OVMF_VARS.fd

run: qemu
qemu: synth
	rm -rf $(ESP_DIR)
	mkdir -p $(ESP_DIR)/EFI/BOOT
	cp $(EFI_APP) $(ESP_DIR)/EFI/BOOT/BOOTX64.EFI
	cp $(OVMF_VARS) $(QEMU_VARS)
	@echo "=== booting QEMU (timeout $(QEMU_TIMEOUT)s) ==="
	timeout $(QEMU_TIMEOUT) qemu-system-x86_64 \
		-machine q35 -m 256 -nographic \
		-drive if=pflash,format=raw,unit=0,readonly=on,file=$(OVMF_CODE) \
		-drive if=pflash,format=raw,unit=1,file=$(QEMU_VARS) \
		-drive format=raw,file=fat:rw:$(ESP_DIR) \
		-net none 2>&1 || true
	@echo
	@echo "=== QEMU exited ==="

# --- Native host build / test -----------------------------------------------
# Build flite natively in the repo root (produces the host static libs the
# sanity test links against). Idempotent-ish: re-running make is cheap.
native:
	cd $(REPO_ROOT) && ./configure --with-audio=none CFLAGS="-g -O2 -fPIC -DDIE_ON_ERROR"
	$(MAKE) -C $(REPO_ROOT)
	$(MAKE) -C $(REPO_ROOT)/lang/cmu_us_slt

# CP-A: sanity-check flite C + compiled-in cmu_us_slt voice on the host.
native-test:
	gcc -I$(REPO_ROOT)/include $(MAKEFILE_DIR)/native-test.c \
		$(NATIVE_LIBDIR)/libflite_cmu_us_slt.a \
		$(NATIVE_LIBDIR)/libflite_usenglish.a \
		$(NATIVE_LIBDIR)/libflite_cmulex.a \
		$(NATIVE_LIBDIR)/libflite.a \
		-lm -o /tmp/flite-native-test
	/tmp/flite-native-test

# --- Cleaning ---------------------------------------------------------------
# clean: wipe everything regenerable EXCEPT the archives (one of which,
# libflite_uefi.a, is checked in — so removing it here would needlessly dirty
# the working tree). Use distclean to nuke the archives too.
clean:
	rm -rf $(OBJ_DIR) $(ELF_OBJ_DIR) $(ESP_DIR)
	rm -f $(QEMU_VARS) /tmp/flite-native-test \
	      /tmp/uefi_build_err.txt /tmp/elf_build_err.txt
	rm -f $(MAKEFILE_DIR)/*.log $(MAKEFILE_DIR)/flite-uefi/*.log
	-$(CARGO) clean --manifest-path $(MAKEFILE_DIR)/flite-uefi/Cargo.toml
	-$(CARGO) clean --manifest-path $(MAKEFILE_DIR)/consumer-demo/Cargo.toml
	@echo "clean: removed obj dirs, esp/, cargo target dirs, logs, tmp files"

clean-archives:
	rm -f $(COFF_ARCHIVE) $(ELF_ARCHIVE)

distclean: clean clean-archives
	-$(MAKE) -C $(REPO_ROOT) clean
	@echo "distclean: also removed archives and native build"

# --- Help -------------------------------------------------------------------
help:
	@echo "flite UEFI port — make targets:"
	@echo "  all (default)  build both C archives + the synth EFI example"
	@echo "  archives       build coff + elf archives"
	@echo "  coff           cross-compile flite C -> libflite_uefi.a (COFF, UEFI)"
	@echo "  elf            cross-compile flite C -> libflite_elf.a  (ELF, freestanding)"
	@echo "  synth          build the synth.efi demo (needs nightly + UEFI target)"
	@echo "  consumer-demo  build the consumer-demo crate"
	@echo "  qemu / run     boot synth.efi under QEMU + OVMF"
	@echo "  native         build flite natively (host static libs)"
	@echo "  native-test    host sanity check of the C library + voice"
	@echo "  clean          remove build intermediates, target dirs, esp/, logs, tmp"
	@echo "  clean-archives remove libflite_uefi.a and libflite_elf.a"
	@echo "  distclean      clean + clean-archives + native 'make clean'"
	@echo ""
	@echo "Overridable vars: CLANG CARGO NIGHTLY PYTHON OVMF_CODE OVMF_VARS QEMU_TIMEOUT"
