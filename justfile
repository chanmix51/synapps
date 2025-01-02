compile:
	cargo build

release:
	cargo build --release

test: check
	cargo test

check:
	cargo fmt
	cargo clippy

