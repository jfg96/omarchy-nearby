# localsend-rs

[![Crates.io](https://img.shields.io/crates/v/localsend-rs)](https://crates.io/crates/localsend-rs)

![LocalSend Rust Banner](localsend-rs.jpg)

A high-performance, type-safe implementation of [LocalSend](https://localsend.org) protocol (v2) in Rust. This project provides both a library and a feature-rich CLI/TUI for secure, local network file and text transfers.

## Features

### Core Functionality
- **Protocol Compatibility**: Full interoperability with official LocalSend clients (Android, iOS, Windows, macOS, Linux).
- **Automatic Discovery**: Multicast UDP discovery to find devices on your network instantly.
- **Direct Send**: Transfer files or text directly to an IP address for speed and reliability.
- **HTTPS Security**: TLS encryption for all transfers using protocol-compliant certificate fingerprinting.
- **Text Messages**: Support for sending and receiving instant text messages.
- **CLI & TUI**: Intuitive command-line interface with optional terminal-based UI.

### Performance & Quality
- **Streaming Transfers**: Memory-efficient streaming for large files (no OOM on multi-gigabyte transfers).
- **Async I/O**: Non-blocking file operations for high concurrency.
- **Type Safety**: Strong typing throughout (`Protocol`, `SessionId`, `FileId`, `Token`, `Port`) prevents bugs at compile time.
- **State Management**: Type-safe state machine for transfer lifecycle.
- **Well-Tested**: 32+ unit tests covering core functionality.

## Installation

### Option 1: Install from Crates.io (Recommended)

Install CLI version:

```bash
cargo install localsend-rs
```

Install with interactive TUI:

```bash
cargo install localsend-rs --features tui
```

Then run with:

```bash
localsend-rs --help        # CLI mode
localsend-rs tui           # Launch TUI (if installed with --features tui)
```

### Option 2: Build from Source

Ensure you have Rust and Cargo installed. Clone the repository and build from source:

```bash
git clone https://github.com/CrossCopy/localsend-rs.git
cd localsend-rs

# CLI + HTTPS (default)
cargo build --release

# With TUI support
cargo build --release --features tui

# All features (CLI + HTTPS + TUI)
cargo build --release --features all
```

## Quick Start

### 1. Discover Devices

Scan the local network for available LocalSend instances:

```bash
cargo run --features https -- discover
```

### 2. Receive Files

Start the receiver server (HTTPS is the default, matching the official app):

```bash
# Start receiving on default port (53317), HTTPS by default
cargo run --features https -- receive

# Use plain HTTP for interop/testing with HTTP-only peers
cargo run --features https -- receive --no-https
```

### 3. Send Files

Send a file to a device by its alias or IP:

```bash
# Send by alias
cargo run --features https -- send "My Phone" ./photos/vacation.jpg

# Send by IP address (bypasses discovery); host:port for a non-default port
cargo run --features https -- send 192.168.1.50 ./documents/report.pdf
cargo run --features https -- send 192.168.1.50:53318 ./documents/report.pdf
```

### 4. Send Text Messages

You can send plain text instead of files by providing a string that isn't a file path:

```bash
cargo run --features https -- send "ROG16" "Hello from Rust CLI!"
```

### 5. Launch TUI

For an interactive terminal-based UI:

```bash
cargo run --features all -- tui
```

## CLI Usage

### `discover`

Find devices on the local network.

- `--timeout <SECS>`: Search duration (default: 10s).
- `--json`: Output discovered devices in JSON format.

### `receive`

Start a LocalSend server to accept incoming transfers.

- `--port <PORT>`: Custom port (default: 53317).
- `--no-https`: Use plain HTTP instead of the default HTTPS.
- `--alias <NAME>`: Custom device name shown to others.
- `--directory <PATH>`: Save directory for received files (default: `./downloads`).

### `send`

Send data to another device.

- `<TARGET>`: Device alias, hostname, or IP address.
- `<FILES...>`: One or more file paths or text strings.
- `--pin <PIN>`: Optional PIN for protected transfers.

### `tui` (requires `--features tui`)

Launch the interactive terminal-based UI for file transfers.

- `--port <PORT>`: Custom port (default: 53317).
- `--no-https`: Use plain HTTP instead of the default HTTPS.
- `--alias <NAME>`: Custom device name shown to others.
- `--pin <PIN>`: Require this PIN from senders before a transfer is offered.
- `--auto-accept`: Accept every incoming transfer without prompting.

```bash
# With all features enabled (HTTPS by default)
cargo run --features all -- tui
```

The TUI supports:

- **Inbox** — arrow keys select a received item, `Enter` reveals it in the OS
  file manager; text messages show a `💬` preview inline.
- **Incoming transfer dialog** — `↑/↓` move, `Space` toggles a file, `a` toggles
  all, `Enter` accepts the selected subset, `y` accepts all, `n`/`Esc` declines.
- **Settings** — `Space` toggles auto-accept live (applies to the next request).
- **Sending to a PIN-protected peer** — a `401` prompts for the PIN and retries.

## Architecture

The codebase is organized into clean, domain-driven modules:

```
src/
├── core/              # Core domain logic
│   ├── builders.rs     # Builder patterns (DeviceInfoBuilder)
│   ├── device.rs       # Device operations
│   ├── file.rs         # File operations
│   ├── session.rs      # Session management
│   └── transfer.rs    # Transfer state machine
├── crypto/            # Cryptography (modularized)
│   ├── fingerprint.rs   # Device fingerprinting
│   ├── hash.rs        # SHA-256 hashing
│   └── tls.rs         # TLS certificate generation
├── storage/           # Storage abstraction
│   ├── traits.rs       # FileSystem trait
│   └── tokio_fs.rs   # Default implementation
├── discovery/         # Multicast UDP & HTTP discovery
├── server/            # Axum HTTP/HTTPS server
├── client/            # Request-based client
├── protocol/          # Protocol types & validation
│   ├── types.rs       # Strong types (SessionId, FileId, etc.)
│   └── validation.rs  # Protocol validation
├── cli/               # Command-line interface
├── tui/               # Terminal UI
├── error.rs           # Structured error handling
└── prelude.rs        # Convenience exports
```

### Key Design Patterns

- **Newtype Pattern**: Strong typing for protocol identifiers (`SessionId`, `FileId`, `Token`, `Port`)
- **State Machine**: Type-safe transfer lifecycle management
- **Builder Pattern**: Fluent API for constructing `DeviceInfo`
- **Strategy Pattern**: Pluggable `FileSystem` implementations for testing
- **Storage Abstraction**: `FileSystem` trait enables mocking and alternative backends

## Features

| Feature | Default | Description |
|----------|----------|-------------|
| `cli` | ✅ | Command-line interface (clap-based) |
| `https` | ✅ | TLS/SSL support for secure transfers |
| `tui` | ❌ | Interactive terminal UI (ratatui-based) |
| `all` | ❌ | Enable all features (`cli` + `https` + `tui`) |

### Build Examples

```bash
# Default (cli + https)
cargo build

# CLI + HTTPS + TUI
cargo build --features all

# Development with all features
cargo build --release --features all
```

## Development Status

This project is under active development with focus on performance and type safety.

### Completed Features
- [x] Discovery (Multicast UDP)
- [x] Receiving (HTTPS/HTTP) with streaming
- [x] Sending Files with streaming uploads
- [x] Sending Text
- [x] TUI (Terminal UI)
- [x] Type-safe protocol types
- [x] State machine for transfer lifecycle
- [x] Builder patterns for ergonomic API
- [x] Storage abstraction for testability

### In Progress
- [ ] Streaming download support
- [ ] Integration test suite
- [ ] Progress callbacks for transfers
- [ ] Resume interrupted transfers

### Future Roadmap
- [ ] Direct Download (v3 feature)
- [ ] Connection pooling
- [ ] Parallel chunk uploads
- [ ] Checksum validation during streaming

## Performance

### Memory Efficiency
- **Streaming Uploads**: O(8KB buffer) instead of O(file_size) - can transfer multi-GB files with <100MB RAM
- **Async I/O**: Non-blocking operations enable high concurrency (100+ simultaneous transfers)
- **Lock Optimization**: `tokio::sync::RwLock` for server, non-blocking `try_read()` for TUI

### Type Safety Benefits
- **Compile-time Guarantees**: Newtypes prevent mixing up session IDs, file IDs, tokens, etc.
- **Protocol Validation**: Version and device info validated at API boundaries
- **Error Context**: Structured errors with relevant context for debugging

## Testing

```bash
# Run all tests
cargo test

# Run tests with specific features
cargo test --features all

# Run clippy for code quality
cargo clippy --all-features

# Check formatting
cargo fmt --check
```

- `cargo test` — unit + conformance + in-process interop suites (no Docker needed).
- `./e2e/run.sh` — containerized sender→receiver e2e over the compose network (requires Docker).

## About

This is a [CrossCopy](https://crosscopy.io) project - a high-performance Rust implementation of the LocalSend protocol for fast, secure local network file transfers.

## License

MIT License - see [LICENSE](LICENSE) for details.
