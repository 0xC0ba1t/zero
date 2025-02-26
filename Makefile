build:
	cargo build

run:
	qemu-system-x86_64 -drive format=raw,file=Z:\zero\target\x86_64-zero\debug\bootimage-zero.bin

run-release:
	cargo run --release

build-release:
	cargo build --release