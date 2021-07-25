.DEFAULT_GOAL := install
.PHONY: install
install: target/release/pamonitor
	@cp target/release/pamonitor "$$XDG_BIN_DIR"

.PHONY: clean
clean:
	@rm -r target

target/release/pamonitor:
	@cargo build --release
