# Disclosure: Use of Generative AI in the UEFI Port

This disclosure covers **only** the `uefi-port/` portion of this repository (the
experimental build that runs flite as a UEFI application on
`x86_64-unknown-uefi`). It does not apply to flite itself or any other part of
the project.

I'm writing this in the interest of transparency: this part of the project was
built almost entirely by a generative-AI coding agent, and I want anyone reading
or relying on it to understand exactly what that means and where the human
judgment was — and wasn't — applied.

## Tool used

- **Claude Code** (Anthropic's CLI coding agent), model **Claude Opus 4.7**,
  in a single working session (May 2026).

## What I (the human) actually did

My involvement was direction and decision-making, not writing code. Concretely,
I:

- Set the original goal: "use c2rust to compile flite for the
  `x86_64-unknown-uefi` Rust target," and gave permission to reimplement basic
  runtime functionality (e.g. allocation) as needed.
- Answered a handful of design questions the agent posed (mostly multiple
  choice): that the success bar was *actually running under QEMU/OVMF* (not just
  compiling), to use a clustergen voice, to lean on the UEFI target's `std` plus
  the `libm` crate for the runtime, how to handle flite's `setjmp`-based error
  path, and where to put the output.
- Made the one substantive course correction: when c2rust turned out to hang on
  flite's core, I told the agent to abandon that route and instead cross-compile
  flite's C with a freestanding compiler and supply the allocator/runtime from
  Rust. That architectural pivot was my call; the agent had also independently
  diagnosed the c2rust failure and proposed options.
- Gave the final packaging instructions: keep this in my own fork (flite is an
  academic project that does not want contributions like this), clean up, add
  build instructions to the README, and open a pull request for me to review.

That is the full extent of my hands-on contribution. **I did not write any of
the code, scripts, headers, or documentation in this directory.** My total
prompting was on the order of a dozen short, high-level messages over one
session; the agent carried out the multi-step work autonomously between them.

## What the AI contributed

Essentially everything in `uefi-port/`, including:

- The design and implementation plan (see `docs/superpowers/`).
- The investigation and root-cause analysis of why c2rust could not be used.
- The C-for-UEFI build path: the minimal freestanding shim headers
  (`cinclude/`), the cross-compile script (`build-uefi-c.sh`), and the
  determination of the libc symbol surface.
- The Rust runtime/application (`flite-uefi/`): the libc shim (`shim.rs`), the
  `printf` implementation (`cprintf.rs`), the WAV serializer (`wav.rs`), and the
  application entry point (`main.rs`).
- All debugging — notably root-causing a stack-overflow crash caused by a
  recursive `memcpy`/`memset` definition, and getting the app to boot under
  QEMU + OVMF.
- The READMEs, this disclosure, and the pull request description.

## Auditing status

Be aware of what has and has not been checked:

- **Functional verification is real and reproducible.** The app builds, boots
  under QEMU + OVMF, synthesizes speech, and writes a valid WAV; the steps to
  reproduce this are in `uefi-port/README.md`.
- **Automated/AI review has been done.** A separate AI review pass examined the
  hand-written Rust for correctness and safety and flagged two latent bugs
  (integer overflow in `atoi`, an incorrect `printf` width assumption), which
  were then fixed.
- **Human audit is still pending.** As of this writing I have *not* personally
  reviewed the code line by line; I intend to read it over before treating it as
  anything more than a proof of concept. Until I do, please treat this code as
  AI-authored and only AI-reviewed.

If you are evaluating or building on this, weight it accordingly: it is an
experiment that demonstrably works, produced by an AI agent under light human
direction, and not yet vetted by a human to the standard I would apply to code I
wrote myself.

— Tait Hoyem
