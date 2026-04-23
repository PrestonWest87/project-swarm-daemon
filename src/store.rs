// src/store.rs
use rusqlite::{Connection, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use crate::crypto::{StoredEncrypted, decrypt_for_storage, encrypt_for_storage};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct DagMessage {
    pub id: String,          
    pub author: String,      
    pub parents: Vec<String>,
    pub content: String,     
}

impl DagMessage {
    pub fn new(author: String, parents: Vec<String>, content: String) -> Self {
        let mut msg = DagMessage {
            id: String::new(),
            author,
            parents,
            content,
        };
        msg.id = msg.calculate_hash();
        msg
    }

    pub fn calculate_hash(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(&self.author);
        for p in &self.parents {
            hasher.update(p);
        }
        hasher.update(&self.content);
        hex::encode(hasher.finalize())
    }
}

pub struct Store {
    conn: Connection,
    storage_key: [u8; 32],
}

impl Store {
    pub fn new(storage_key: [u8; 32]) -> Result<Self> {
        let conn = Connection::open("swarm_dag.db")?;
        
        conn.execute(
            "CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                author TEXT NOT NULL,
                parents TEXT NOT NULL,
                content_nonce BLOB NOT NULL,
                content_ciphertext BLOB NOT NULL
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS peers (
                peer_id TEXT PRIMARY KEY,
                x25519_pub BLOB NOT NULL,
                mlkem_pub BLOB NOT NULL,
                signature BLOB NOT NULL
            )",
            [],
        )?;

        Ok(Self { conn, storage_key })
    }

    pub fn save_message(&self, msg: &DagMessage) -> Result<()> {
        let encrypted = encrypt_for_storage(msg.content.as_bytes(), &self.storage_key);
        let parents_json = serde_json::to_string(&msg.parents).unwrap_or_default();
        self.conn.execute(
            "INSERT OR IGNORE INTO messages (id, author, parents, content_nonce, content_ciphertext) VALUES (?1, ?2, ?3, ?4, ?5)",
            (&msg.id, &msg.author, &parents_json, &encrypted.nonce, &encrypted.ciphertext),
        )?;
        Ok(())
    }

    pub fn save_peer_keys(&self, peer_id: &str, x25519_pub: &[u8], mlkem_pub: &[u8], signature: &[u8]) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO peers (peer_id, x25519_pub, mlkem_pub, signature) VALUES (?1, ?2, ?3, ?4)",
            (peer_id, x25519_pub, mlkem_pub, signature),
        )?;
        Ok(())
    }

    pub fn get_peer_keys(&self, peer_id: &str) -> Result<Option<(Vec<u8>, Vec<u8>)>> {
        let mut stmt = self.conn.prepare("SELECT x25519_pub, mlkem_pub FROM peers WHERE peer_id = ?1")?;
        let mut rows = stmt.query([peer_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some((row.get(0)?, row.get(1)?)))
        } else {
            Ok(None)
        }
    }

    pub fn get_recent_messages(&self, limit: u32) -> Result<Vec<DagMessage>> {
        let mut stmt = self.conn.prepare("SELECT id, author, parents, content_nonce, content_ciphertext FROM messages ORDER BY rowid DESC LIMIT ?1")?;
        let msg_iter = stmt.query_map([limit], |row| {
            let parents_json: String = row.get(2)?;
            let parents: Vec<String> = serde_json::from_str(&parents_json).unwrap_or_default();
            let nonce: Vec<u8> = row.get(3)?;
            let ciphertext: Vec<u8> = row.get(4)?;
            let encrypted = StoredEncrypted {
                nonce: nonce.try_into().unwrap_or([0u8; 12]),
                ciphertext,
            };
            let decrypted = decrypt_for_storage(&encrypted, &self.storage_key).unwrap_or_default();
            let content = String::from_utf8(decrypted).unwrap_or_default();
            Ok(DagMessage {
                id: row.get(0)?,
                author: row.get(1)?,
                parents,
                content,
            })
        })?;

        let mut messages = Vec::new();
        for msg in msg_iter {
            messages.push(msg?);
        }
        messages.reverse();
        Ok(messages)
    }

pub fn get_messages_after(&self, known_leaves: &[String]) -> Result<Vec<DagMessage>> {
        let mut start_rowid: i64 = 0;
        
        if !known_leaves.is_empty() {
            let placeholders = known_leaves.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let query = format!("SELECT MAX(rowid) FROM messages WHERE id IN ({})", placeholders);
            let mut stmt = self.conn.prepare(&query)?;
            let params = rusqlite::params_from_iter(known_leaves);
            
            start_rowid = match stmt.query_row(params, |row| row.get::<_, i64>(0)) {
                Ok(id) => id,
                Err(_) => return Ok(vec![]), 
            };
        }

        let mut stmt = self.conn.prepare("SELECT id, author, parents, content_nonce, content_ciphertext FROM messages WHERE rowid > ?1 ORDER BY rowid ASC")?;
        
        let msg_iter = stmt.query_map([start_rowid], |row| {
            let parents_json: String = row.get(2)?;
            let parents: Vec<String> = serde_json::from_str(&parents_json).unwrap_or_default();
            let nonce: Vec<u8> = row.get(3)?;
            let ciphertext: Vec<u8> = row.get(4)?;
            let encrypted = StoredEncrypted {
                nonce: nonce.try_into().unwrap_or([0u8; 12]),
                ciphertext,
            };
            let decrypted = decrypt_for_storage(&encrypted, &self.storage_key).unwrap_or_default();
            let content = String::from_utf8(decrypted).unwrap_or_default();
            
            Ok(DagMessage {
                id: row.get(0)?,
                author: row.get(1)?,
                parents,
                content,
            })
        })?;

        let mut messages = Vec::new();
        for msg in msg_iter {
            messages.push(msg?);
        }
        Ok(messages)
    }

    pub fn get_latest_leaves(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT id FROM messages ORDER BY rowid DESC LIMIT 1")?;
        let mut rows = stmt.query([])?;
        let mut leaves = Vec::new();
        
        if let Some(row) = rows.next()? {
            leaves.push(row.get(0)?);
        }
        Ok(leaves)
    }
}