build:
	cargo build --target x86_64-zero.json

run:
	cargo run --target x86_64-zero.json

run-release:
	cargo run --release --target x86_64-zero.json

build-release:
	cargo build --release --target x86_64-zero.json