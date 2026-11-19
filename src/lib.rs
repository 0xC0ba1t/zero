// The zero-os crate is a build wrapper that creates the bootable disk image
// for the zero kernel. The actual kernel lives in the `kernel` subdirectory.

#![no_std]

use core::option::Option;

// Re-export the disk image paths that are set by build.rs
pub const BIOS_PATH: Option<&str> = option_env!("BIOS_PATH");
pub const UEFI_PATH: Option<&str> = option_env!("UEFI_PATH");
