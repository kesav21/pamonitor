target/release/pamonitor: Cargo.lock Cargo.toml src/main.rs
	@cargo build --release

.PHONY: install
install: target/release/pamonitor
	@cp $^ "$$XDG_BIN_DIR"

.PHONY: clean
clean:
	@rm -r target
