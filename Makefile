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
	cd app && xcodegen generate -q && xcodebuild -project Atlas.xcodeproj -scheme Atlas -configuration Debug build -quiet

build-release: ## Build optimized binaries
	cargo build --release

# ─── Testing ──────────────────────────────────────────────

test: ## Run all tests
	cargo test
	cd app && xcodegen generate -q && xcodebuild -project Atlas.xcodeproj -scheme AtlasTests test -quiet

# ─── App ──────────────────────────────────────────────────

app: ## Generate Xcode project and open in Xcode
	cd app && xcodegen generate && open Atlas.xcodeproj

app-build: ## Build SwiftUI app via xcodebuild
	cd app && xcodegen generate && xcodebuild -project Atlas.xcodeproj -scheme Atlas -configuration Debug build

app-generate: ## Regenerate Xcode project from project.yml
	cd app && xcodegen generate

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
