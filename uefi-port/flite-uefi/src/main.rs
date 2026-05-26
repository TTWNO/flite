#![feature(c_variadic)]
//! flite-uefi: a UEFI application that links the cross-compiled flite C archive
//! (`libflite_uefi.a`) and provides the C runtime/libc surface it needs.
//!
//! Checkpoint C: stub `main`, the goal is a clean link producing a PE32+ .efi.
//! Real synthesis lands in the next checkpoint.

mod cprintf;
mod shim;

fn main() {
    // Stub. Real synthesis comes in the next checkpoint.
    // Reference the shim/cprintf modules so the linker keeps the symbols that
    // the flite C archive resolves against (belt-and-suspenders; the static
    // archive normally forces them to be retained).
    keep_alive();
}

/// Touch a few shim symbols so they are not dead-stripped before the C archive
/// is linked. The `#[used]` static below is the primary guard.
fn keep_alive() {
    unsafe {
        core::ptr::read_volatile(&raw const shim::stdout);
    }
}

#[used]
static KEEP: unsafe extern "C" fn(usize) -> *mut core::ffi::c_void = shim::malloc;
