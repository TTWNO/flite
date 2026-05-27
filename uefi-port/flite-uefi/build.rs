fn main() {
    let dir = "/home/tait/Documents/flite/uefi-port";
    println!("cargo:rustc-link-search=native={dir}");
    println!("cargo:rustc-link-lib=static=flite_uefi");
    // Relink when the C archive is regenerated (build-uefi-c.sh), otherwise
    // cargo reuses a stale link and changes to the C side are silently ignored.
    println!("cargo:rerun-if-changed={dir}/libflite_uefi.a");
}
