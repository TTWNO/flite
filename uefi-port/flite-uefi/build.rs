fn main() {
    println!("cargo:rustc-link-search=native=/home/tait/Documents/flite/uefi-port");
    println!("cargo:rustc-link-lib=static=flite_uefi");
}
