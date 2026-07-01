# muse — project tasks. Run `make` or `make help` to list targets.

# Directory to open with `make run` (override: make run DIR=~/Music)
DIR ?= .
# File for `make probe` (override: make probe FILE=track.flac)
FILE ?=
BIN := muse

DIST := dist

# Cross-compile targets (uses cargo-zigbuild; zig bundles the C toolchain).
WIN_X64 := x86_64-pc-windows-gnu
WIN_ARM := aarch64-pc-windows-gnullvm
MAC_X64 := x86_64-apple-darwin
MAC_ARM := aarch64-apple-darwin
LNX_X64 := x86_64-unknown-linux-gnu
LNX_ARM := aarch64-unknown-linux-gnu

# Canned build recipe. $(1)=rust triple, $(2)=output name, $(3)=builder.
# Windows targets get a .exe source suffix automatically.
define cross_build
	@mkdir -p $(DIST)
	$(if $(filter zig,$(3)),cargo zigbuild,cargo build) --release --target $(1)
	cp target/$(1)/release/$(BIN)$(if $(findstring windows,$(1)),.exe) $(DIST)/$(2)
	@echo "  -> $(DIST)/$(2)"
endef

.DEFAULT_GOAL := help

.PHONY: help build release run probe test check fmt fmt-check lint audit \
        clean install uninstall tag cross-setup build-all \
        win-x64 win-x86 win-arm mac-x64 mac-arm linux-x64 linux-arm

help: ## Show this help
	@awk 'BEGIN {FS = ":.*?## "} \
		/^##@/ {printf "\n\033[1m%s\033[0m\n", substr($$0, 5); next} \
		/^[a-zA-Z_-]+:.*?## / {printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2}' \
		$(MAKEFILE_LIST)

##@ Build
build: ## Debug build
	cargo build

release: ## Optimized native build -> dist/muse
	cargo build --release
	@mkdir -p $(DIST)
	cp target/release/$(BIN) $(DIST)/$(BIN)
	@echo "  -> $(DIST)/$(BIN)"

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

##@ Cross-compile (all outputs land in dist/)
cross-setup: ## Install cargo-zigbuild + all rust targets (needs zig)
	@command -v zig >/dev/null || { echo "zig not found — brew install zig"; exit 1; }
	cargo install cargo-zigbuild
	rustup target add $(WIN_X64) $(WIN_ARM) $(MAC_X64) $(MAC_ARM) $(LNX_X64) $(LNX_ARM)

mac-arm: ## macOS arm64   -> dist/muse-macos-aarch64
	$(call cross_build,$(MAC_ARM),$(BIN)-macos-aarch64,cargo)

mac-x64: ## macOS x86_64  -> dist/muse-macos-x86_64
	$(call cross_build,$(MAC_X64),$(BIN)-macos-x86_64,cargo)

win-x64: ## Windows x86_64 -> dist/muse-windows-x86_64.exe
	$(call cross_build,$(WIN_X64),$(BIN)-windows-x86_64.exe,zig)

win-x86: win-x64 ## Alias for win-x64

win-arm: ## Windows arm64  -> dist/muse-windows-aarch64.exe
	$(call cross_build,$(WIN_ARM),$(BIN)-windows-aarch64.exe,zig)

# Linux needs ALSA (libasound) dev libs for the target — present on a Linux host
# but not on macOS, so these build on Linux / in CI (apt install libasound2-dev),
# not by cross-compiling from a Mac.
linux-x64: ## Linux x86_64 -> dist/muse-linux-x86_64 (Linux/CI host)
	$(call cross_build,$(LNX_X64),$(BIN)-linux-x86_64,zig)

linux-arm: ## Linux arm64  -> dist/muse-linux-aarch64 (Linux/CI host)
	$(call cross_build,$(LNX_ARM),$(BIN)-linux-aarch64,zig)

build-all: ## Build every OS/arch into dist/ (continues past failures)
	@mkdir -p $(DIST)
	-@$(MAKE) --no-print-directory mac-arm
	-@$(MAKE) --no-print-directory mac-x64
	-@$(MAKE) --no-print-directory win-x64
	-@$(MAKE) --no-print-directory win-arm
	-@$(MAKE) --no-print-directory linux-x64
	-@$(MAKE) --no-print-directory linux-arm
	@echo "=== $(DIST) ==="; ls -1 $(DIST)

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
