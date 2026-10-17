use std::path::PathBuf;

fn main() {
    // Get the kernel binary path (set by cargo's artifact dependency feature)
    let kernel = PathBuf::from(std::env::var_os("CARGO_BIN_FILE_ZERO_zero").unwrap());

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

    // Pass disk image paths as env variables to the main crate
    println!("cargo:rustc-env=UEFI_PATH={}", uefi_path.display());
    println!("cargo:rustc-env=BIOS_PATH={}", bios_path.display());

    // Also copy disk images to a known location for easy access
    let target_dir = PathBuf::from(std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| {
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("target")
    }));
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let release_dir = if profile == "release" { "release" } else { "debug" };

    let dest_bios = target_dir.join("x86_64-unknown-none").join(release_dir).join("bios.img");
    let dest_uefi = target_dir.join("x86_64-unknown-none").join(release_dir).join("uefi.img");

    std::fs::create_dir_all(dest_bios.parent().unwrap()).ok();
    std::fs::copy(&bios_path, &dest_bios).ok();
    std::fs::copy(&uefi_path, &dest_uefi).ok();

    println!("cargo:rerun-if-changed=kernel/src/");
    println!("cargo:rerun-if-changed=kernel/Cargo.toml");
    println!("cargo:rerun-if-changed=build.rs");
}
