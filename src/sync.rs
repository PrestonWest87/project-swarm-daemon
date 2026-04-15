// src/sync.rs
use libp2p::StreamProtocol;
use serde::{Deserialize, Serialize};
use crate::store::DagMessage;

// The official name of our custom protocol on the wire
pub const SYNC_PROTOCOL_NAME: StreamProtocol = StreamProtocol::new("/project-swarm/sync/1.0.0");

// The Question
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncRequest {
    pub known_leaves: Vec<String>, // The hashes of the most recent messages we have
}

// The Answer
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncResponse {
    pub missing_messages: Vec<DagMessage>, // The blocks the peer needs
}
