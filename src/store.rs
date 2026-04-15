// src/store.rs
use rusqlite::{Connection, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

// This is the actual payload that will travel across the mesh
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct DagMessage {
    pub id: String,          // The SHA256 hash of this message
    pub author: String,      // The Public Key of the sender
    pub parents: Vec<String>,// The hashes of the messages that came before this one
    pub content: String,     // The actual text message (soon to be End-to-End Encrypted)
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

    // Hash the payload so it becomes an immutable block in the DAG
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
}

impl Store {
    pub fn new() -> Result<Self> {
        // Creates a local SQLite file in the directory you run the daemon from
        let conn = Connection::open("swarm_dag.db")?;
        
        // Initialize the table if it doesn't exist
        conn.execute(
            "CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                author TEXT NOT NULL,
                parents TEXT NOT NULL,
                content TEXT NOT NULL
            )",
            [],
        )?;
        Ok(Self { conn })
    }

    pub fn save_message(&self, msg: &DagMessage) -> Result<()> {
        let parents_json = serde_json::to_string(&msg.parents).unwrap_or_default();
        // INSERT OR IGNORE means if we receive a message we already have, we safely drop it
        self.conn.execute(
            "INSERT OR IGNORE INTO messages (id, author, parents, content) VALUES (?1, ?2, ?3, ?4)",
            (&msg.id, &msg.author, &parents_json, &msg.content),
        )?;
        Ok(())
    }

    pub fn get_all_messages(&self) -> Result<Vec<DagMessage>> {
        let mut stmt = self.conn.prepare("SELECT id, author, parents, content FROM messages ORDER BY rowid ASC")?;
        
        let msg_iter = stmt.query_map([], |row| {
            let parents_json: String = row.get(2)?;
            let parents: Vec<String> = serde_json::from_str(&parents_json).unwrap_or_default();
            
            Ok(DagMessage {
                id: row.get(0)?,
                author: row.get(1)?,
                parents,
                content: row.get(3)?,
            })
        })?;

        let mut messages = Vec::new();
        for msg in msg_iter {
            messages.push(msg?);
        }
        Ok(messages)
    }

    // Add this inside `impl Store` in src/store.rs
    pub fn get_messages_after(&self, known_leaves: &[String]) -> Result<Vec<DagMessage>> {
        let mut start_rowid: i64 = 0;
        
        // If they gave us leaves, find the row ID of the most recent one they know about
        if !known_leaves.is_empty() {
            let placeholders = known_leaves.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let query = format!("SELECT MAX(rowid) FROM messages WHERE id IN ({})", placeholders);
            let mut stmt = self.conn.prepare(&query)?;
            let params = rusqlite::params_from_iter(known_leaves);
            
            // If we don't find their hash (maybe they are on a different fork), default to 0 (sync everything)
            start_rowid = stmt.query_row(params, |row| row.get(0)).unwrap_or(0);
        }

        // Fetch everything after that row ID
        let mut stmt = self.conn.prepare("SELECT id, author, parents, content FROM messages WHERE rowid > ?1 ORDER BY rowid ASC")?;
        
        let msg_iter = stmt.query_map([start_rowid], |row| {
            let parents_json: String = row.get(2)?;
            let parents: Vec<String> = serde_json::from_str(&parents_json).unwrap_or_default();
            
            Ok(DagMessage {
                id: row.get(0)?,
                author: row.get(1)?,
                parents,
                content: row.get(3)?,
            })
        })?;

        let mut messages = Vec::new();
        for msg in msg_iter {
            messages.push(msg?);
        }
        Ok(messages)
    }

    // To add a new message, we need to know the ID of the most recent message(s) to link to.
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
