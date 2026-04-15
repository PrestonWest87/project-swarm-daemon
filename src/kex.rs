use libp2p::StreamProtocol;
use serde::{Deserialize, Serialize};

pub const KEX_PROTOCOL_NAME: StreamProtocol = StreamProtocol::new("/project-swarm/kex/1.0.0");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KexRequest {
    pub x25519_pub: Vec<u8>,
    pub mlkem_pub: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KexResponse {
    pub x25519_pub: Vec<u8>,
    pub mlkem_pub: Vec<u8>,
}