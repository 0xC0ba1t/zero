use std::path::PathBuf;

fn main() {
    // Get the kernel binary path (set by cargo's artifact dependency feature)
    let kernel = PathBuf::from(std::env::var_os("CARGO_BIN_FILE_KERNEL_zero").unwrap());

    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());

    // Create BIOS disk image using DiskImageBuilder
    let bios_path = out_dir.join("bios.img");
    bootloader::DiskImageBuilder::new(kernel.clone())
        .create_bios_image(&bios_path)
        .expect("Failed to create BIOS disk image");

    // Create UEFI disk image
    let uefi_path = out_dir.join("uefi.img");
    bootloader::DiskImageBuilder::new(kernel)
        .create_uefi_image(&uefi_path)
        .expect("Failed to create UEFI disk image");

    // Pass disk image paths as env variables
    println!("cargo:rustc-env=UEFI_PATH={}", uefi_path.display());
    println!("cargo:rustc-env=BIOS_PATH={}", bios_path.display());

    println!("cargo:rerun-if-changed=kernel/src/");
    println!("cargo:rerun-if-changed=kernel/Cargo.toml");
    println!("cargo:rerun-if-changed=build.rs");
}
