# commander-mcp

A Model Context Protocol (MCP) server for background process management. Built in Rust, commander-mcp provides powerful tools for running, monitoring, and managing background processes using PMDaemon through the MCP protocol.

## Features

- **MCP Protocol Support**: Built on the official [Rust MCP SDK](https://github.com/modelcontextprotocol/rust-sdk) (v0.7)
- **PMDaemon Integration**: Powerful process management with built-in monitoring and log management
- **Process Management Tools**: Run, kill, restart, and monitor processes
- **Persistent Logs**: Process output is saved to files and can be read at any time
- **Cross-Platform**: Works on Linux, Windows, and macOS
- **Standard I/O Transport**: Communicates via stdin/stdout for easy integration
- **Async Runtime**: Built on Tokio for high-performance async operations
- **Type-Safe**: Leverages Rust's type system for reliability

## Quick Start

### Prerequisites

- Rust 1.70 or later (for building from source)
- Cargo (comes with Rust)

### Installation

#### Using the Install Script (Recommended)

The easiest way to install commander-mcp is using the install script:

```bash
curl -fsSL https://raw.githubusercontent.com/towry/commander-mcp/main/install.sh | bash
```

Or download and inspect the script first:

```bash
curl -fsSL https://raw.githubusercontent.com/towry/commander-mcp/main/install.sh -o install.sh
chmod +x install.sh
./install.sh
```

**Supported platforms:**
- macOS Apple Silicon (M1/M2/M3) - aarch64-apple-darwin
- Linux x86_64 - x86_64-unknown-linux-gnu

The script will:
1. Detect your platform automatically
2. Download the latest release binary
3. Install it to `/usr/local/bin`
4. Verify the installation

#### Building from Source

```bash
# Clone the repository
git clone https://github.com/towry/commander-mcp.git
cd commander-mcp

# Build the project
cargo build --release

# Run the server
cargo run --release
```

### Usage

The MCP server communicates via standard input/output using the MCP protocol. You can test it using the [MCP Inspector](https://github.com/modelcontextprotocol/inspector):

```bash
# Install MCP Inspector (requires Node.js)
npm install -g @modelcontextprotocol/inspector

# If installed via install script
npx @modelcontextprotocol/inspector commander-mcp

# If running from source
npx @modelcontextprotocol/inspector cargo run --release
```

#### Using with Claude Desktop

To use this MCP server with Claude Desktop, add the following to your Claude Desktop configuration file:

**macOS**: `~/Library/Application Support/Claude/claude_desktop_config.json`
**Linux**: `~/.config/Claude/claude_desktop_config.json`

```json
{
  "mcpServers": {
    "commander": {
      "command": "/usr/local/bin/commander-mcp"
    }
  }
}
```

Or if running from source:

```json
{
  "mcpServers": {
    "commander": {
      "command": "cargo",
      "args": ["run", "--release", "--manifest-path", "/path/to/commander-mcp/Cargo.toml"]
    }
  }
}
```

## Available Tools

### run

Execute a command in the background.

**Description**: Run a command in the background. Returns the process ID that can be used with other tools.

**Parameters**:
- `command` (string): The command to run

**Example**:
```json
{
  "command": "npm run dev"
}
```

**Response**:
Returns a process ID generated from the command (e.g., `npm_run`).

### kill

Kill a previously started process by its ID.

**Parameters**:
- `process_id` (string): The process ID to kill

**Example**:
```json
{
  "process_id": "npm_run"
}
```

### kill_all

Kill all running processes.

**Parameters**: None

**Example**: Call with no parameters to stop all processes.

### read

Read the stdout output from a process by its ID.

**Description**: Returns recent log output (last 100 lines).

**Parameters**:
- `process_id` (string): The process ID to read output from

**Example**:
```json
{
  "process_id": "npm_run"
}
```

**Response**:
Returns the recent output from the process log file.

### restart

Restart a previously run process by its ID.

**Parameters**:
- `process_id` (string): The process ID to restart

**Example**:
```json
{
  "process_id": "npm_run"
}
```

### list

List all currently running or stopped processes.

**Parameters**: None

**Response**:
Returns a list of all processes with their status, PID, uptime, CPU usage, memory usage, and restart count.

## How It Works

1. **Process Management**: Uses PMDaemon, a high-performance process manager written in Rust
2. **Process IDs**: Automatically generated based on command keywords (e.g., `npm run dev` → `npm_run`)
3. **Persistent Logs**: All process output is saved to log files that can be read at any time
4. **Real-time Monitoring**: Track CPU, memory usage, and process state
5. **Cross-platform**: Native support for Linux, Windows, and macOS

## Key Features

- **Persistent log management**: Logs are saved to files and can be read at any time (not just live output)
- **Per-process log access**: Can read logs for specific processes
- **No external dependencies**: PMDaemon is a Rust library, no external binaries needed
- **Cross-platform**: Native support for Windows, macOS, and Linux
- **Built-in monitoring**: CPU and memory tracking included
- **Direct API**: Programmatic control through Rust API

## Development

### Running Tests

```bash
# Run all tests
cargo test --all-features --verbose

# Run tests with output
cargo test -- --nocapture
```

### Code Quality

This project follows Rust best practices and uses conventional commits for automated releases.

```bash
# Format code (REQUIRED before committing)
cargo fmt --all

# Check formatting
cargo fmt --all -- --check

# Run linting
RUSTFLAGS="-D warnings" cargo clippy --workspace --all-targets --all-features --verbose
```

### Using Just Commands

This project includes a `justfile` for convenient development:

```bash
# Format, lint, and test
just check

# Format code
just format

# Run linting
just lint

# Run tests
just test

# Build release binary
just build
```

## Project Structure

```
commander-mcp/
├── src/
│   ├── main.rs              # Entry point with MCP server setup
│   ├── process_server.rs    # MCP tools implementation
│   └── process_manager.rs   # PMDaemon wrapper for process management
├── Cargo.toml               # Project dependencies and metadata
└── README.md                # This file
```

## Related

- [PMDaemon](https://github.com/entrepeneur4lyf/pmdaemon) - The process manager library powering this server
- [MCP Protocol](https://modelcontextprotocol.io) - Model Context Protocol specification
- [Rust MCP SDK](https://github.com/modelcontextprotocol/rust-sdk) - Official MCP SDK for Rust
