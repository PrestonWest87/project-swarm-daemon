# Project Swarm Daemon

[![Rust](https://img.shields.io/badge/Rust-2024-orange.svg)](https://www.rust-lang.org/)
[![libp2p](https://img.shields.io/badge/libp2p-0.53-blue.svg)](https://libp2p.io/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

A post-quantum, peer-to-peer (P2P), masterless, locally hosted communication daemon. Project Swarm is designed for high resilience, aggressive security, and complete independence from central cloud infrastructure.

## 🚀 Features

- **🔐 Post-Quantum Cryptography**: Hybrid X25519 + ML-KEM-768 (NIST standardized) key exchange
- **⚡ P2P Mesh Network**: No central servers, fully decentralized architecture
- **🛡️ End-to-End Encryption**: ChaCha20-Poly1305 AEAD for all messages
- **📊 DAG-Based Storage**: Cryptographic message chaining with SHA-256
- **🌐 NAT Traversal**: AutoNAT, DCUtR hole-punching, and emergent relay servers
- **🔑 Multiple Identity Layers**: Ed25519 (protocol) + Hybrid PQ (encryption)
- **💾 Encrypted Storage**: SQLite with ChaCha20-Poly1305 at rest

## 📋 Table of Contents

- [Architecture](#architecture)
- [Quick Start](#quick-start)
- [Installation](#installation)
- [Usage](#usage)
- [Documentation](#documentation)
- [Development](#development)
- [Security](#security)
- [Contributing](#contributing)
- [License](#license)

## 🏗️ Architecture

Project Swarm operates as a **masterless mesh** where every node is sovereign:

```
┌─────────────────────────────────────────────────┐
│           Decentralized Swarm Mesh              │
├─────────────────────────────────────────────────┤
│  Node A ◄──┐      ┌──► Node B                │
│             │      │                           │
│  (Relay)    ├──► Node C ◄──┼──► Node D       │
│             │      │          │                │
│  Node E ◄──┘      └──► Node F ►── Node G     │
└─────────────────────────────────────────────────┘
```

### Core Components

| Component | Technology | Purpose |
|-----------|------------|---------|
| **Core Daemon** | Rust (2024 edition) | P2P routing, crypto, state management |
| **Networking** | libp2p 0.53 | QUIC, TCP, Noise, Kademlia DHT, GossipSub |
| **Cryptography** | X25519 + ML-KEM-768 + ChaCha20-Poly1305 | Hybrid post-quantum security |
| **Storage** | SQLite (bundled) | DAG-based message store with encryption |
| **Async Runtime** | Tokio | Fearless concurrency |

For detailed architecture, see [ARCHITECTURE.md](docs/ARCHITECTURE.md).

## 🚀 Quick Start

### Prerequisites

- **Rust**: Latest stable or nightly (for 2024 edition)
- **OS**: Linux, macOS, or Windows
- **Dependencies**: `build-essential`, `pkg-config`, `libssl-dev` (Linux)

### Installation

```bash
# Clone the repository
git clone git@github.com:yourusername/project-swarm-daemon.git
cd project-swarm-daemon

# Build in release mode
cargo build --release

# Or build in debug mode (faster compilation)
cargo build
```

### Running

```bash
# Run the daemon
cargo run --release

# Or use the compiled binary
./target/release/project-swarm-daemon
```

**First run output**:
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

## 💬 Usage

### Basic Commands

| Command | Description | Example |
|---------|-------------|---------|
| `/invite` | Create private room with invite link | `/invite` |
| `/join <b64>` | Join a room using base64 invite | `/join eyJ0b3BpYyI6...` |
| `/history` | View last 50 messages | `/history` |
| `/discover` | Find peers on global DHT | `/discover` |
| `/connect <addr>` | Connect to peer by Multiaddr or PeerId | `/connect /ip4/...` |
| `/whisper <peer> <msg>` | Send E2EE private message | `/whisper 12D3KooW... Hi` |
| `<message>` | Broadcast to current room | `Hello swarm!` |

### Creating a Private Room

```
> /invite
--- YOUR DIRECT CONNECT INVITE ---
Give this command to your peer:
  /join eyJ0b3BpYyI6InN3YXJtLXJvb20tMWVhYjIzYyJ9...
----------------------------------
[SYSTEM] 🟡 You have moved to private room: 'swarm-room-1eab23c'. Waiting for peers...
```

### Joining a Room

```
> /join eyJ0b3BpYyI6InN3YXJtLXJvb20tMWVhYjIzYyJ9...
[NETWORK] Invite verified. Sender identity confirmed. Joining room: 'swarm-room-1eab23c'
```

### Sending Encrypted Whispers

```
> /whisper 12D3KooWLEqG... Hey, secret message!
[BROADCAST] 🟢 ML-KEM encrypted whisper sent to 12D3KooWLEqG...
```

## 📚 Documentation

Comprehensive documentation is available in the [docs/](docs/) directory:

| Document | Description |
|----------|-------------|
| [ARCHITECTURE.md](docs/ARCHITECTURE.md) | System design, components, data flow |
| [PROTOCOLS.md](docs/PROTOCOLS.md) | Network protocols, message formats, KEX/Sync |
| [CRYPTO.md](docs/CRYPTO.md) | Cryptography details, algorithms, threat model |
| [API.md](docs/API.md) | Internal API, data structures, module interfaces |
| [DEVELOPMENT.md](docs/DEVELOPMENT.md) | Build instructions, testing, contributing |

### Quick Links

- **Guiding Document**: See [Guiding Doc.md](Guiding%20Doc.md) for the master technical specification
- **libp2p Docs**: https://libp2p.io/docs/
- **Rust Book**: https://doc.rust-lang.org/book/

## 🛠️ Development

### Building from Source

```bash
# Debug build (fast compilation)
cargo build

# Release build (optimized)
cargo build --release

# Run tests
cargo test

# Check code style
cargo fmt --check
cargo clippy
```

### Project Structure

```
project-swarm-daemon/
├── src/
│   ├── main.rs      # Entry point, swarm event loop
│   ├── crypto.rs    # Post-quantum cryptography
│   ├── store.rs     # SQLite DAG storage
│   ├── sync.rs      # DAG synchronization protocol
│   └── kex.rs       # Key exchange protocol
├── docs/
│   ├── ARCHITECTURE.md
│   ├── PROTOCOLS.md
│   ├── CRYPTO.md
│   ├── API.md
│   └── DEVELOPMENT.md
├── Cargo.toml       # Rust dependencies
├── Guiding Doc.md   # Master specification
├── Dockerfile       # Container image
└── README.md       # This file
```

### Key Dependencies

```toml
[dependencies]
tokio = { version = "1.32", features = ["full"] }
libp2p = { version = "0.53", features = ["tokio", "quic", "tcp", "noise", "kad", "gossipsub", "request-response", "dcutr", "relay", "autonat", "upnp"] }
pqcrypto-mlkem = "0.1.1"          # Post-quantum KEM
x25519-dalek = "2.0"              # Classical ECDH
chacha20poly1305 = "0.10"         # AEAD cipher
rusqlite = { version = "0.31", features = ["bundled"] }  # SQLite
```

For detailed development instructions, see [DEVELOPMENT.md](docs/DEVELOPMENT.md).

## 🔒 Security

### Cryptographic Stack

| Layer | Algorithm | Key Size | Purpose |
|-------|-----------|----------|---------|
| **Protocol Identity** | Ed25519 | 32 bytes | PeerId, signatures |
| **Key Exchange (Classical)** | X25519 | 32 bytes | ECDH shared secret |
| **Key Exchange (PQ)** | ML-KEM-768 | 1184 pub, 2400 sec | NIST post-quantum KEM |
| **Symmetric Encryption** | ChaCha20-Poly1305 | 32 bytes | AEAD (confidentiality + integrity) |
| **Key Derivation** | HKDF-SHA256 | variable | NIST SP 800-56C |
| **Hashing** | SHA-256 | 32 bytes | Message IDs, DAG integrity |

### Security Features

- ✅ **Post-Quantum Safe**: ML-KEM-768 protects against quantum attacks
- ✅ **Forward Secrecy**: Ephemeral X25519 keys for each session
- ✅ **Hybrid Security**: Both classical and PQ algorithms must be broken
- ✅ **End-to-End Encryption**: Messages encrypted before leaving device
- ✅ **Signed Protocols**: KEX and invites signed with Ed25519
- ✅ **Encrypted Storage**: Local SQLite database encrypted at rest

### Threat Model

**Protected Against**:
- Eavesdropping (E2EE)
- Man-in-the-Middle (hybrid key exchange)
- Quantum attacks (ML-KEM)
- Message tampering (AEAD + DAG hashes)
- Replay attacks (nonces)
- Impersonation (Ed25519 signatures)

**Not Protected Against**:
- Endpoint compromise (malware on device)
- Traffic analysis (timing/size)
- Denial of service

For full details, see [CRYPTO.md](docs/CRYPTO.md).

## 🤝 Contributing

Contributions are welcome! Please follow these steps:

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'feat: add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

### Development Guidelines

- Follow Rust conventions (`cargo fmt`, `cargo clippy`)
- Add tests for new functionality
- Update documentation as needed
- Use [Conventional Commits](https://www.conventionalcommits.org/) for commit messages

For detailed development instructions, see [DEVELOPMENT.md](docs/DEVELOPMENT.md).

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🙏 Acknowledgments

- **libp2p**: For the robust P2P networking stack
- **Rust Community**: For the amazing ecosystem and crates
- **NIST**: For standardizing ML-KEM (FIPS 203)
- **PQClean**: For the post-quantum cryptographic implementations

## 📬 Contact

- **Issues**: [GitHub Issues](https://github.com/yourusername/project-swarm-daemon/issues)
- **Discussions**: [GitHub Discussions](https://github.com/yourusername/project-swarm-daemon/discussions)

---

**⚠️ Early Development Warning**: This project is in active development. Cryptographic implementations should be audited before production use.

**🔬 Experimental**: Post-quantum cryptography is still evolving. Stay updated with the latest security recommendations.

---

Made with 🦀 Rust and 🔐 cryptography
