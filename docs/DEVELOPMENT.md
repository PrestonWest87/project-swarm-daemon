# Project Swarm - Development Guide

This document provides instructions for building, testing, and contributing to Project Swarm.

## Table of Contents
- [Prerequisites](#prerequisites)
- [Building](#building)
- [Running](#running)
- [Testing](#testing)
- [Code Structure](#code-structure)
- [Contributing](#contributing)
- [Debugging](#debugging)

## Prerequisites

### System Requirements

- **OS**: Linux, macOS, or Windows (Linux recommended for development)
- **Rust**: Latest stable (1.75+ recommended) or nightly with 2024 edition
- **SQLite**: 3.x (bundled with `rusqlite` feature)
- **OpenSSL**: System library (for some libp2p features)

### Rust Installation

```bash
# Install Rust via rustup
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Ensure you have the correct edition
rustc --version  # Should show 1.75+ or nightly with 2024 edition support

# If using nightly (required for 2024 edition as of early 2024):
rustup default nightly
```

### System Dependencies

**Ubuntu/Debian**:
```bash
sudo apt update
sudo apt install -y build-essential pkg-config libssl-dev
```

**Fedora/RHEL**:
```bash
sudo dnf install -y gcc pkg-config openssl-devel
```

**macOS**:
```bash
brew install openssl
```

## Building

### Clone the Repository

```bash
git clone git@github.com:yourusername/project-swarm-daemon.git
cd project-swarm-daemon
```

### Build Commands

```bash
# Debug build (fast, less optimized)
cargo build

# Release build (optimized, slower compilation)
cargo build --release

# Check compilation without building
cargo check

# Build with specific features (if any)
cargo build --features "some-feature"
```

### Build Output

- Debug binary: `target/debug/project-swarm-daemon`
- Release binary: `target/release/project-swarm-daemon`

### Clean Build

```bash
cargo clean  # Removes target/ directory
cargo build   # Fresh build
```

## Running

### Basic Execution

```bash
# Run debug build
cargo run

# Run release build
cargo run --release

# Pass arguments (if implemented)
cargo run -- --help
```

### First Run

On first run, the daemon will:
1. Generate a new Ed25519 identity (`swarm_network_key.bin`)
2. Generate a hybrid post-quantum identity (X25519 + ML-KEM)
3. Initialize the SQLite database (`swarm_dag.db`)
4. Connect to IPFS bootstrap nodes for DHT discovery
5. Start listening on port 4001 (QUIC + TCP)
6. Display the interactive prompt

**Output**:
```
[SYSTEM] Generating Hybrid X25519 + ML-KEM Cryptographic Keys...
[🔐] Local database initialized with ChaCha20-Poly1305 encryption.
[🛡️] Quantum-resistant identity secured. Node ID: 12D3KooW...

╔════════════════════════════════════════════════════════════════╗
║         🛜  DECENTRALIZED MESH ENGINE ONLINE               ║
╠════════════════════════════════════════════════════════════════╣
║ 🔐 End-to-End Encrypted  │  🧪 Post-Quantum  │  ⚡ P2P Mesh  ║
╚════════════════════════════════════════════════════════════════╝

📋 AVAILABLE COMMANDS:
  /invite    → Generate secure direct-connect invite link
  /join <b64> → Join room from invite link
  /history   → View local message history
  /discover  → Find peers on DHT
  /connect   → Connect to specific peer
  /whisper   → Send encrypted private message
  <message>  → Send broadcast to current room

💬 Ready. Type a message or /invite to start a new room.
```

### Interacting with the Daemon

Commands are typed into stdin:

```
> Hello, swarm!
[BROADCAST] 🟢 Message (Hash: a1b2c3d4) successfully encrypted and sent to channel.

> /invite
--- YOUR DIRECT CONNECT INVITE ---
Give this command to your peer:
  /join eyJ0b3BpYyI6InN3YXJtLXJvb20tMWVhYjIzYyJ9...
----------------------------------
[SYSTEM] 🟡 You have moved to private room: 'swarm-room-1eab23c'. Waiting for peers...

> /join eyJ0b3BpYyI6InN3YXJtLXJvb20tMWVhYjIzYyJ9...
[NETWORK] Invite verified. Sender identity confirmed. Joining room: 'swarm-room-1eab23c'

> /history
--- LOCAL DAG HISTORY (LAST 50) ---
[12D3KooW...] (Hash: a1b2c3d4): Hello, swarm!
---------------------------

> /whisper 12D3KooW... Hey there!
[BROADCAST] 🟢 ML-KEM encrypted whisper sent to 12D3KooW...

> /discover
[NETWORK] Querying Global DHT for public swarm nodes...

> /connect /ip4/192.168.1.100/tcp/4001/p2p/12D3KooW...
[NETWORK] Dialing direct Multiaddr...
```

## Testing

### Running Tests

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run a specific test
cargo test test_name

# Run tests in release mode
cargo test --release
```

### Current Test Status

As of now, the project is in early development. Tests should be added for:
- Cryptographic functions (crypto.rs)
- DAG message creation and hashing (store.rs)
- Serialization/deserialization of protocols (sync.rs, kex.rs)

**Example test** (to be added):
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dag_message_hash() {
        let msg = DagMessage::new(
            "test_author".to_string(),
            vec![],
            "Hello".to_string(),
        );
        assert!(!msg.id.is_empty());
        assert_eq!(msg.id.len(), 64); // SHA-256 hex = 64 chars
    }

    #[test]
    fn test_hybrid_identity_generation() {
        let id = crypto::HybridIdentity::generate();
        // Add assertions
    }
}
```

### Manual Testing

1. **Start two instances** (on same or different machines):
   ```bash
   # Terminal 1
   cargo run
   
   # Terminal 2 (another copy or different machine)
   cargo run
   ```

2. **Test invite flow**:
   - In Terminal 1: type `/invite`
   - Copy the `/join ...` command
   - In Terminal 2: paste the `/join ...` command
   - Wait for connection
   - Type messages in either terminal

3. **Test discovery**:
   - Type `/discover` to find peers on DHT
   - Type `/connect <peer_id>` to connect manually

4. **Test whisper**:
   - After connecting, use `/whisper <peer_id> <message>`
   - Verify E2EE works

## Code Structure

### Module Overview

```
src/
├── main.rs      # Entry point, CLI, swarm event loop
├── crypto.rs    # Post-quantum cryptography (X25519 + ML-KEM + ChaCha20)
├── store.rs     # SQLite DAG storage with encryption
├── sync.rs      # DAG sync protocol (request/response)
└── kex.rs       # Key exchange protocol (request/response)
```

### Key Files Explained

**main.rs** (~666 lines):
- Lines 1-60: Imports and struct definitions
- Lines 62-665: `main()` function with event loop
- Key functions: CLI parsing, swarm event handling, GossipSub publishing

**crypto.rs** (~132 lines):
- Lines 1-40: `HybridIdentity` struct and key generation
- Lines 42-63: Storage encryption (encrypt/decrypt for SQLite)
- Lines 65-120: Network encryption (seal/open for E2EE)
- Line 122-132: `seal_for_network` helper

**store.rs** (~182 lines):
- Lines 1-36: `DagMessage` struct and hash calculation
- Lines 38-69: `Store` struct and initialization
- Lines 71-97: Message and peer key storage
- Lines 99-170: Message retrieval and DAG sync queries

**sync.rs** (~19 lines):
- Protocol name constant
- `SyncRequest` and `SyncResponse` structs

**kex.rs** (~19 lines):
- Protocol name constant
- `KexRequest` and `KexResponse` structs

### Adding New Features

**To add a new CLI command**:
1. Edit `main.rs`
2. Find the `Ok(Some(line)) = stdin.next_line()` block
3. Add a new `if input == "/command"` block
4. Implement the feature

**To add a new protocol**:
1. Create a new file (e.g., `src/new_protocol.rs`)
2. Define request/response structs with serde
3. Add to `Cargo.toml` dependencies if needed
4. Register in `SwarmProtocol` behaviour
5. Handle events in the main event loop

**To modify crypto**:
1. Edit `src/crypto.rs`
2. Add tests for new functions
3. Update documentation in `docs/CRYPTO.md`

## Contributing

### Workflow

1. **Fork the repository** (if contributing to upstream)
2. **Create a feature branch**:
   ```bash
   git checkout -b feature/my-feature
   ```

3. **Make changes**:
   - Follow Rust conventions (rustfmt, clippy)
   - Add comments for public API
   - Update documentation if needed

4. **Test locally**:
   ```bash
   cargo fmt --check
   cargo clippy
   cargo test
   cargo build --release
   ```

5. **Commit with clear message**:
   ```bash
   git add .
   git commit -m "feat: add new feature X"
   ```

6. **Push to your fork/branch**:
   ```bash
   git push origin feature/my-feature
   ```

7. **Open a Pull Request** (if applicable)

### Code Style

- Run `cargo fmt` before committing
- Address all `cargo clippy` warnings
- Use meaningful variable names
- Document public functions with `///` comments
- Keep functions focused and small

### Commit Messages

Follow conventional commits:
```
feat: add new feature
fix: bug fix
docs: update documentation
refactor: code refactoring
test: add tests
chore: build process or auxiliary tool changes
```

## Debugging

### Logging

The daemon logs to `swarm_daemon.log` with detailed tracing.

**View logs**:
```bash
tail -f swarm_daemon.log

# Or view specific events
grep "ERROR" swarm_daemon.log
grep "KEX" swarm_daemon.log
```

**Log levels**:
- `trace` - Very detailed (libp2p internals)
- `debug` - General debugging (default)
- `info` - Important events
- `warn` - Warnings
- `error` - Errors only

**Change log level** (via RUST_LOG):
```bash
RUST_LOG=debug,libp2p_swarm=trace cargo run
```

### Common Issues

**Issue**: `error[E0658]: use of unstable library feature 'edition2024'`
**Fix**: Use nightly Rust: `rustup default nightly`

**Issue**: `signal: 11, SIGSEGV: invalid memory reference`
**Cause**: Usually a bug in unsafe code or FFI
**Fix**: Check logs, run with `RUST_BACKTRACE=1`

**Issue**: Connection timeout / No peers found
**Cause**: Firewall blocking port 4001, or NAT issues
**Fix**: Check firewall, ensure port 4001 is open

**Issue**: `Decryption failed` for whispers
**Cause**: Peer keys not exchanged, or wrong keys
**Fix**: Ensure KEX completed before whispering

### GDB/LLDB Debugging

```bash
# Debug build with symbols
cargo build

# Run with GDB
gdb target/debug/project-swarm-daemon
(gdb) run

# Or with LLDB (macOS)
lldb target/debug/project-swarm-daemon
(lldb) run
```

### Rust-Specific Debugging

```bash
# Backtrace on panic
RUST_BACKTRACE=1 cargo run

# Debug prints (in code)
dbg!(&variable);
println!("{:?}", variable);

# Debugger support
rustup component add rust-gdb rust-lldb
```

## Docker (Optional)

### Build Docker Image

```dockerfile
# Dockerfile (already exists in project)
FROM rust:latest
WORKDIR /app
COPY . .
RUN cargo build --release
CMD ["./target/release/project-swarm-daemon"]
```

```bash
docker build -t project-swarm-daemon .
docker run -it --rm project-swarm-daemon
```

### Docker Compose (Multiple Nodes)

```yaml
version: '3'
services:
  node1:
    build: .
    ports:
      - "4001:4001/udp"
      - "4001:4001"
  node2:
    build: .
    ports:
      - "4002:4001/udp"
      - "4002:4001"
```

## Deployment

### Running as a Service (Linux)

Create systemd service:

```ini
# /etc/systemd/system/swarm-daemon.service
[Unit]
Description=Project Swarm Daemon
After=network.target

[Service]
Type=simple
User=youruser
WorkingDirectory=/path/to/project-swarm-daemon
ExecStart=/path/to/project-swarm-daemon/target/release/project-swarm-daemon
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl enable swarm-daemon
sudo systemctl start swarm-daemon
sudo journalctl -u swarm-daemon -f
```

### Headless Mode (Future)

For server deployment without GUI:
```bash
# Run daemon in background
nohup ./target/release/project-swarm-daemon > /dev/null 2>&1 &

# Or use screen/tmux
tmux new -s swarm
cargo run --release
# Ctrl-b d to detach
```

## Resources

- **libp2p Documentation**: https://libp2p.io/docs/
- **Rust Book**: https://doc.rust-lang.org/book/
- **Tokio Async Runtime**: https://tokio.rs/tokio/tutorial
- **ML-KEM (NIST FIPS 203)**: https://csrc.nist.gov/pubs/fips/203/final
- **Project Swarm Spec**: See `Guiding Doc.md` in project root
