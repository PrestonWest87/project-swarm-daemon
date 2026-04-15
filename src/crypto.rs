use serde::{Deserialize, Serialize};
use pqcrypto_mlkem::mlkem768;
use pqcrypto_traits::kem::{Ciphertext, SharedSecret, PublicKey as PqPublicKey};
use x25519_dalek::{EphemeralSecret, PublicKey as X25519PublicKey, StaticSecret};
use chacha20poly1305::{aead::{Aead, AeadCore, KeyInit}, ChaCha20Poly1305, Key, Nonce};
use hkdf::Hkdf;
use sha2::Sha256;
use rand_core::OsRng as RandOsRng;
use crate::kex::{KexRequest, KexResponse};

pub struct HybridIdentity {
    pub x25519_secret: StaticSecret,
    pub x25519_public: X25519PublicKey,
    pub mlkem_secret: mlkem768::SecretKey,
    pub mlkem_public: mlkem768::PublicKey,
}

impl HybridIdentity {
    pub fn generate() -> Self {
        let x25519_secret = StaticSecret::random_from_rng(RandOsRng);
        let x25519_public = X25519PublicKey::from(&x25519_secret);
        let (mlkem_public, mlkem_secret) = mlkem768::keypair();

        Self {
            x25519_secret,
            x25519_public,
            mlkem_secret,
            mlkem_public,
        }
    }

    pub fn to_kex_request(&self) -> KexRequest {
        KexRequest {
            x25519_pub: self.x25519_public.to_bytes().to_vec(),
            mlkem_pub: self.mlkem_public.as_bytes().to_vec(),
        }
    }

    pub fn to_kex_response(&self) -> KexResponse {
        KexResponse {
            x25519_pub: self.x25519_public.to_bytes().to_vec(),
            mlkem_pub: self.mlkem_public.as_bytes().to_vec(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EncryptedBundle {
    pub ephemeral_x25519: [u8; 32],
    pub pq_ciphertext: Vec<u8>,
    pub nonce: [u8; 12],
    pub encrypted_payload: Vec<u8>,
}

pub fn seal_payload(
    plaintext: &[u8],
    recipient_x25519_pub: &X25519PublicKey,
    recipient_mlkem_pub: &mlkem768::PublicKey,
) -> EncryptedBundle {
    let ephemeral_secret = EphemeralSecret::random_from_rng(RandOsRng);
    let ephemeral_public = X25519PublicKey::from(&ephemeral_secret);
    let classical_shared_secret = ephemeral_secret.diffie_hellman(recipient_x25519_pub);

    let (pq_shared_secret, pq_ciphertext) = mlkem768::encapsulate(recipient_mlkem_pub);

    let hkdf = Hkdf::<Sha256>::new(None, classical_shared_secret.as_bytes());
    let mut derived_key = [0u8; 32];
    hkdf.expand(pq_shared_secret.as_bytes(), &mut derived_key).expect("HKDF expansion failed");

    let cipher = ChaCha20Poly1305::new(Key::from_slice(&derived_key));
    let nonce = ChaCha20Poly1305::generate_nonce(&mut RandOsRng);
    let encrypted_payload = cipher.encrypt(&nonce, plaintext).expect("Encryption failed");

    EncryptedBundle {
        ephemeral_x25519: ephemeral_public.to_bytes(),
        pq_ciphertext: pq_ciphertext.as_bytes().to_vec(),
        nonce: nonce.into(),
        encrypted_payload,
    }
}

pub fn open_payload(
    bundle: &EncryptedBundle,
    my_identity: &HybridIdentity,
) -> Result<Vec<u8>, &'static str> {
    let sender_ephemeral = X25519PublicKey::from(bundle.ephemeral_x25519);
    let classical_shared_secret = my_identity.x25519_secret.diffie_hellman(&sender_ephemeral);

    let pq_ciphertext = mlkem768::Ciphertext::from_bytes(&bundle.pq_ciphertext)
        .map_err(|_| "Invalid ML-KEM ciphertext format")?;
    let pq_shared_secret = mlkem768::decapsulate(&pq_ciphertext, &my_identity.mlkem_secret);

    let hkdf = Hkdf::<Sha256>::new(None, classical_shared_secret.as_bytes());
    let mut derived_key = [0u8; 32];
    hkdf.expand(pq_shared_secret.as_bytes(), &mut derived_key).expect("HKDF expansion failed");

    let cipher = ChaCha20Poly1305::new(Key::from_slice(&derived_key));
    let nonce = Nonce::from_slice(&bundle.nonce);
    
    cipher.decrypt(nonce, bundle.encrypted_payload.as_ref())
        .map_err(|_| "Decryption failed. Invalid key, corrupted payload, or tampered data.")
}

pub fn seal_for_network(
    plaintext: &[u8],
    recipient_x25519_bytes: &[u8],
    recipient_mlkem_bytes: &[u8],
) -> Result<EncryptedBundle, &'static str> {
    let x_bytes: [u8; 32] = recipient_x25519_bytes.try_into().map_err(|_| "Invalid X25519 key length")?;
    let x_pub = X25519PublicKey::from(x_bytes);
    let pq_pub = pqcrypto_traits::kem::PublicKey::from_bytes(recipient_mlkem_bytes).map_err(|_| "Invalid ML-KEM key")?;
    
    Ok(seal_payload(plaintext, &x_pub, &pq_pub))
}