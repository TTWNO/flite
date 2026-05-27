#![feature(c_variadic)]
//! flite-uefi: a UEFI application that links the cross-compiled flite C archive
//! (`libflite_uefi.a`) and provides the C runtime/libc surface it needs.
//!
//! Registers the compiled-in `cmu_us_slt` clustergen voice, synthesizes a line
//! of text to PCM, reports on the UEFI console, and writes a WAV to the ESP.

mod cprintf;
mod shim;
mod wav;

use core::ffi::{c_char, c_int, c_void};

/// Mirror of flite's `cst_wave` (include/cst_wave.h).
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
        let voice = register_cmu_us_slt(core::ptr::null());
        if voice.is_null() {
            println!("FAIL: register_cmu_us_slt returned NULL");
            return;
        }
        let w = flite_text_to_wave(TEXT.as_ptr() as *const c_char, voice);
        if w.is_null() {
            println!("FAIL: flite_text_to_wave returned NULL");
            return;
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

        let buf = wav::wave_to_wav(w);
        println!("FLITE-UEFI: wav bytes={}", buf.len());
        write_wav(&buf);
    }
    // Return to the firmware boot manager (EFI_SUCCESS); do not spin (that would
    // stall/lock on real hardware after the work completes).
    println!("FLITE-UEFI: done — returning control to firmware");
}

/// Write the WAV to the ESP and read it back to confirm. Probes a few path
/// spellings because UEFI std path semantics are not well documented.
fn write_wav(buf: &[u8]) {
    for path in ["hello.wav", "\\hello.wav", "/hello.wav"] {
        match std::fs::write(path, buf) {
            Ok(()) => {
                println!("FLITE-UEFI: wrote {:?} ({} bytes)", path, buf.len());
                match std::fs::read(path) {
                    Ok(rb) => {
                        let magic = &rb[..rb.len().min(4)];
                        println!(
                            "FLITE-UEFI: readback {:?} ok, {} bytes, magic={:?}",
                            path, rb.len(), core::str::from_utf8(magic).unwrap_or("?")
                        );
                        println!("FLITE-UEFI: WAV WRITE OK");
                        return;
                    }
                    Err(e) => println!("FLITE-UEFI: readback {:?} failed: {}", path, e),
                }
            }
            Err(e) => println!("FLITE-UEFI: write {:?} failed: {}", path, e),
        }
    }
    println!("FLITE-UEFI: WAV WRITE FAILED (all paths)");
}
