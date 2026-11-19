//! Builds the Doom engine (the C sources in `third_party/doomgeneric`) into a
//! static library that the kernel links.
//!
//! The kernel itself is plain Rust. Doom is C, and it needs a C compiler that
//! can target x86_64 with the kernel's restrictions (no red zone, no SSE).
//! `zig cc` is used for that: it is one self-contained download, and it ships
//! the libc *headers* the C code includes (the libc itself is Rust code in
//! `src/doom/libc.rs`).
//!
//! Where to find Zig, in order: the `ZIG` environment variable (path to
//! `zig` / `zig.exe`), `zig` on the PATH, or any `zig*` folder inside `~/zig-tool`.
//! Without Zig the kernel builds fine, just without Doom (`doom` then says
//! so). Set `ZERO_NO_DOOM=1` to skip it on purpose.

use std::path::{Path, PathBuf};
use std::process::Command;

fn find_zig() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("ZIG") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }
    // is there a `zig` on the PATH?
    let probe = if cfg!(windows) { "zig.exe" } else { "zig" };
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            let candidate = dir.join(probe);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    // a folder next to the home directory, where the zig zip was unpacked
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    let tool = PathBuf::from(home).join("zig-tool");
    for entry in std::fs::read_dir(tool).ok()?.flatten() {
        let candidate = entry.path().join(probe);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Compiles one C file; returns the object file.
fn compile(zig: &Path, source: &Path, out_dir: &Path, include: &Path) -> Result<PathBuf, String> {
    let object = out_dir.join(format!("{}.o", source.file_stem().unwrap().to_string_lossy()));
    let output = Command::new(zig)
        .arg("cc")
        // only the headers of musl are used; its functions come from src/doom/libc.rs
        .args(["-target", "x86_64-linux-musl"])
        .arg("-c")
        .args(["-std=gnu11", "-O2", "-w"])
        // the kernel runs without SSE, without a red zone, and without relocation
        .args(["-mno-red-zone", "-mno-sse", "-mno-sse2", "-mno-mmx", "-mno-avx", "-msoft-float"])
        .args(["-mcmodel=small", "-fno-pic", "-fno-pie", "-fno-stack-protector"])
        .args(["-fno-asynchronous-unwind-tables", "-fno-unwind-tables", "-fno-sanitize=undefined"])
        .args(["-DNORMALUNIX", "-D_DEFAULT_SOURCE", "-DDOOMGENERIC_RESX=640", "-DDOOMGENERIC_RESY=400"])
        .arg("-I")
        .arg(include)
        .arg(source)
        .arg("-o")
        .arg(&object)
        .output()
        .map_err(|e| format!("could not run zig: {}", e))?;
    if output.status.success() {
        Ok(object)
    } else {
        Err(format!("{} failed:\n{}", source.display(), String::from_utf8_lossy(&output.stderr)))
    }
}

fn main() {
    println!("cargo::rustc-check-cfg=cfg(has_doom)");
    // Link the kernel at 1 GiB instead of the linker's default 2 MiB. The UEFI bootloader
    // identity-maps a few frames of ordinary memory, and a kernel at the default address
    // can cover one of them ("PageAlreadyMapped" panic) once it is a few megabytes big.
    println!("cargo:rustc-link-arg-bins=--image-base=0x40000000");
    println!("cargo:rerun-if-changed=../third_party/doomgeneric");
    println!("cargo:rerun-if-env-changed=ZIG");
    println!("cargo:rerun-if-env-changed=ZERO_NO_DOOM");
    println!("cargo:rerun-if-changed=build.rs");

    if std::env::var_os("ZERO_NO_DOOM").is_some() {
        return;
    }
    let Some(zig) = find_zig() else {
        println!("cargo:warning=no Zig compiler found: building without Doom (see kernel/build.rs)");
        return;
    };

    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let source_dir = manifest.join("../third_party/doomgeneric");
    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());

    let mut sources: Vec<PathBuf> = std::fs::read_dir(&source_dir)
        .expect("third_party/doomgeneric is missing")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().map_or(false, |e| e == "c"))
        .collect();
    sources.sort();

    // compile in parallel: there are about a hundred files
    let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(12);
    let chunk = (sources.len() + workers - 1) / workers;
    let results: Vec<Result<PathBuf, String>> = std::thread::scope(|scope| {
        let handles: Vec<_> = sources
            .chunks(chunk.max(1))
            .map(|files| {
                let (zig, out_dir, source_dir) = (&zig, &out_dir, &source_dir);
                scope.spawn(move || {
                    files.iter().map(|f| compile(zig, f, out_dir, source_dir)).collect::<Vec<_>>()
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().unwrap()).collect()
    });
    let mut objects = Vec::new();
    for result in results {
        match result {
            Ok(object) => objects.push(object),
            Err(message) => panic!("building Doom: {}", message),
        }
    }

    let library = out_dir.join("libdoom.a");
    let _ = std::fs::remove_file(&library);
    let status = Command::new(&zig)
        .arg("ar")
        .arg("rcs")
        .arg(&library)
        .args(&objects)
        .status()
        .expect("could not run zig ar");
    assert!(status.success(), "zig ar failed");

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=doom");
    println!("cargo:rustc-cfg=has_doom");
}
