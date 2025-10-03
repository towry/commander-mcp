# Justfile for vibe-rust-template
# Use `just <recipe>` to run commands

# Show available recipes
help:
    @just --list

# Format all Rust code
format:
    cargo fmt --all

# Check if code is formatted
format-check:
    cargo fmt --all -- --check

# Run linting with warnings as errors
lint:
    RUSTFLAGS="-D warnings" cargo clippy --workspace --all-targets --all-features --verbose

# Run all tests
test:
    cargo test --all-features --verbose

# Run benchmarks
bench:
    cargo bench --verbose

# Format code, then run linting and tests (recommended before committing)
check: format lint test

# Build release binary
build:
    cargo build --release

# Clean build artifacts
clean:
    cargo clean