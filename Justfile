# Default: list available recipes
default:
    @just --list

# Build the project
build:
    cargo build

# Build in release mode
release:
    cargo build --release

# Run clippy (warnings as errors)
lint:
    cargo clippy -- -D warnings

# Run all checks (build + lint)
check: build lint

# Run the binary
run:
    cargo run

# Run with all output redirected to /tmp/ralph_tui.log for debugging (tail -f /tmp/ralph_tui.log in another terminal)
run-log:
    cargo run 2>/tmp/ralph_tui.log

# Run with a release build
run-release:
    cargo run --release

# Run tests
test:
    cargo test

# Auto-fix clippy suggestions
fix:
    cargo clippy --fix --allow-staged

# Format source code
fmt:
    cargo fmt

# Check formatting without modifying files
fmt-check:
    cargo fmt -- --check

# Remove build artifacts
clean:
    cargo clean

# Copy agents and skills into ~/.claude (removes old prd/prd-clarify skill dirs first)
set-resources:
    rm -rf ~/.claude/skills/prd ~/.claude/skills/prd-clarify
    cp -rf ./resources/ ~/.claude/

# Installs this app as `ralph-tui` command
install:
    cargo install --path .
