.DEFAULT_GOAL := compile

.PHONY: compile
compile:
	cargo build -vvv --release

.PHONY: clean
clean:
	cargo clean
