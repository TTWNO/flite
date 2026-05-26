#![feature(c_variadic)]
//! flite-uefi: a UEFI application that links the cross-compiled flite C archive
//! (`libflite_uefi.a`) and provides the C runtime/libc surface it needs.
//!
//! It registers the compiled-in `cmu_us_slt` clustergen voice, synthesizes a
//! line of text to PCM, and reports the result on the UEFI console.

mod cprintf;
mod shim;

use core::ffi::{c_char, c_int, c_void};

/// Mirror of flite's `cst_wave` (include/cst_wave.h). `#[repr(C)]` reproduces
/// the C layout (incl. the padding before `samples`).
#[repr(C)]
pub struct CstWave {
    pub type_: *const c_char,
    pub sample_rate: c_int,
    pub num_samples: c_int,
    pub num_channels: c_int,
    pub samples: *mut i16,
}

extern "C" {
    fn flite_init() -> c_int;
    fn register_cmu_us_slt(voxdir: *const c_char) -> *mut c_void;
    fn flite_text_to_wave(text: *const c_char, voice: *mut c_void) -> *mut CstWave;
}

const TEXT: &[u8] = b"hello world\0";

fn main() {
    println!("FLITE-UEFI: start");
    unsafe {
        flite_init();
        println!("FLITE-UEFI: flite_init done");
        let voice = register_cmu_us_slt(core::ptr::null());
        println!("FLITE-UEFI: register_cmu_us_slt done ({:p})", voice);
        if voice.is_null() {
            println!("FAIL: register_cmu_us_slt returned NULL");
            halt();
        }
        println!("FLITE-UEFI: calling flite_text_to_wave");
        let w = flite_text_to_wave(TEXT.as_ptr() as *const c_char, voice);
        println!("FLITE-UEFI: flite_text_to_wave returned ({:p})", w);
        if w.is_null() {
            println!("FAIL: flite_text_to_wave returned NULL");
            halt();
        }
        let w = &*w;
        println!(
            "FLITE-UEFI: num_samples={} sample_rate={} num_channels={}",
            w.num_samples, w.sample_rate, w.num_channels
        );
        if w.num_samples > 0 {
            println!("FLITE-UEFI: SYNTHESIS OK");
        } else {
            println!("FLITE-UEFI: SYNTHESIS PRODUCED NO SAMPLES");
        }
    }
    halt();
}

/// Spin forever so the single run's console output stays put (QEMU is killed by
/// an external timeout). Avoids the boot manager re-launching BOOTX64.EFI.
fn halt() -> ! {
    loop {
        core::hint::spin_loop();
    }
}
