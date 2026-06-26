# muse — project tasks. Run `make` or `make help` to list targets.

# Directory to open with `make run` (override: make run DIR=~/Music)
DIR ?= .
# File for `make probe` (override: make probe FILE=track.flac)
FILE ?=
BIN := muse

.DEFAULT_GOAL := help

.PHONY: help build release run probe test check fmt fmt-check lint audit \
        clean install uninstall tag

help: ## Show this help
	@awk 'BEGIN {FS = ":.*?## "} \
		/^##@/ {printf "\n\033[1m%s\033[0m\n", substr($$0, 5); next} \
		/^[a-zA-Z_-]+:.*?## / {printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2}' \
		$(MAKEFILE_LIST)

##@ Build
build: ## Debug build
	cargo build

release: ## Optimized release build
	cargo build --release

clean: ## Remove build artifacts
	cargo clean

##@ Run
run: build ## Run on DIR (default ".") — e.g. make run DIR=~/Music
	cargo run -- $(DIR)

probe: build ## Headless decode+tag check of FILE — make probe FILE=track.mp3
	@test -n "$(FILE)" || { echo "usage: make probe FILE=<path>"; exit 1; }
	./target/debug/$(BIN) --probe "$(FILE)"

##@ Quality
test: ## Run tests
	cargo test

check: ## Fast type-check without building artifacts
	cargo check

fmt: ## Format the code
	cargo fmt

fmt-check: ## Verify formatting (CI-friendly, no writes)
	cargo fmt --check

lint: ## Clippy with warnings as errors
	cargo clippy --all-targets -- -D warnings

audit: ## Scan dependencies for security advisories (needs cargo-audit)
	cargo audit

##@ Install
install: release ## Install the binary to ~/.cargo/bin
	cargo install --path .

uninstall: ## Remove the installed binary
	cargo uninstall $(BIN)

##@ Release
tag: ## GPG-sign a git tag from the Cargo.toml version (vX.Y.Z)
	@v=$$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2); \
		echo "tagging v$$v"; \
		git tag -s "v$$v" -m "$(BIN) v$$v"
