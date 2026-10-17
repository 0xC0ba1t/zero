build:
	cargo build

run:
	cargo build
	qemu-system-x86_64 -drive format=raw,file=target/x86_64-zero/debug/bios.img

run-release:
	cargo build --release
	qemu-system-x86_64 -drive format=raw,file=target/x86_64-zero/release/bios.img

build-release:
	cargo build --release

test:
	cargo +nightly test --workspace

test_lib:
	cargo +nightly test --lib
