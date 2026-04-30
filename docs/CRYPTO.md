# Project Swarm - Cryptography Documentation

This document details the cryptographic protocols, algorithms, and implementations used in Project Swarm.

## Table of Contents
- [Overview](#overview)
- [Cryptographic Identity](#cryptographic-identity)
- [Hybrid Key Exchange](#hybrid-key-exchange)
- [Encryption/Decryption](#encryptiondecryption)
- [Storage Encryption](#storage-encryption)
- [Signature Verification](#signature-verification)
- [Threat Model](#threat-model)

## Overview

Project Swarm uses a **hybrid post-quantum cryptographic scheme** combining:
- **Classical**: X25519 (Elliptic Curve Diffie-Hellman)
- **Post-Quantum**: ML-KEM-768 (formerly Kyber768, NIST standardized)
- **Symmetric**: ChaCha20-Poly1305 (AEAD)
- **Hashing**: SHA-256, HKDF-SHA256
- **Signatures**: Ed25519 (libp2p identity)

This provides protection against both classical and quantum computer attacks.

## Cryptographic Identity

Each node maintains two separate identities:

### 1. Protocol Identity (libp2p)
- **Algorithm**: Ed25519
- **Key Size**: 32 bytes (public), 64 bytes (private)
- **Purpose**: Peer identification, protocol-level signing
- **Storage**: `swarm_network_key.bin` (protobuf encoding)

```rust
// Generated/loaded in main.rs
let local_key = identity::Keypair::generate_ed25519();
// or loaded from disk
let local_key = identity::Keypair::from_protobuf_encoding(&bytes)?;
```

**PeerId Derivation**:
```rust
let local_peer_id = PeerId::from(local_key.public());
// Example: PeerId = "12D3KooW..."
```

### 2. Cryptographic Identity (Hybrid)
- **Components**: X25519 + ML-KEM-768 keypair
- **Purpose**: End-to-end encryption, key exchange
- **Storage**: In-memory (ephemeral, generated at startup)

```rust
pub struct HybridIdentity {
    pub x25519_secret: StaticSecret,     // 32 bytes
    pub x25519_public: X25519PublicKey, // 32 bytes
    pub mlkem_secret: mlkem768::SecretKey, // 2400 bytes
    pub mlkem_public: mlkem768::PublicKey, // 1184 bytes
}
```

**Generation**:
```rust
impl HybridIdentity {
    pub fn generate() -> Self {
        let x25519_secret = StaticSecret::random_from_rng(OsRng);
        let x25519_public = X25519PublicKey::from(&x25519_secret);
        let (mlkem_public, mlkem_secret) = mlkem768::keypair();
        
        Self { x25519_secret, x25519_public, mlkem_secret, mlkem_public }
    }
}
```

### Storage Key Derivation

The SQLite database encryption key is derived from the cryptographic identity:

```rust
pub fn derive_storage_key(&self) -> [u8; 32] {
    let mut key_input = self.x25519_secret.to_bytes().to_vec();
    key_input.extend_from_slice(self.mlkem_secret.as_bytes());
    let hkdf = Hkdf::<Sha256>::new(None, &key_input);
    let mut key = [0u8; 32];
    hkdf.expand(b"storage-key-v1", &mut key).expect("HKDF expand failed");
    key
}
```

**Process**:
1. Concatenate X25519 secret (32 bytes) + ML-KEM secret (2400 bytes)
2. Use as input keying material (IKM) for HKDF-SHA256
3. Expand with salt=None and info="storage-key-v1"
4. Output: 32-byte ChaCha20-Poly1305 key

## Hybrid Key Exchange

When two peers connect, they perform a hybrid key exchange to establish shared secrets.

### Key Exchange Protocol (KEX)

**Trigger**: After Identify protocol completes and peer supports KEX

**Protocol Flow**:
```
Peer A                              Peer B
  │                                   │
  ├─ Generate ephemeral keys          │
  │                                   │
  ├──── KexRequest ──────────────────►│
  │   (X25519 pub, ML-KEM pub,       │
  │    signed with Ed25519)           │
  │                                   ├─ Verify Ed25519 signature
  │                                   ├─ Save peer keys to DB
  │                                   │
  │◄─── KexResponse ─────────────────┤
  │   (X25519 pub, ML-KEM pub,       │
  │    signed with Ed25519)           │
  ├─ Verify Ed25519 signature         │
  ├─ Save peer keys to DB             │
  │                                   │
  ├─ Both peers now have each         │
  │  other's X25519 and ML-KEM keys   │
  │                                   │
```

**Request/Response Format**:
```rust
struct KexRequest {
    x25519_pub: Vec<u8>,    // 32 bytes
    mlkem_pub: Vec<u8>,     // 1184 bytes
    signature: Vec<u8>,     // 64 bytes (Ed25519)
}

struct KexResponse {
    x25519_pub: Vec<u8>,
    mlkem_pub: Vec<u8>,
    signature: Vec<u8>,
}
```

**Signature Creation**:
```rust
// On sender side
let mut payload_to_sign = my_crypto_id.x25519_public.to_bytes().to_vec();
payload_to_sign.extend_from_slice(my_crypto_id.mlkem_public.as_bytes());
let signature = local_key.sign(&payload_to_sign); // Ed25519 sign
```

**Signature Verification**:
```rust
// On receiver side
let mut verify_payload = request.x25519_pub.clone();
verify_payload.extend_from_slice(&request.mlkem_pub);

// Extract Ed25519 public key from PeerId
let pub_key = libp2p::identity::PublicKey::try_decode_protobuf(&peer.to_bytes()[2..])?;
pub_key.verify(&verify_payload, &request.signature)?;
```

## Encryption/Decryption

### Whisper Messages (End-to-End Encrypted)

Private messages between two peers use the `EncryptedBundle` format.

**Bundle Structure**:
```rust
struct EncryptedBundle {
    ephemeral_x25519: [u8; 32],     // Sender's ephemeral X25519 public key
    pq_ciphertext: Vec<u8>,         // ML-KEM ciphertext (1088 bytes)
    nonce: [u8; 12],                // ChaCha20-Poly1305 nonce
    encrypted_payload: Vec<u8>,     // Encrypted message
}
```

### Encryption Process (`seal_payload`)

```rust
pub fn seal_payload(
    plaintext: &[u8],
    recipient_x25519_pub: &X25519PublicKey,
    recipient_mlkem_pub: &mlkem768::PublicKey,
) -> EncryptedBundle {
    // Step 1: Generate ephemeral X25519 keypair
    let ephemeral_secret = EphemeralSecret::random_from_rng(OsRng);
    let ephemeral_public = X25519PublicKey::from(&ephemeral_secret);
    
    // Step 2: Classical ECDH
    let classical_shared_secret = ephemeral_secret.diffie_hellman(recipient_x25519_pub);
    
    // Step 3: Post-quantum KEM
    let (pq_shared_secret, pq_ciphertext) = mlkem768::encapsulate(recipient_mlkem_pub);
    
    // Step 4: Combine secrets with HKDF
    let hkdf = Hkdf::<Sha256>::new(None, classical_shared_secret.as_bytes());
    let mut derived_key = [0u8; 32];
    hkdf.expand(pq_shared_secret.as_bytes(), &mut derived_key).expect("HKDF expansion failed");
    
    // Step 5: Encrypt with ChaCha20-Poly1305
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&derived_key));
    let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
    let encrypted_payload = cipher.encrypt(&nonce, plaintext).expect("Encryption failed");
    
    EncryptedBundle {
        ephemeral_x25519: ephemeral_public.to_bytes(),
        pq_ciphertext: pq_ciphertext.as_bytes().to_vec(),
        nonce: nonce.into(),
        encrypted_payload,
    }
}
```

**Visual Representation**:
```
                     Sender Side
┌──────────────────────────────────────────────┐
│  Ephemeral X25519 Keypair                    │
│  ├─ Secret ────► ECDH ──┐                   │
│  └─ Public ──────────────┼─► HKDF-SHA256 ──┼─► ChaCha20
│                          │     (expand)     │    Poly1305
│  Recipient X25519 Pub ───┘         │        │    Encrypt
│                                    │        │
│  ML-KEM Encapsulate ──► PQ Secret ─┘        │
│  (with recipient ML-KEM Pub)               │
│    └─ Ciphertext                            │
└──────────────────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────┐
│  EncryptedBundle sent to recipient  │
│  - ephemeral_x25519 (32 bytes)      │
│  - pq_ciphertext (1088 bytes)       │
│  - nonce (12 bytes)                 │
│  - encrypted_payload (variable)     │
└─────────────────────────────────────┘
```

### Decryption Process (`open_payload`)

```rust
pub fn open_payload(
    bundle: &EncryptedBundle,
    my_identity: &HybridIdentity,
) -> Result<Vec<u8>, &'static str> {
    // Step 1: Reconstruct classical shared secret
    let sender_ephemeral = X25519PublicKey::from(bundle.ephemeral_x25519);
    let classical_shared_secret = my_identity.x25519_secret.diffie_hellman(&sender_ephemeral);
    
    // Step 2: Decapsulate PQ shared secret
    let pq_ciphertext = mlkem768::Ciphertext::from_bytes(&bundle.pq_ciphertext)
        .map_err(|_| "Invalid ML-KEM ciphertext format")?;
    let pq_shared_secret = mlkem768::decapsulate(&pq_ciphertext, &my_identity.mlkem_secret);
    
    // Step 3: Re-derive symmetric key
    let hkdf = Hkdf::<Sha256>::new(None, classical_shared_secret.as_bytes());
    let mut derived_key = [0u8; 32];
    hkdf.expand(pq_shared_secret.as_bytes(), &mut derived_key).expect("HKDF expansion failed");
    
    // Step 4: Decrypt
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&derived_key));
    let nonce = Nonce::from_slice(&bundle.nonce);
    cipher.decrypt(nonce, bundle.encrypted_payload.as_ref())
        .map_err(|_| "Decryption failed")
}
```

**Security Properties**:
- **Forward Secrecy**: Ephemeral X25519 key is used once and discarded
- **Post-Quantum Safe**: ML-KEM provides quantum resistance
- **Hybrid Security**: If one algorithm is broken, the other still provides protection
- **Authenticated Encryption**: ChaCha20-Poly1305 provides both confidentiality and integrity

## Storage Encryption

Messages stored in SQLite are encrypted at rest using ChaCha20-Poly1305 with the storage key.

### Encryption (`encrypt_for_storage`)

```rust
pub fn encrypt_for_storage(plaintext: &[u8], key: &[u8; 32]) -> StoredEncrypted {
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
    let ciphertext = cipher.encrypt(&nonce, plaintext).expect("storage encryption failed");
    
    StoredEncrypted {
        nonce: nonce.into(),
        ciphertext,
    }
}
```

### Decryption (`decrypt_for_storage`)

```rust
pub fn decrypt_for_storage(encrypted: &StoredEncrypted, key: &[u8; 32]) -> Result<Vec<u8>, &'static str> {
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    let nonce = Nonce::from_slice(&encrypted.nonce);
    cipher.decrypt(nonce, encrypted.ciphertext.as_ref())
        .map_err(|_| "storage decryption failed")
}
```

### Database Schema (Encrypted)

```sql
CREATE TABLE messages (
    id TEXT PRIMARY KEY,
    author TEXT NOT NULL,
    parents TEXT NOT NULL,
    content_nonce BLOB NOT NULL,    -- Used to decrypt content_ciphertext
    content_ciphertext BLOB NOT NULL -- Encrypted message content
);
```

**Note**: Only the message content is encrypted at rest. Metadata (id, author, parents) is stored in plaintext for DAG reconstruction.

## Signature Verification

Ed25519 signatures are used to verify the authenticity of:
1. Key exchange messages (KEX protocol)
2. Fat invite links

### KEX Signature Verification

**Purpose**: Ensure the peer actually owns the X25519 and ML-KEM keys they claim.

**Signing**:
```rust
// Sign the concatenation of X25519 || ML-KEM public keys
let mut payload_to_sign = my_x25519_pub.to_bytes().to_vec();
payload_to_sign.extend_from_slice(my_mlkem_pub.as_bytes());
let signature = my_ed25519_key.sign(&payload_to_sign);
```

**Verification**:
```rust
let mut verify_payload = received_x25519_pub.clone();
verify_payload.extend_from_slice(&received_mlkem_pub);

// Extract Ed25519 public key from PeerId
let pub_key = libp2p::identity::PublicKey::try_decode_protobuf(&peer_id_bytes)?;
pub_key.verify(&verify_payload, &received_signature)?;
```

### Invite Signature Verification

**Purpose**: Prevent tampering with invite links.

**Signing**:
```rust
let invite_data = FatInvite { topic, addrs, sender_pubkey, signature: Vec::new() };
let json = serde_json::to_string(&invite_data).unwrap();
let signature = sender_ed25519_key.sign(json.as_bytes());

let signed_invite = FatInvite { signature: signature.to_vec(), ..invite_data };
let b64_invite = base64::encode(serde_json::to_string(&signed_invite).unwrap());
```

**Verification**:
```rust
let mut invite_copy = received_invite.clone();
invite_copy.signature = Vec::new();
let payload = serde_json::to_string(&invite_copy).unwrap();

let pub_key = libp2p::identity::PublicKey::try_decode_protobuf(&invite_data.sender_pubkey)?;
pub_key.verify(payload.as_bytes(), &invite_data.signature)?;
```

## Threat Model

### Protected Against

| Threat | Protection Mechanism |
|--------|---------------------|
| Eavesdropping | End-to-end encryption (ChaCha20-Poly1305) |
| Man-in-the-Middle | ML-KEM + X25519 hybrid key exchange |
| Quantum Attacks | ML-KEM-768 (NIST post-quantum standard) |
| Harvest Now, Decrypt Later | Post-quantum cryptography |
| Message Tampering | AEAD encryption (integrity + authenticity) |
| Replay Attacks | Nonce-based encryption (unique per message) |
| Impersonation | Ed25519 signatures on key exchanges |
| Invite Tampering | Signed FatInvite structures |

### Not Protected Against

| Threat | Why | Potential Mitigation |
|--------|-----|---------------------|
| Endpoint Compromise | Malware on user device | OS-level security, user education |
| Traffic Analysis | Message timing/size visible | Padding, cover traffic (future) |
| Denial of Service | Network-level flooding | Rate limiting (future) |
| Metadata Leakage | DAG structure reveals communication patterns | Mix networks (future) |
| Bad Randomness | Weak RNG compromises all crypto | Use OsRng (system RNG) |

### Cryptographic Assumptions

1. **X25519** is secure (no practical attacks against Curve25519)
2. **ML-KEM-768** is secure (NIST standardized, no known quantum attacks)
3. **ChaCha20-Poly1305** is secure (no known practical attacks)
4. **SHA-256** is collision-resistant
5. **Ed25519** signatures are unforgeable
6. **System RNG** (`OsRng`) is properly seeded and unpredictable

## References

- **NIST FIPS 203**: ML-KEM (Module-Lattice-Based Key-Encapsulation Mechanism)
- **RFC 7748**: Elliptic Curves for Security (Curve25519)
- **RFC 8439**: ChaCha20 and Poly1305 for IETF Protocols
- **RFC 5869**: HMAC-based Extract-and-Expand Key Derivation Function (HKDF)
- **libp2p Noise**: https://github.com/libp2p/specs/tree/master/noise
- **MLS Architecture**: https://datatracker.ietf.org/doc/draft-ietf-mls-architecture/

## Implementation Notes

### Crate Versions Used
```toml
x25519-dalek = "2.0"
pqcrypto-mlkem = "0.1.1"
chacha20poly1305 = "0.10"
hkdf = "0.12"
sha2 = "0.10"
rand_core = "0.6"
```

### Security Auditing
- All cryptographic operations use well-audited crates
- No custom cryptographic implementations
- Random number generation uses `OsRng` (system entropy)
- Key zeroization is handled by the `Zeroize` trait implementations in the crates

### Future Improvements
- [ ] Implement MLS (Messaging Layer Security) for group encryption
- [ ] Add forward secrecy for long-term keys (ratcheting)
- [ ] Implement post-quantum signatures (e.g., SPHINCS+ or ML-DSA)
- [ ] Add optional padding to frustrate traffic analysis
- [ ] Consider hardware security module (HSM) support for key storage
