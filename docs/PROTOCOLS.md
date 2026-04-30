# Project Swarm - Protocol Documentation

This document details the network protocols, message formats, and communication patterns used in Project Swarm.

## Table of Contents
- [Network Stack](#network-stack)
- [libp2p Protocols](#libp2p-protocols)
- [Custom Protocols](#custom-protocols)
- [Message Formats](#message-formats)
- [Room Management](#room-management)

## Network Stack

Project Swarm uses libp2p as its networking foundation, leveraging multiple protocols for different purposes.

### Transport Layer
- **QUIC** (primary): `/ip4/0.0.0.0/udp/{port}/quic-v1`
- **TCP** (fallback): `/ip4/0.0.0.0/tcp/{port}`
- **Noise Protocol**: Handshake encryption
- **Yamux**: Connection multiplexing

### Default Port
The daemon listens on port **4001** by default (both UDP/QUIC and TCP).

## libp2p Protocols

### 1. GossipSub (Pub/Sub Messaging)

**Purpose**: Broadcast messages to all peers in a "topic" (room/channel)

**Configuration**:
- Heartbeat interval: 5 seconds
- Validation mode: Strict
- Message ID: Hash-based (`DefaultHasher`)

**Topic Naming**:
- Default lobby: `swarm-alpha`
- Private rooms: `swarm-room-{8 hex chars}` (e.g., `swarm-room-1a2b3c4d`)

**Usage**:
```rust
// Subscribe to a topic
let topic = gossipsub::IdentTopic::new("swarm-alpha");
swarm.behaviour_mut().gossipsub.subscribe(&topic);

// Publish a message
let payload = serde_json::to_vec(&dag_message).unwrap();
swarm.behaviour_mut().gossipsub.publish(topic, payload);
```

### 2. Kademlia DHT

**Purpose**: Distributed hash table for peer discovery and content routing

**Configuration**:
- Query timeout: 15 seconds
- Replication factor: 2
- Mode: Server (enables providing records)

**Rendezvous Key**: `project-swarm-rendezvous-v1`

**Usage Patterns**:
```rust
// Start providing a room (become a seeder)
let room_key = kad::RecordKey::new(&room_topic);
swarm.behaviour_mut().kademlia.start_providing(room_key);

// Find providers for a room
swarm.behaviour_mut().kademlia.get_providers(room_key);

// Bootstrap from public nodes
swarm.behaviour_mut().kademlia.bootstrap();
```

**Bootstrap Nodes** (IPFS public DHT):
```
/dnsaddr/bootstrap.libp2p.io/p2p/QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN
/dnsaddr/bootstrap.libp2p.io/p2p/QmQCU2EcMqAqQPR2i9bChDtGNJchTbq5TbXBPxW8V92uMb
/ip4/104.131.131.82/tcp/4001/p2p/QmaCpDMGvV2BGHeYERUEnRQAwe3N8SzbUtfsmvsqQLuvuJ
```

### 3. mDNS (Local Discovery)

**Purpose**: Discover peers on the local network without DHT

**Configuration**: Default mDNS settings with tokio runtime

**Automatic**: Peers on the same LAN are discovered automatically.

### 4. Identify Protocol

**Purpose**: Exchange peer information (addresses, protocols, observed IP)

**Protocol ID**: `/project-swarm/1.0.0`

**Exchanged Information**:
- Listen addresses
- Supported protocols
- Observed address (NAT detection)
- Agent version string

### 5. AutoNAT

**Purpose**: Automatically detect if node is behind NAT or has public IP

**Event Handling**:
```rust
SwarmEvent::Behaviour(SwarmProtocolEvent::Autonat(autonat::Event::StatusChanged { old, new })) => {
    if let autonat::NatStatus::Public(addr) = new {
        // Promote to relay server
    }
}
```

**Status Types**:
- `Unknown`: Initial state
- `Private`: Behind NAT
- `Public(addr)`: Has public IP, can accept incoming connections

### 6. DCUtR (Direct Connection Upgrade through Relay)

**Purpose**: Hole-punch through NATs to establish direct peer-to-peer connections

**Process**:
1. Peers connect via relay
2. DCUtR negotiates direct connection
3. Relay is dropped once direct connection established

### 7. Relay (Circuit Relay)

**Purpose**: Route traffic for NAT-trapped peers

**Components**:
- **Relay Client**: Can connect through relays
- **Relay Server**: Can act as relay for others (if public IP detected)

**Emergent Behavior**: Nodes with public IP automatically become relay servers.

### 8. UPnP

**Purpose**: Automatically configure port forwarding on NAT routers

**Automatic**: Attempts to open port 4001 on the local router.

## Custom Protocols

### 1. Key Exchange Protocol (KEX)

**Protocol ID**: `/project-swarm/kex/1.0.0`

**Purpose**: Exchange post-quantum cryptographic keys between peers

**Request Format**:
```rust
struct KexRequest {
    x25519_pub: Vec<u8>,    // 32 bytes - X25519 public key
    mlkem_pub: Vec<u8>,     // 1184 bytes - ML-KEM-768 public key
    signature: Vec<u8>,     // Ed25519 signature of (x25519_pub + mlkem_pub)
}
```

**Response Format**:
```rust
struct KexResponse {
    x25519_pub: Vec<u8>,    // 32 bytes
    mlkem_pub: Vec<u8>,     // 1184 bytes
    signature: Vec<u8>,     // Ed25519 signature
}
```

**Protocol Flow**:
```
Peer A                              Peer B
  │                                   │
  ├──── KexRequest(signed keys) ────►│
  │                                   ├─ Verify signature
  │                                   ├─ Save peer keys to DB
  │                                   │
  │◄─── KexResponse(signed keys) ────┤
  ├─ Verify signature                 │
  ├─ Save peer keys to DB             │
  │                                   │
  ├──── SyncRequest(known leaves) ───►│
  │◄─── SyncResponse(missing msgs) ──┤
  │                                   │
```

**Signature Verification**:
```rust
let mut payload = request.x25519_pub.clone();
payload.extend_from_slice(&request.mlkem_pub);
let pub_key = libp2p::identity::PublicKey::try_decode_protobuf(&peer_bytes)?;
pub_key.verify(&payload, &request.signature)?;
```

### 2. DAG Synchronization Protocol

**Protocol ID**: `/project-swarm/sync/1.0.0`

**Purpose**: Synchronize message DAG between peers

**Request Format**:
```rust
struct SyncRequest {
    known_leaves: Vec<String>, // Hash IDs of latest known messages
}
```

**Response Format**:
```rust
struct SyncResponse {
    missing_messages: Vec<DagMessage>, // Messages the requester doesn't have
}
```

**Synchronization Logic**:
1. Requester sends hashes of their latest messages (leaves)
2. Responder finds messages with `rowid > max_rowid_of_known_leaves`
3. Responder sends missing messages
4. Requester saves missing messages to local DAG

**Implementation**:
```rust
// Request sync
let known_leaves = db.lock().unwrap().get_latest_leaves()?;
swarm.behaviour_mut().req_res.send_request(&peer, SyncRequest { known_leaves });

// Handle response
SwarmEvent::Behaviour(SwarmProtocolEvent::ReqRes(event)) => {
    match event {
        request_response::Message::Response { response, .. } => {
            for msg in response.missing_messages {
                db.lock().unwrap().save_message(&msg);
            }
        }
        _ => {}
    }
}
```

## Message Formats

### 1. DagMessage (Broadcast Messages)

**Purpose**: Plaintext messages in the DAG (encrypted at storage layer)

**Format**:
```rust
struct DagMessage {
    id: String,           // SHA-256 hash (hex encoded)
    author: String,       // PeerId of author
    parents: Vec<String>, // Parent message hashes
    content: String,      // Message text (plaintext in transit, encrypted at rest)
}
```

**ID Calculation**:
```rust
fn calculate_hash(&self) -> String {
    let mut hasher = Sha256::new();
    hasher.update(&self.author);
    for p in &self.parents {
        hasher.update(p);
    }
    hasher.update(&self.content);
    hex::encode(hasher.finalize())
}
```

**Serialization**: JSON (via serde)

### 2. EncryptedBundle (Whisper Messages)

**Purpose**: End-to-end encrypted private messages

**Format**:
```rust
struct EncryptedBundle {
    ephemeral_x25519: [u8; 32],     // Sender's ephemeral X25519 public key
    pq_ciphertext: Vec<u8>,         // ML-KEM ciphertext (1088 bytes)
    nonce: [u8; 12],                // ChaCha20-Poly1305 nonce
    encrypted_payload: Vec<u8>,     // Encrypted message content
}
```

**Encryption Process** (see [crypto.md](CRYPTTO.md) for details):
1. Generate ephemeral X25519 keypair
2. Perform X25519 ECDH with recipient's static X25519 public key
3. Encapsulate ML-KEM shared secret to recipient's ML-KEM public key
4. Derive symmetric key: HKDF-SHA256(classical_secret || pq_secret)
5. Encrypt payload: ChaCha20-Poly1305(key, nonce, plaintext)

**Decryption Process**:
1. Compute classical secret: X25519(receiver_secret, sender_ephemeral)
2. Decapsulate PQ secret: ML-KEM(decrypt ciphertext with receiver_secret)
3. Derive symmetric key: HKDF-SHA256(combined_secrets)
4. Decrypt: ChaCha20-Poly1305(key, nonce, ciphertext)

## Room Management

### Room Creation (Invite)

**Command**: `/invite`

**Process**:
1. Generate random room code (4 bytes, hex encoded)
2. Create topic: `swarm-room-{room_code}`
3. Unsubscribe from current topic
4. Subscribe to new topic
5. Start providing topic on DHT
6. Collect local and external addresses
7. Create signed `FatInvite` struct
8. Encode to base64 for sharing

**FatInvite Format**:
```rust
struct FatInvite {
    topic: String,           // Room topic string
    addrs: Vec<String>,      // Multiaddrs with PeerId (direct dial)
    sender_pubkey: Vec<u8>,  // Ed25519 public key (for signature verification)
    signature: Vec<u8>,      // Ed25519 signature of JSON-serialized invite
}
```

### Room Joining

**Command**: `/join {base64_invite}`

**Process**:
1. Decode base64 to JSON
2. Parse `FatInvite` struct
3. Verify Ed25519 signature
4. Unsubscribe from current topic
5. Subscribe to invite topic
6. Start providing topic on DHT
7. Directly dial Multiaddrs from invite
8. Query DHT for additional providers

**Signature Verification**:
```rust
let mut invite_copy = invite_data.clone();
invite_copy.signature = Vec::new();
let payload = serde_json::to_string(&invite_copy).unwrap();

let pub_key = libp2p::identity::PublicKey::try_decode_protobuf(&invite_data.sender_pubkey)?;
pub_key.verify(payload.as_bytes(), &invite_data.signature)?;
```

## Peer Discovery Flow

### 1. Bootstrap (First Run)
```
1. Connect to IPFS bootstrap nodes (DHT seeders)
2. Bootstrap Kademlia (find closest peers)
3. Start providing rendezvous key
4. Discover peers via mDNS (local) and DHT (global)
```

### 2. Normal Operation
```
1. Listen for incoming connections (QUIC + TCP)
2. mDNS discovers local peers automatically
3. Identify protocol exchanges addresses
4. KEX protocol exchanges crypto keys
5. Sync protocol synchronizes DAG
6. GossipSub broadcasts messages
```

### 3. NAT Traversal
```
1. AutoNAT detects NAT status
2. If public: promote to relay server
3. If private: use relays + DCUtR for hole punching
4. UPnP attempts automatic port forwarding
```

## Error Handling

### Connection Errors
- **Timeout**: Routine, logged as debug
- **Connection Refused**: Peer offline, logged as debug
- **Handshake Failed**: Incompatible protocols, logged as warning
- **WrongPeerId**: Dialed wrong peer, logged as debug

### Cryptographic Errors
- **Invalid Signature**: Peer rejected, warning logged
- **Decryption Failed**: Message dropped, error logged
- **Invalid Key Format**: Peer rejected, error logged

### Storage Errors
- **SQLite Write Failed**: Message not saved, error logged
- **Encryption Failed**: Message not stored, error logged

## Logging

The daemon uses the `tracing` crate with the following configuration:
- **Log File**: `swarm_daemon.log` (rolling, never rotates)
- **Format**: Plain text, no ANSI colors
- **Filters**:
  - `debug` (global)
  - `libp2p_mdns=off`
  - `libp2p_kad=trace`
  - `libp2p_swarm=trace`
  - `libp2p_quic=debug`
  - `libp2p_tcp=debug`
  - `project_swarm_daemon=trace`

**Environment Variable**: Set `RUST_LOG` to override filters.
