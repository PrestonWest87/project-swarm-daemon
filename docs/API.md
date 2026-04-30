# Project Swarm - API Documentation

This document describes the internal API, data structures, and module interfaces for Project Swarm.

## Table of Contents
- [Overview](#overview)
- [Core Modules](#core-modules)
- [Data Structures](#data-structures)
- [Public Interfaces](#public-interfaces)
- [Database API](#database-api)
- [Crypto API](#crypto-api)

## Overview

Project Swarm is structured as a Rust binary crate with modular components:

```
src/
├── main.rs      # Entry point, swarm event loop
├── crypto.rs    # Post-quantum cryptography
├── store.rs     # SQLite DAG storage
├── sync.rs      # DAG synchronization protocol
└── kex.rs       # Key exchange protocol
```

## Core Modules

### main.rs

**Purpose**: Application entry point, libp2p swarm initialization, event loop

**Key Components**:
- `SwarmProtocol` - Network behaviour struct (derived from `#[derive(NetworkBehaviour)]`)
- `FatInvite` - Invite link structure
- `main()` - Async entry point with Tokio runtime

**Public Items**:
```rust
mod store;
mod sync;
mod crypto;
mod kex;

struct FatInvite {
    topic: String,
    addrs: Vec<String>,
    sender_pubkey: Vec<u8>,
    signature: Vec<u8>,
}

#[derive(NetworkBehaviour)]
struct SwarmProtocol {
    gossipsub: gossipsub::Behaviour,
    mdns: mdns::tokio::Behaviour,
    kademlia: kad::Behaviour<kad::store::MemoryStore>,
    req_res: request_response::cbor::Behaviour<SyncRequest, SyncResponse>,
    kex: request_response::cbor::Behaviour<KexRequest, KexResponse>,
    identify: identify::Behaviour,
    autonat: autonat::Behaviour,
    dcutr: dcutr::Behaviour,
    relay_client: relay::client::Behaviour,
    relay_server: relay::Behaviour,
    upnp: upnp::tokio::Behaviour,
}
```

### crypto.rs

**Purpose**: Post-quantum hybrid cryptography (X25519 + ML-KEM + ChaCha20-Poly1305)

**Public Structs**:
```rust
pub struct HybridIdentity {
    pub x25519_secret: StaticSecret,
    pub x25519_public: X25519PublicKey,
    pub mlkem_secret: mlkem768::SecretKey,
    pub mlkem_public: mlkem768::PublicKey,
}

pub struct StoredEncrypted {
    pub nonce: [u8; 12],
    pub ciphertext: Vec<u8>,
}

pub struct EncryptedBundle {
    pub ephemeral_x25519: [u8; 32],
    pub pq_ciphertext: Vec<u8>,
    pub nonce: [u8; 12],
    pub encrypted_payload: Vec<u8>,
}
```

**Public Functions**:
```rust
impl HybridIdentity {
    pub fn generate() -> Self;
    pub fn derive_storage_key(&self) -> [u8; 32];
}

pub fn encrypt_for_storage(plaintext: &[u8], key: &[u8; 32]) -> StoredEncrypted;
pub fn decrypt_for_storage(encrypted: &StoredEncrypted, key: &[u8; 32]) -> Result<Vec<u8>, &'static str>;

pub fn seal_payload(
    plaintext: &[u8],
    recipient_x25519_pub: &X25519PublicKey,
    recipient_mlkem_pub: &mlkem768::PublicKey,
) -> EncryptedBundle;

pub fn open_payload(
    bundle: &EncryptedBundle,
    my_identity: &HybridIdentity,
) -> Result<Vec<u8>, &'static str>;

pub fn seal_for_network(
    plaintext: &[u8],
    recipient_x25519_bytes: &[u8],
    recipient_mlkem_bytes: &[u8],
) -> Result<EncryptedBundle, &'static str>;
```

### store.rs

**Purpose**: SQLite-based DAG storage with encryption at rest

**Public Structs**:
```rust
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct DagMessage {
    pub id: String,          // SHA-256 hash (hex)
    pub author: String,      // PeerId of author
    pub parents: Vec<String>, // Parent message IDs
    pub content: String,     // Message content (plaintext, encrypted at rest)
}

pub struct Store {
    conn: Connection,
    storage_key: [u8; 32],
}
```

**Public Methods**:
```rust
impl DagMessage {
    pub fn new(author: String, parents: Vec<String>, content: String) -> Self;
    pub fn calculate_hash(&self) -> String;
}

impl Store {
    pub fn new(storage_key: [u8; 32]) -> Result<Self>;
    pub fn save_message(&self, msg: &DagMessage) -> Result<()>;
    pub fn save_peer_keys(&self, peer_id: &str, x25519_pub: &[u8], mlkem_pub: &[u8], signature: &[u8]) -> Result<()>;
    pub fn get_peer_keys(&self, peer_id: &str) -> Result<Option<(Vec<u8>, Vec<u8>)>>;
    pub fn get_recent_messages(&self, limit: u32) -> Result<Vec<DagMessage>>;
    pub fn get_messages_after(&self, known_leaves: &[String]) -> Result<Vec<DagMessage>>;
    pub fn get_latest_leaves(&self) -> Result<Vec<String>>;
}
```

### sync.rs

**Purpose**: DAG synchronization protocol definitions

**Public Constants**:
```rust
pub const SYNC_PROTOCOL_NAME: StreamProtocol = StreamProtocol::new("/project-swarm/sync/1.0.0");
```

**Public Structs**:
```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncRequest {
    pub known_leaves: Vec<String>, // Hashes of the most recent messages we have
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncResponse {
    pub missing_messages: Vec<DagMessage>, // The blocks the peer needs
}
```

### kex.rs

**Purpose**: Key exchange protocol definitions

**Public Constants**:
```rust
pub const KEX_PROTOCOL_NAME: StreamProtocol = StreamProtocol::new("/project-swarm/kex/1.0.0");
```

**Public Structs**:
```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KexRequest {
    pub x25519_pub: Vec<u8>,
    pub mlkem_pub: Vec<u8>,
    pub signature: Vec<u8>, // Ed25519 signature of the keys
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KexResponse {
    pub x25519_pub: Vec<u8>,
    pub mlkem_pub: Vec<u8>,
    pub signature: Vec<u8>, // Ed25519 signature of the keys
}
```

## Data Structures

### DagMessage

Represents a single message in the DAG (Directed Acyclic Graph).

**Fields**:
- `id: String` - SHA-256 hash of (author + parents + content), hex encoded
- `author: String` - PeerId of the message author
- `parents: Vec<String>` - Vector of parent message IDs (usually 1, but can fork/merge)
- `content: String` - Message text content

**Example**:
```rust
let msg = DagMessage {
    id: "a1b2c3d4e5f6...".to_string(),
    author: "12D3KooW...".to_string(),
    parents: vec!["previous_hash...".to_string()],
    content: "Hello, swarm!".to_string(),
};
```

**Hash Calculation**:
```rust
pub fn calculate_hash(&self) -> String {
    let mut hasher = Sha256::new();
    hasher.update(&self.author);
    for p in &self.parents {
        hasher.update(p);
    }
    hasher.update(&self.content);
    hex::encode(hasher.finalize())
}
```

### EncryptedBundle

End-to-end encrypted message format for whisper (private) messages.

**Fields**:
- `ephemeral_x25519: [u8; 32]` - Sender's ephemeral X25519 public key
- `pq_ciphertext: Vec<u8>` - ML-KEM-768 ciphertext (1088 bytes)
- `nonce: [u8; 12]` - ChaCha20-Poly1305 nonce
- `encrypted_payload: Vec<u8>` - Encrypted message content

**Serialization**: JSON (via serde)

### FatInvite

Invite link structure for out-of-band room joining.

**Fields**:
- `topic: String` - GossipSub topic (room identifier)
- `addrs: Vec<String>` - Multiaddrs with PeerId for direct dialing
- `sender_pubkey: Vec<u8>` - Ed25519 public key (for signature verification)
- `signature: Vec<u8>` - Ed25519 signature of the invite

**Encoding**: Base64 of JSON-serialized struct

## Public Interfaces

### CLI Commands

The daemon accepts commands via stdin:

| Command | Description | Example |
|---------|-------------|---------|
| `/invite` | Create a new private room and generate invite | `/invite` |
| `/join <b64>` | Join a room using base64 invite | `/join eyJ0b3BpYyI6InN3YXJt...` |
| `/history` | View last 50 messages from local DAG | `/history` |
| `/discover` | Find peers on the global DHT | `/discover` |
| `/connect <addr>` | Connect to specific peer by Multiaddr or PeerId | `/connect /ip4/.../p2p/...` |
| `/whisper <peer> <msg>` | Send encrypted private message | `/whisper 12D3KooW... Hello` |
| `<message>` | Send broadcast message to current room | `Hello, everyone!` |

### Event Loop Interface

The main event loop processes two types of events:

1. **User Input** (stdin):
```rust
tokio::select! {
    Ok(Some(line)) = stdin.next_line() => {
        // Parse and handle CLI commands
    }
}
```

2. **Swarm Events** (libp2p):
```rust
event = swarm.select_next_some() => match event {
    SwarmEvent::Behaviour(SwarmProtocolEvent::Gossipsub(...)) => { /* messages */ }
    SwarmEvent::Behaviour(SwarmProtocolEvent::Kex(...)) => { /* key exchange */ }
    SwarmEvent::Behaviour(SwarmProtocolEvent::ReqRes(...)) => { /* sync */ }
    SwarmEvent::ConnectionEstablished { ... } => { /* new peer */ }
    _ => {}
}
```

## Database API

### Connection Management

```rust
// Create/open database with encryption key
let storage_key = my_crypto_id.derive_storage_key();
let db = Store::new(storage_key)?;

// Database file: swarm_dag.db (SQLite)
```

### Message Operations

**Save Message**:
```rust
let msg = DagMessage::new(author, parents, content);
db.save_message(&msg)?;
```

**Get Recent Messages**:
```rust
let messages = db.get_recent_messages(50)?;
for msg in messages {
    println!("[{}] (Hash: {}): {}", msg.author, msg.id, msg.content);
}
```

**Get Messages After Known Leaves** (for sync):
```rust
let known_leaves = db.get_latest_leaves()?;
let missing = db.get_messages_after(&known_leaves)?;
```

### Peer Key Operations

**Save Peer Keys** (after KEX):
```rust
db.save_peer_keys(
    &peer_id.to_string(),
    &kex_response.x25519_pub,
    &kex_response.mlkem_pub,
    &kex_response.signature,
)?;
```

**Get Peer Keys** (for whisper):
```rust
if let Some((x25519_pub, mlkem_pub)) = db.get_peer_keys(&target_peer_str)? {
    let bundle = crypto::seal_for_network(message.as_bytes(), &x25519_pub, &mlkem_pub)?;
    // Send bundle...
}
```

## Crypto API

### Key Generation

```rust
// Generate hybrid identity (X25519 + ML-KEM)
let my_crypto_id = crypto::HybridIdentity::generate();

// Derive storage encryption key
let storage_key = my_crypto_id.derive_storage_key();
```

### Whisper Encryption/Decryption

**Encrypt for specific peer**:
```rust
let bundle = crypto::seal_for_network(
    message_bytes,
    &recipient_x25519_bytes,
    &recipient_mlkem_bytes,
)?;

// Serialize and send via GossipSub
let payload = serde_json::to_vec(&bundle)?;
swarm.behaviour_mut().gossipsub.publish(topic, payload);
```

**Decrypt received whisper**:
```rust
if let Ok(bundle) = serde_json::from_slice::<EncryptedBundle>(&message.data) {
    if let Ok(decrypted) = crypto::open_payload(&bundle, &my_crypto_id) {
        let text = String::from_utf8_lossy(&decrypted);
        println!("[WHISPER] From [{}]: {}", sender, text);
    }
}
```

### Storage Encryption

**Encrypt before saving to DB**:
```rust
let encrypted = crypto::encrypt_for_storage(message_content.as_bytes(), &storage_key);
// Save encrypted.nonce and encrypted.ciphertext to SQLite
```

**Decrypt after reading from DB**:
```rust
let encrypted = StoredEncrypted { nonce, ciphertext };
let decrypted = crypto::decrypt_for_storage(&encrypted, &storage_key)?;
let content = String::from_utf8(decrypted)?;
```

## Error Handling

### Common Error Types

**Crypto Errors**:
- `storage decryption failed` - Wrong key or corrupted data
- `Decryption failed` - Invalid key, corrupted payload, or tampered data
- `Invalid ML-KEM ciphertext format` - Malformed PQ ciphertext
- `Invalid X25519 key length` - Wrong key size

**Storage Errors**:
- `rusqlite::Error` - SQLite operation failed
- `serde_json::Error` - JSON serialization/deserialization failed

**Network Errors**:
- `libp2p::swarm::SwarmBuilderError` - Swarm setup failed
- `libp2p::TransportError` - Connection failed
- `gossipsub::PublishError` - Message publish failed

### Error Propagation

Most functions return `Result<T, E>` with:
- `Box<dyn Error>` for main()
- `rusqlite::Result<T>` for store operations
- `&'static str` for crypto operations (simple error messages)

## Thread Safety

### Concurrency Model

- **Main thread**: Tokio async runtime, single-threaded event loop
- **Blocking operations**: `tokio::task::spawn_blocking` for SQLite and crypto
- **Shared state**: `Arc<Mutex<Store>>` for database access

**Example**:
```rust
let db = Arc::new(Mutex::new(Store::new(storage_key)?));

// In async context, spawn blocking:
let db_clone = Arc::clone(&db);
tokio::task::spawn_blocking(move || {
    db_clone.lock().unwrap().save_message(&msg);
}).await?;
```

### Mutex Usage

The `Store` is wrapped in `Mutex` because:
- SQLite `Connection` is not `Send` (cannot cross await points)
- Multiple async tasks may access the database
- `spawn_blocking` ensures blocking I/O doesn't stall the runtime

## Configuration

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `RUST_LOG` | Tracing filter | `debug,libp2p_mdns=off,...` |

### Compile-Time Configuration

**Cargo.toml features**:
```toml
[dependencies]
tokio = { version = "1.32", features = ["full"] }
libp2p = { version = "0.53", features = ["tokio", "quic", "tcp", "noise", "yamux", "kad", "mdns", "macros", "gossipsub", "request-response", "cbor", "identify", "autonat", "dcutr", "relay", "upnp", "dns", "rsa"] }
```

### Runtime Configuration

**Listening ports**: Fixed at 4001 (UDP/QUIC and TCP)

**Logging**:
- Output: `swarm_daemon.log` (file, non-blocking)
- Format: Plain text, no ANSI colors
- Filter: Configurable via `EnvFilter`

## Future API Extensions

### Planned Public API

For GUI/TUI integration, the daemon will expose:

```rust
// TCP/Unix socket API (future)
pub enum DaemonCommand {
    SendMessage { content: String, room: String },
    SendWhisper { peer: PeerId, content: String },
    CreateRoom { name: String } -> RoomId,
    JoinRoom { invite: String },
    GetMessages { limit: u32 } -> Vec<DagMessage>,
    GetPeers {} -> Vec<PeerInfo>,
}

pub enum DaemonEvent {
    MessageReceived { message: DagMessage },
    PeerConnected { peer: PeerId },
    PeerDisconnected { peer: PeerId },
    RoomJoined { room: String },
}
```

### FFI Interface (Future)

For language bindings (C, Python, etc.):
```rust
#[no_mangle]
pub extern "C" fn swarm_init() -> *mut DaemonHandle;

#[no_mangle]
pub extern "C" fn swarm_send_message(handle: *mut DaemonHandle, msg: *const c_char) -> c_int;
```
