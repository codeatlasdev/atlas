.PHONY: dev daemon check app build test clean setup

# ─── Development ──────────────────────────────────────────

dev: ## Start full dev environment (TUI with all processes)
	@mprocs

daemon: ## Run daemon with auto-restart on file changes
	RUST_LOG=atlas=debug cargo watch -q -c -x 'run --bin atlas-daemon'

check: ## Watch for errors and clippy warnings
	cargo watch -q -c -x 'clippy --all-targets'

# ─── Build ────────────────────────────────────────────────

build: ## Build everything (Rust + Swift)
	cargo build
	cd app && swift build

build-release: ## Build optimized binaries
	cargo build --release

# ─── Testing ──────────────────────────────────────────────

test: ## Run all tests
	cargo test
	cd app && swift test

# ─── App ──────────────────────────────────────────────────

app: ## Open SwiftUI app in Xcode
	cd app && open Package.swift

app-build: ## Build SwiftUI app
	cd app && swift build

app-run: ## Run SwiftUI app from terminal
	cd app && swift build && .build/debug/Atlas

# ─── CLI ──────────────────────────────────────────────────

cli: ## Run CLI (pass ARGS="servers list")
	cargo run --bin atlas-cli -- $(ARGS)

# ─── Utilities ────────────────────────────────────────────

clean: ## Clean all build artifacts
	cargo clean
	cd app && swift package clean

setup: ## Install dev dependencies
	@echo "Installing dev tools..."
	cargo install cargo-watch mprocs
	@echo "✓ Ready. Run 'make dev' to start."

fmt: ## Format Rust code
	cargo fmt

lint: ## Run clippy with strict checks
	cargo clippy --all-targets -- -D warnings

# ─── Socket testing ───────────────────────────────────────

ping: ## Test daemon connection
	@echo '{"method":"servers.list","params":{},"id":"ping"}' | nc -U ~/.atlas/atlas.sock

# ─── Help ─────────────────────────────────────────────────

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-15s\033[0m %s\n", $$1, $$2}'

.DEFAULT_GOAL := help
