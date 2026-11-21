QEMU ?= qemu-system-x86_64
# sound: an AC97 card (Doom plays music and effects through it); use pa, alsa or sdl on other systems
AUDIO ?= -audiodev pa,id=snd0 -device AC97,audiodev=snd0
# OVMF firmware for UEFI boots (QEMU ships it in its share directory)
OVMF ?= /usr/share/OVMF/OVMF_CODE.fd
IMG = target/x86_64-zero/debug

build:
	cargo build

# UEFI boot: honors the 1920x1080 request in build.rs
run: build
	$(QEMU) -accel kvm -accel tcg -m 256M -smp 4 -serial stdio $(AUDIO) -bios $(OVMF) -drive format=raw,file=$(IMG)/uefi.img

# legacy BIOS boot (capped at 1280x720 by the bootloader)
run-bios: build
	$(QEMU) -accel kvm -accel tcg -m 256M -smp 4 -serial stdio $(AUDIO) -drive format=raw,file=$(IMG)/bios.img

run-release:
	cargo build --release
	$(QEMU) -accel kvm -accel tcg -m 256M -smp 4 -serial stdio $(AUDIO) -bios $(OVMF) -drive format=raw,file=target/x86_64-zero/release/uefi.img

build-release:
	cargo build --release

test:
	cargo +nightly test --workspace

test_lib:
	cargo +nightly test --lib
