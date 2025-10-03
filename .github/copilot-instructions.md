# GitHub Copilot Instructions

This is a **single-binary Rust project** optimized for rapid development with
automated CI/CD. The project emphasizes simplicity, conventional commits for
automated releases, and cross-platform binary distribution.

## Project Architecture

- **Single binary crate**: Organized with proper Rust module structure for maintainability
- **Minimal dependencies**: Add dependencies incrementally as needed
- **Release automation**: Uses Google's release-please for semantic versioning
  based on conventional commits, it handle Cargo.toml versioning and changelog,
  so do not edit manually

## Key Development Workflows

### Testing

**IMPORTANT: Always run rustfmt before committing or running tests.**

```bash
# Format code (REQUIRED before committing/testing)
cargo fmt --all

# Run formatting check
cargo fmt --all -- --check

# Run all tests (includes tests in modules and integration tests)
cargo test --all-features --verbose

# Run linting with warnings as errors
RUSTFLAGS="-D warnings" cargo clippy --workspace --all-targets --all-features --verbose

# Run benchmarks
cargo bench --verbose
```

### Release Process

- **Conventional Commits**: This project REQUIRES conventional commit messages
  (feat:, fix:, etc.) for automated releases
- **Automated releases**: Push to `main` triggers release-please workflow that:
  1. Analyzes commit history
  2. Creates/updates release PR with changelog
  3. On merge, creates GitHub release and builds cross-platform binaries
- **Binary distribution**: Automatically builds for Linux x86_64 and macOS ARM64

### CI/CD Pipeline Structure

The `.github/workflows/` follows a dependency chain:

1. **rust.yml**: fmt → clippy → test → bench (on push only)
2. **release.yml**: Triggers on main branch, calls build-artifacts.yml if
   release created
3. **build-artifacts.yml**: Reusable workflow for cross-platform binary builds

## Project-Specific Conventions

### Project Organization

Follow standard Rust module organization patterns:

- **`src/main.rs`**: Keep focused on program entry point and CLI setup
- **`src/lib.rs`**: Create for shared library functionality (optional)
- **Module structure**: Organize features into logical modules using `mod.rs` or single `.rs` files
- **Tests**: Unit tests in same files, integration tests in `tests/` directory
- **Documentation**: Use `//!` for module-level docs, `///` for item-level docs

Example evolution:
1. Start simple in `main.rs`
2. Extract functions to modules as code grows
3. Create `lib.rs` when you have reusable components
4. Organize related functionality into module directories

### Binary Naming

- Default binary name: `template` (from Cargo.toml)
- Release binaries: `vibe-rust-template` (overridden in build workflow)
- Customize via `bin_name` input in build-artifacts workflow

### Profile Configuration

```toml
[profile.release]
lto = true          # Link-time optimization for smaller binaries
panic = "abort"     # Smaller binaries, faster startup
```

### Test Patterns

Tests follow standard Rust conventions:

- **Unit tests**: `#[test]` functions in the same file as the code they test
- **Integration tests**: Place in `tests/` directory for testing public API
- **Expected panic tests**: `#[test] #[should_panic(expected = "...")]`
- **Module organization**: Group related tests using `#[cfg(test)]` modules

### Cargo Features

- Uses `default = []` feature set
- Run tests with `--all-features` to ensure compatibility

## Just Commands (Recommended)

This project includes a `justfile` for convenient development commands:

```bash
# Show all available commands
just help

# Format code (REQUIRED before committing)
just format

# Check if code is formatted
just format-check

# Run linting
just lint

# Run tests
just test

# Format, lint, and test (recommended before committing)
just check

# Build release binary
just build

# Clean build artifacts
just clean
```

## Common Development Tasks

### Adding Dependencies

1. Add to `[dependencies]` or `[dev-dependencies]` in Cargo.toml
2. Update keywords/categories if the dependency changes project purpose
3. Consider if it affects the release profile optimization settings

### Creating a New Feature

1. Use conventional commit: `feat: add new functionality`
2. Format your code before committing: `just format` or `cargo fmt --all`
3. Run the full check suite: `just check` (format + lint + test)
4. Organize code into appropriate modules following Rust conventions
5. Place tests alongside the code they test or in integration tests
6. Update README.md if it changes usage patterns

### Debugging Build Issues

- Check `target/` directory for compilation artifacts
- Workflow caching uses prefixed keys (`v1-rust-{fmt,clippy,test,bench}`)
- Binary builds target specific platforms - check matrix in build-artifacts.yml

## File Structure Context

- `CHANGELOG.md`: Maintained by release-please, don't edit manually
- `CODE_OF_CONDUCT.md`: Standard contributor covenant
- `target/`: Standard Rust build artifacts, heavily cached in CI
- `.github/FUNDING.yml`: GitHub Sponsors configuration

## External Integrations

- **release-please**: Requires `MY_RELEASE_PLEASE_TOKEN` secret for triggering
  workflows
- **GitHub Actions**: Extensive caching strategy for Rust toolchain and cargo
  artifacts
- **Cross-compilation**: Handles Linux and macOS builds automatically on release

When working on this project, prioritize simplicity and maintainability. The
structure is designed for rapid iteration while maintaining production-ready
CI/CD practices.
