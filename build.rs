use bootloader_boot_config::{BootConfig, LevelFilter};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

// ---- the persistent partition ------------------------------------------
//
// Every disk image gets a 100 MiB data partition at a fixed offset, after
// the bootable part. It holds the saved file system, settings and read-only
// assets (the Doom game data). The layout is shared with the kernel
// (`kernel/src/partition.rs`); keep both in step.
const MIB: u64 = 1024 * 1024;
const PART_OFFSET: u64 = 32 * MIB;
const PART_SIZE: u64 = 100 * MIB;
const SECTOR: u64 = 512;
const MAGIC: &[u8; 8] = b"ZEROPART";
const TABLE_LBA: u64 = 1;
const SETTINGS_LBA: u64 = 9;
const TREE_LBA: u64 = 64;
const TREE_SECTORS: u64 = 16384;
const ASSETS_LBA: u64 = TREE_LBA + TREE_SECTORS;

/// Files that go into the partition's read-only asset area.
fn assets() -> Vec<(&'static str, Vec<u8>)> {
    let mut list = Vec::new();
    let wad = Path::new("assets/doom1.wad");
    if wad.exists() {
        list.push(("doom1.wad", fs::read(wad).expect("could not read assets/doom1.wad")));
    }
    list
}

/// The first sectors of the partition: header and asset table (the second
/// value is where each asset's data goes, relative to the partition).
fn partition_head(assets: &[(&str, Vec<u8>)]) -> (Vec<u8>, Vec<(u64, usize)>) {
    let mut head = vec![0u8; (SETTINGS_LBA * SECTOR) as usize];
    head[..8].copy_from_slice(MAGIC);
    head[8..12].copy_from_slice(&1u32.to_le_bytes());
    head[16..24].copy_from_slice(&(PART_SIZE / SECTOR).to_le_bytes());
    head[24..28].copy_from_slice(&(TABLE_LBA as u32).to_le_bytes());
    head[28..32].copy_from_slice(&(SETTINGS_LBA as u32).to_le_bytes());
    head[32..36].copy_from_slice(&(TREE_LBA as u32).to_le_bytes());
    head[36..40].copy_from_slice(&(TREE_SECTORS as u32).to_le_bytes());
    let mut placed = Vec::new();
    let mut lba = ASSETS_LBA;
    for (i, (name, data)) in assets.iter().enumerate() {
        let entry = (TABLE_LBA * SECTOR) as usize + i * 64;
        head[entry..entry + name.len()].copy_from_slice(name.as_bytes());
        head[entry + 48..entry + 56].copy_from_slice(&lba.to_le_bytes());
        head[entry + 56..entry + 64].copy_from_slice(&(data.len() as u64).to_le_bytes());
        placed.push((lba, i));
        lba += (data.len() as u64 + SECTOR - 1) / SECTOR;
        lba = (lba + 7) & !7;
    }
    (head, placed)
}

/// Pads a bootable image to the partition offset and appends a fresh partition.
fn add_partition(image: &Path) {
    let mut boot = fs::read(image).expect("could not read the disk image");
    assert!((boot.len() as u64) < PART_OFFSET, "the boot image outgrew the space reserved for it");
    boot.resize(PART_OFFSET as usize, 0);
    let assets = assets();
    let (head, placed) = partition_head(&assets);

    let mut out = File::create(image).expect("could not rewrite the disk image");
    out.write_all(&boot).unwrap();
    out.write_all(&head).unwrap();
    for (lba, index) in placed {
        out.seek(SeekFrom::Start(PART_OFFSET + lba * SECTOR)).unwrap();
        out.write_all(&assets[index].1).unwrap();
    }
    out.set_len(PART_OFFSET + PART_SIZE).unwrap();
}

/// Puts a freshly built image at `dest`. If `dest` already holds a partition
/// with the same layout, only the bootable part is replaced, so saved files
/// and settings survive a rebuild.
fn install(built: &Path, dest: &Path) {
    let has_partition = |path: &Path| -> bool {
        let Ok(mut file) = File::open(path) else { return false };
        let Ok(meta) = file.metadata() else { return false };
        if meta.len() != PART_OFFSET + PART_SIZE {
            return false;
        }
        let mut magic = [0u8; 8];
        file.seek(SeekFrom::Start(PART_OFFSET)).is_ok()
            && file.read_exact(&mut magic).is_ok()
            && &magic == MAGIC
    };
    if has_partition(dest) {
        let mut boot = vec![0u8; PART_OFFSET as usize];
        File::open(built).unwrap().read_exact(&mut boot).unwrap();
        let mut target = OpenOptions::new().write(true).open(dest).expect("could not update the disk image");
        target.write_all(&boot).unwrap();
    } else {
        fs::copy(built, dest).ok();
    }
}

fn main() {
    // Get the kernel binary path (set by cargo's artifact dependency feature)
    let kernel = PathBuf::from(std::env::var_os("CARGO_BIN_FILE_ZERO_zero").unwrap());

    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());

    // Ask for a big framebuffer (UEFI honors this; the BIOS loader stops at
    // 1280x720) and keep the bootloader's own log off the screen so boot goes
    // straight to the kernel.
    let mut config = BootConfig::default();
    config.frame_buffer.minimum_framebuffer_width = Some(1920);
    config.frame_buffer.minimum_framebuffer_height = Some(1080);
    config.frame_buffer_logging = false;
    config.log_level = LevelFilter::Info;

    // Create BIOS disk image using DiskImageBuilder
    let bios_path = out_dir.join("bios.img");
    bootloader::DiskImageBuilder::new(kernel.clone())
        .set_boot_config(&config)
        .create_bios_image(&bios_path)
        .expect("Failed to create BIOS disk image");

    // Create UEFI disk image
    let uefi_path = out_dir.join("uefi.img");
    bootloader::DiskImageBuilder::new(kernel)
        .set_boot_config(&config)
        .create_uefi_image(&uefi_path)
        .expect("Failed to create UEFI disk image");

    // Both images get the persistent partition.
    add_partition(&bios_path);
    add_partition(&uefi_path);

    // Pass disk image paths as env variables to the main crate
    println!("cargo:rustc-env=UEFI_PATH={}", uefi_path.display());
    println!("cargo:rustc-env=BIOS_PATH={}", bios_path.display());

    // Also copy disk images to a known location for easy access
    let target_dir = PathBuf::from(std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| {
        std::env::var("CARGO_MANIFEST_DIR").unwrap() + "/target"
    }));
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let release_dir = if profile == "release" { "release" } else { "debug" };

    let dest_bios = target_dir.join("x86_64-zero").join(release_dir).join("bios.img");
    let dest_uefi = target_dir.join("x86_64-zero").join(release_dir).join("uefi.img");

    std::fs::create_dir_all(dest_bios.parent().unwrap()).ok();
    install(&bios_path, &dest_bios);
    install(&uefi_path, &dest_uefi);

    println!("cargo:rerun-if-changed=kernel/src/");
    println!("cargo:rerun-if-changed=kernel/build.rs");
    println!("cargo:rerun-if-changed=kernel/Cargo.toml");
    println!("cargo:rerun-if-changed=assets");
    println!("cargo:rerun-if-changed=build.rs");
}
