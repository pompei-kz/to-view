.DEFAULT_GOAL := compile

.PHONY: compile
compile:
	cargo build --release
