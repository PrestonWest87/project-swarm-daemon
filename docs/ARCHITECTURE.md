# Project Swarm - Architecture Documentation

## Overview

Project Swarm is a post-quantum, peer-to-peer (P2P), masterless, locally hosted communication suite designed for high resilience, aggressive security, and complete independence from central cloud infrastructure.

## Core Architecture

### Masterless Mesh Topology

The network operates as a decentralized mesh of sovereign nodes (clients) with no central servers. The "server" is a shared state maintained by all connected peers.

```
┌─────────────────────────────────────────────────────────┐
│                    Swarm Mesh Network                    │
├─────────────────────────────────────────────────────────┤
│  Node A ◄──┐      ┌──► Node B      ┌──► Node C        │
│             │      │                │                    │
│  (Relay)    ├──► Node D ◄──┼──► Node E                │
│             │      │                │                    │
│  Node F ◄──┘      └──► Node G      └──► Node H        │
└─────────────────────────────────────────────────────────┘
```

### Administrative Authority

- **Genesis Node**: The user who creates the instance generates a unique Root Cryptographic Keypair
- **Administrative Actions**: Commands (kick user, delete messages, change permissions) are broadcast as cryptographic payloads signed by the Root Private Key
- **Swarm Consensus**: Every node verifies the signature against the Genesis Public Key and independently executes commands
- **Delegation**: Genesis Node can sign tokens granting administrative rights to other public keys

## Component Architecture

### Core Daemon (Rust)

The core daemon handles:
- P2P routing and peer discovery
- Cryptographic operations
- State management (DAG-based message storage)
- Protocol handling (KEX, Sync, GossipSub)

**Key Modules:**
- `main.rs` - Entry point, swarm initialization, event loop
- `crypto.rs` - Post-quantum hybrid cryptography (X25519 + ML-KEM)
- `store.rs` - SQLite-based DAG storage with encryption
- `sync.rs` - DAG synchronization protocol
- `kex.rs` - Key exchange protocol

### Data Flow

```
┌─────────────┐     ┌──────────────┐     ┌─────────────┐
│  User Input │────►│  Crypto      │────►│  GossipSub  │
│  (stdin)    │     │  (Encrypt)   │     │  (Broadcast)│
└─────────────┘     └──────────────┘     └─────────────┘
                                               │
                                               ▼
┌─────────────┐     ┌──────────────┐     ┌─────────────┐
│  SQLite DB  │◄────│  DAG Store   │◄────│  Peers      │
│  (Encrypted)│     │  (Save Msg)  │     │  (Receive)  │
└─────────────┘     └──────────────┘     └─────────────┘
```

## Cryptographic Architecture

### Hybrid Post-Quantum Security

```
┌─────────────────────────────────────────────────────────────┐
│              Encrypted Message Bundle                       │
├─────────────────────────────────────────────────────────────┤
│  Ephemeral X25519 Public Key (32 bytes)                    │
│  ML-KEM Ciphertext (1088 bytes)                            │
│  Nonce (12 bytes)                                          │
│  Encrypted Payload (variable)                              │
└─────────────────────────────────────────────────────────────┘
```

**Key Exchange Process:**
1. Classical: X25519 Diffie-Hellman
2. Post-Quantum: ML-KEM-768 (Kyber) encapsulation
3. Key Derivation: HKDF-SHA256 with combined secrets
4. Encryption: ChaCha20-Poly1305 AEAD

### Identity System

- **Protocol Identity**: Ed25519 keypair (libp2p PeerId)
- **Cryptographic Identity**: Hybrid X25519 + ML-KEM keypair
- **Storage Key**: Derived from cryptographic identity via HKDF

## Networking Architecture

### libp2p Stack

```
┌────────────────────────────────────────────────────────────┐
│                     Application Layer                      │
├────────────────────────────────────────────────────────────┤
│  GossipSub (Pub/Sub) │  KEX (Key Exchange) │ Sync (DAG)  │
├────────────────────────────────────────────────────────────┤
│  Identify │ mDNS │ Kademlia DHT │ AutoNAT │ DCUtR │ Relay │
├────────────────────────────────────────────────────────────┤
│              Transport Layer (QUIC, TCP)                   │
├────────────────────────────────────────────────────────────┤
│         Noise Protocol (Handshake) + Yamux (Mux)          │
└────────────────────────────────────────────────────────────┘
```

### Peer Discovery Mechanisms

1. **mDNS**: Local network discovery
2. **Kademlia DHT**: Global DHT for provider records
3. **Fat Invites**: Out-of-band bootstrapping with embedded Multiaddrs
4. **Identify Protocol**: Exchange listen addresses and protocols

### NAT Traversal

- **AutoNAT**: Automatic detection of NAT status
- **Relay**: Emergent relay servers (nodes with public IP promote themselves)
- **DCUtR**: Direct Connection Upgrade through Relay (hole punching)

## Storage Architecture

### DAG (Directed Acyclic Graph)

Messages are structured as a DAG where each message references its parent(s):

```
Message 1 ──────► Message 2 ──────► Message 3
                    │                   │
                    └─────► Message 4 ──┘
                          (fork/merge)
```

**DAG Properties:**
- Cryptographic hashing (SHA-256) for message IDs
- Conflict resolution via CRDT semantics
- Cryptographic integrity (each message signed implicitly via hash chain)

### SQLite Schema

```sql
-- Messages table (encrypted at rest)
CREATE TABLE messages (
    id TEXT PRIMARY KEY,           -- SHA-256 hash of message content + parents
    author TEXT NOT NULL,           -- PeerId of author
    parents TEXT NOT NULL,          -- JSON array of parent message IDs
    content_nonce BLOB NOT NULL,    -- ChaCha20-Poly1305 nonce
    content_ciphertext BLOB NOT NULL -- Encrypted message content
);

-- Peers table (public keys)
CREATE TABLE peers (
    peer_id TEXT PRIMARY KEY,       -- libp2p PeerId
    x25519_pub BLOB NOT NULL,       -- X25519 public key
    mlkem_pub BLOB NOT NULL,       -- ML-KEM public key
    signature BLOB NOT NULL         -- Ed25519 signature of keys
);
```

## Protocol Specifications

### Key Exchange Protocol (KEX)

**Protocol ID**: `/project-swarm/kex/1.0.0`

**Request:**
```rust
struct KexRequest {
    x25519_pub: Vec<u8>,    // 32 bytes
    mlkem_pub: Vec<u8>,     // 1184 bytes (ML-KEM-768)
    signature: Vec<u8>,     // Ed25519 signature of keys
}
```

**Response:**
```rust
struct KexResponse {
    x25519_pub: Vec<u8>,
    mlkem_pub: Vec<u8>,
    signature: Vec<u8>,
}
```

### DAG Sync Protocol

**Protocol ID**: `/project-swarm/sync/1.0.0`

**Request:**
```rust
struct SyncRequest {
    known_leaves: Vec<String>, // Hashes of latest known messages
}
```

**Response:**
```rust
struct SyncResponse {
    missing_messages: Vec<DagMessage>, // Messages peer doesn't have
}
```

### GossipSub Messaging

Messages are published to topics (rooms/channels) via GossipSub:
- **Topic Naming**: `swarm-room-{hex_4_bytes}` for private rooms
- **Default Topic**: `swarm-alpha` for public lobby
- **Message Types**:
  - `DagMessage` - Broadcast messages (plaintext, encrypted at app layer)
  - `EncryptedBundle` - Whisper messages (E2EE)

## Security Model

### Threat Model

**Protected Against:**
- Eavesdropping (all messages E2EE)
- Man-in-the-middle (ML-KEM + X25519 hybrid)
- "Harvest Now, Decrypt Later" (post-quantum crypto)
- Message tampering (DAG hash chain)
- Replay attacks (nonce-based encryption)
- Impersonation (Ed25519 signatures)

**Not Protected Against:**
- Endpoint compromise (malware on user device)
- Traffic analysis (timing, message size)
- Denial of service (network level)

### Cryptographic Primitives

| Purpose | Algorithm | Key Size | Notes |
|---------|-----------|----------|-------|
| Peer Identity | Ed25519 | 32 bytes | libp2p PeerId |
| Key Exchange (Classical) | X25519 | 32 bytes | ECDH |
| Key Exchange (PQ) | ML-KEM-768 | 1184 pub, 2400 sec | NIST standardized |
| Symmetric Encryption | ChaCha20-Poly1305 | 32 bytes | AEAD |
| Key Derivation | HKDF-SHA256 | variable | NIST SP 800-56C |
| Hashing | SHA-256 | 32 bytes | Message IDs |

## Scalability Considerations

### Current Limitations
- In-memory Kademlia store (not persistent)
- Single-threaded event loop (async but not parallel)
- SQLite for local storage (not distributed)

### Emergent Scaling
- Public nodes automatically become relay servers
- Blind store-and-forward for offline peers
- DAG allows merge of concurrent message branches
- Media pointers avoid forcing sync of large files

## Future Architecture (Planned)

### Tauri + React/Svelte Desktop GUI
- Native OS webviews
- Rust backend shared with daemon
- Minimal RAM footprint vs Electron

### Ratatui TUI
- Fully featured terminal interface
- Direct API access to core daemon
- Keyboard-driven workflow

### MLS Integration
- Messaging Layer Security for group encryption
- Ratchet tree for efficient group key management
- Forward secrecy with automatic key rotation
