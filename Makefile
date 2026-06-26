# muse — project tasks. Run `make` or `make help` to list targets.

# Directory to open with `make run` (override: make run DIR=~/Music)
DIR ?= .
# File for `make probe` (override: make probe FILE=track.flac)
FILE ?=
BIN := muse

# Cross-compile (uses cargo-zigbuild; zig bundles the C toolchain + libs)
WIN_TARGET   := x86_64-pc-windows-gnu
LINUX_TARGET := x86_64-unknown-linux-gnu
DIST         := dist

.DEFAULT_GOAL := help

.PHONY: help build release run probe test check fmt fmt-check lint audit \
        clean install uninstall tag cross-setup windows linux dist

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

##@ Cross-compile
cross-setup: ## Install cross-compile deps (cargo-zigbuild + rust targets; needs zig)
	@command -v zig >/dev/null || { echo "zig not found — brew install zig"; exit 1; }
	cargo install cargo-zigbuild
	rustup target add $(WIN_TARGET) $(LINUX_TARGET)

windows: ## Cross-build a Windows x64 exe -> $(DIST)/muse-windows-x64.exe
	cargo zigbuild --release --target $(WIN_TARGET)
	@mkdir -p $(DIST)
	cp target/$(WIN_TARGET)/release/$(BIN).exe $(DIST)/$(BIN)-windows-x64.exe
	@echo "built $(DIST)/$(BIN)-windows-x64.exe"

# Linux needs ALSA (libasound) dev libs for the *target*. That ships with a
# Linux host but not a macOS one, so this works when run ON Linux (or in CI /
# Docker with libasound2-dev installed). From macOS it will fail at link.
linux: ## Build a Linux x64 binary -> $(DIST)/muse-linux-x64 (run on Linux/CI)
	cargo zigbuild --release --target $(LINUX_TARGET)
	@mkdir -p $(DIST)
	cp target/$(LINUX_TARGET)/release/$(BIN) $(DIST)/$(BIN)-linux-x64
	@echo "built $(DIST)/$(BIN)-linux-x64"

dist: windows linux ## Build both Windows and Linux binaries into $(DIST)/

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
