mod store;
mod sync;
mod crypto;
mod kex;

use futures::StreamExt;
use libp2p::{
    gossipsub, kad, mdns, request_response, identify, autonat, dcutr, relay, upnp,
    swarm::{NetworkBehaviour, SwarmEvent},
    identity, PeerId, Multiaddr, tcp, noise, yamux
};
use pqcrypto_traits::kem::PublicKey;
use std::collections::{HashMap, HashSet};
use std::collections::hash_map::DefaultHasher;
use std::error::Error;
use std::hash::{Hash, Hasher};
use std::time::Duration;
use std::sync::{Arc, Mutex};
use tokio::io::{self, AsyncBufReadExt};

use store::{DagMessage, Store};
use kex::{KexRequest, KexResponse, KEX_PROTOCOL_NAME};

// Standard public IPFS/libp2p bootstrap nodes used for global DHT routing
const BOOTSTRAP_NODES: &[&str] = &[
    "/dnsaddr/bootstrap.libp2p.io/p2p/QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN",
    "/dnsaddr/bootstrap.libp2p.io/p2p/QmQCU2EcMqAqQPR2i9bChDtGNJchTbq5TbXBPxW8V92uMb",
    "/ip4/104.131.131.82/tcp/4001/p2p/QmaCpDMGvV2BGHeYERUEnRQAwe3N8SzbUtfsmvsqQLuvuJ",
];

#[derive(NetworkBehaviour)]
struct SwarmProtocol {
    gossipsub: gossipsub::Behaviour,
    mdns: mdns::tokio::Behaviour,
    kademlia: kad::Behaviour<kad::store::MemoryStore>,
    req_res: request_response::cbor::Behaviour<sync::SyncRequest, sync::SyncResponse>,
    kex: request_response::cbor::Behaviour<KexRequest, KexResponse>,
    identify: identify::Behaviour,
    autonat: autonat::Behaviour,
    dcutr: dcutr::Behaviour,
    relay_client: relay::client::Behaviour,
    upnp: upnp::tokio::Behaviour,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Thread-safe Store wrapped in Arc<Mutex> for non-blocking async access
    let db = Arc::new(Mutex::new(Store::new().expect("Failed to initialize SQLite database")));
    println!("[SYSTEM] Local DAG database initialized.");

    println!("[SYSTEM] Generating Hybrid X25519 + ML-KEM Cryptographic Keys...");
    let my_crypto_id = crypto::HybridIdentity::generate();
    
    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());
    let local_author_id = local_peer_id.to_string();
    println!("[SYSTEM] Quantum-resistant identity secured. Node ID: {}", local_peer_id);

    let message_id_fn = |message: &gossipsub::Message| {
        let mut s = DefaultHasher::new();
        message.data.hash(&mut s);
        gossipsub::MessageId::from(s.finish().to_string())
    };

    let gossipsub_config = gossipsub::ConfigBuilder::default()
        .heartbeat_interval(Duration::from_secs(10))
        .validation_mode(gossipsub::ValidationMode::Strict)
        .message_id_fn(message_id_fn)
        .build()
        .expect("Valid gossipsub config");

    let mut gossipsub_behaviour = gossipsub::Behaviour::new(
        gossipsub::MessageAuthenticity::Signed(local_key.clone()),
        gossipsub_config,
    ).expect("Valid gossipsub setup");

    let topic = gossipsub::IdentTopic::new("swarm-alpha");
    gossipsub_behaviour.subscribe(&topic).unwrap();

    let mdns_behaviour = mdns::tokio::Behaviour::new(
        mdns::Config::default(), 
        local_key.public().to_peer_id()
    )?;

    let kad_store = kad::store::MemoryStore::new(local_peer_id);
    let mut kad_behaviour = kad::Behaviour::new(local_peer_id, kad_store);
    kad_behaviour.set_mode(Some(kad::Mode::Server)); // Actively participate in routing

    let req_res_behaviour = request_response::cbor::Behaviour::new(
        [(sync::SYNC_PROTOCOL_NAME, request_response::ProtocolSupport::Full)],
        request_response::Config::default(),
    );

    let kex_behaviour = request_response::cbor::Behaviour::new(
        [(KEX_PROTOCOL_NAME, request_response::ProtocolSupport::Full)],
        request_response::Config::default(),
    );

    let identify_behaviour = identify::Behaviour::new(identify::Config::new(
        "/project-swarm/1.0.0".into(),
        local_key.public(),
    ));

    let autonat_behaviour = autonat::Behaviour::new(local_peer_id, autonat::Config::default());
    let (_relay_transport, relay_client_behaviour) = relay::client::new(local_peer_id);
    let dcutr_behaviour = dcutr::Behaviour::new(local_peer_id);
    let upnp_behaviour = upnp::tokio::Behaviour::default();

    let behaviour = SwarmProtocol {
        gossipsub: gossipsub_behaviour,
        mdns: mdns_behaviour,
        kademlia: kad_behaviour,
        req_res: req_res_behaviour,
        kex: kex_behaviour,
        identify: identify_behaviour,
        autonat: autonat_behaviour,
        dcutr: dcutr_behaviour,
        relay_client: relay_client_behaviour,
        upnp: upnp_behaviour,
    };

    let mut swarm = libp2p::SwarmBuilder::with_existing_identity(local_key.clone())
        .with_tokio()
        .with_tcp(tcp::Config::default(), noise::Config::new, yamux::Config::default)? // Fallback TCP transport
        .with_quic()
        .with_dns()? 
        .with_behaviour(|_| behaviour)?
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(60)))
        .build();

    // Listen on both QUIC (UDP) and TCP for maximum NAT penetration
    let static_port = 4001;
    swarm.listen_on(format!("/ip4/0.0.0.0/udp/{}/quic-v1", static_port).parse()?)?;
    swarm.listen_on(format!("/ip4/0.0.0.0/tcp/{}", static_port).parse()?)?;

    // Dial Bootstrap Relays
    for node in BOOTSTRAP_NODES {
        if let Ok(addr) = node.parse::<Multiaddr>() {
            println!("[NETWORK] Dialing public bootstrap relay: {}", addr);
            let _ = swarm.dial(addr.clone());
        }
    }
    let _ = swarm.behaviour_mut().kademlia.bootstrap();

    let mut stdin = io::BufReader::new(io::stdin()).lines();
    println!("[SYSTEM] Network Engine Online. Type a message to broadcast.");

    let mut synced_peers = HashSet::new();
    let mut listen_addrs: Vec<Multiaddr> = Vec::new(); 
    let mut key_ring: HashMap<PeerId, KexResponse> = HashMap::new();

    loop {
        tokio::select! {
            Ok(Some(line)) = stdin.next_line() => {
                let input = line.trim();
                if input.is_empty() { continue; }

                if input.starts_with("/connect ") {
                    let addr_str = input.strip_prefix("/connect ").unwrap().trim();
                    match addr_str.parse::<Multiaddr>() {
                        Ok(addr) => {
                            println!("[NETWORK] Dialing out to {}...", addr);
                            let _ = swarm.dial(addr);
                        }
                        Err(e) => println!("[ERROR] Invalid address format: {}", e),
                    }
                    continue;
                }

                if input == "/invite" {
                    println!("--- YOUR MAGIC INVITE LINKS ---");
                    for addr in &listen_addrs {
                        let addr_str = addr.to_string();
                        if addr_str.contains("/127.0.0.1/") || 
                           addr_str.contains("/169.254.") || 
                           addr_str.contains("/172.") || 
                           addr_str.contains("/10.") ||
                           (addr_str.contains("/192.168.") && !addr_str.contains("/192.168.1.")) {
                            continue;
                        }
                        println!("/connect {}/p2p/{}", addr, local_peer_id);
                    }
                    println!("------------------------------------");
                    continue; 
                }

                if input == "/history" {
                    println!("--- LOCAL DAG HISTORY ---");
                    match db.get_all_messages() {
                        Ok(messages) => {
                            for msg in messages {
                                println!("[{}] (Hash: {}): {}", &msg.author[..8], &msg.id[..8], msg.content);
                            }
                        }
                        Err(e) => println!("[ERROR] Failed to read history: {}", e),
                    }
                    println!("---------------------------");
                    continue; 
                }

                // WHISPER COMMAND
                if input.starts_with("/whisper ") {
                    let parts: Vec<&str> = input.splitn(3, ' ').collect();
                    if parts.len() == 3 {
                        let target_peer_str = parts[1];
                        let message_text = parts[2];
                        
                        if let Ok(target_peer) = target_peer_str.parse::<PeerId>() {
                            if let Some(target_keys) = key_ring.get(&target_peer) {
                                if let Ok(bundle) = crypto::seal_for_network(
                                    message_text.as_bytes(),
                                    &target_keys.x25519_pub,
                                    &target_keys.mlkem_pub
                                ) {
                                    let payload = serde_json::to_vec(&bundle).unwrap();
                                    let _ = swarm.behaviour_mut().gossipsub.publish(topic.clone(), payload);
                                    println!("[SYSTEM] ML-KEM encrypted whisper sent to {}.", target_peer);
                                }
                            } else {
                                println!("[ERROR] Public keys for {} not in KeyRing. Have they connected?", target_peer);
                            }
                        } else {
                            println!("[ERROR] Invalid PeerId format.");
                        }
                    } else {
                        println!("[SYSTEM] Usage: /whisper <PeerId> <Message>");
                    }
                    continue;
                }

                // ... keep the /history and /invite and /whisper logic identical ...

                let db_clone = Arc::clone(&db);
                let local_author_clone = local_author_id.clone();
                let input_clone = input.to_string();
                let topic_clone = topic.clone();
                
                // ASYNC FIX: Spawn blocking task for SQLite operations
                let parents = tokio::task::spawn_blocking(move || {
                    let lock = db_clone.lock().unwrap();
                    let p = lock.get_latest_leaves().unwrap_or_default();
                    let dag_msg = DagMessage::new(local_author_clone, p.clone(), input_clone);
                    let _ = lock.save_message(&dag_msg);
                    dag_msg
                }).await.unwrap();

                let payload = serde_json::to_vec(&parents).unwrap();
                match swarm.behaviour_mut().gossipsub.publish(topic_clone, payload) {
                    Ok(_) => {} 
                    Err(gossipsub::PublishError::InsufficientPeers) => println!("[SYSTEM] (Saved locally. Will sync.)"),
                    Err(e) => println!("[ERROR] Publish error: {e:?}"),
                }
            }
            
            event = swarm.select_next_some() => match event {
                SwarmEvent::NewListenAddr { address, .. } => {
                    if !listen_addrs.contains(&address) {
                        listen_addrs.push(address);
                    }
                }
                
                SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                    println!("[NETWORK] Secure tunnel established with {}", peer_id);
                    if endpoint.is_dialer() {
                        swarm.behaviour_mut().kademlia.add_address(&peer_id, endpoint.get_remote_address().clone());
                        swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                    }

                    // Authenticate the Key Exchange
                    let mut payload_to_sign = my_crypto_id.x25519_public.to_bytes().to_vec();
                    payload_to_sign.extend_from_slice(my_crypto_id.mlkem_public.as_bytes());
                    let signature = local_key.sign(&payload_to_sign).unwrap();

                    println!("[SECURITY] Initiating Authenticated PQ Key Exchange with {}...", peer_id);
                    swarm.behaviour_mut().kex.send_request(
                        &peer_id,
                        KexRequest {
                            x25519_pub: my_crypto_id.x25519_public.to_bytes().to_vec(),
                            mlkem_pub: my_crypto_id.mlkem_public.as_bytes().to_vec(),
                            signature,
                        }
                    );
                }

                SwarmEvent::Behaviour(SwarmProtocolEvent::Kex(request_response::Event::Message { peer, message })) => match message {
                    request_response::Message::Request { request, channel, .. } => {
                        let mut verify_payload = request.x25519_pub.clone();
                        verify_payload.extend_from_slice(&request.mlkem_pub);
                        
                        // Extract Peer's Ed25519 public key and verify signature
                        let pub_key = libp2p::identity::PublicKey::try_decode_protobuf(&peer.to_bytes()[2..]).unwrap();
                        if !pub_key.verify(&verify_payload, &request.signature) {
                            println!("[SECURITY] Critical: KEX Signature verification failed for {}!", peer);
                            continue;
                        }

                        println!("[SECURITY] Authenticated KEX Request from {}", peer);
                        key_ring.insert(peer, KexResponse {
                            x25519_pub: request.x25519_pub,
                            mlkem_pub: request.mlkem_pub,
                            signature: request.signature,
                        });
                        
                        let mut payload_to_sign = my_crypto_id.x25519_public.to_bytes().to_vec();
                        payload_to_sign.extend_from_slice(my_crypto_id.mlkem_public.as_bytes());
                        let signature = local_key.sign(&payload_to_sign).unwrap();

                        let _ = swarm.behaviour_mut().kex.send_response(channel, KexResponse {
                            x25519_pub: my_crypto_id.x25519_public.to_bytes().to_vec(),
                            mlkem_pub: my_crypto_id.mlkem_public.as_bytes().to_vec(),
                            signature,
                        });
                    }
                    request_response::Message::Response { response, .. } => {
                        let mut verify_payload = response.x25519_pub.clone();
                        verify_payload.extend_from_slice(&response.mlkem_pub);
                        
                        let pub_key = libp2p::identity::PublicKey::try_decode_protobuf(&peer.to_bytes()[2..]).unwrap();
                        if !pub_key.verify(&verify_payload, &response.signature) {
                            println!("[SECURITY] Critical: KEX Signature verification failed for {}!", peer);
                            continue;
                        }

                        println!("[SECURITY] KEX Handshake successful with {}", peer);
                        key_ring.insert(peer, response);
                    }
                }
                
                // Add your other standard event matchers (Gossipsub, req_res sync) here wrapped in spawn_blocking where writing to db
                _ => {}
            }
        }
    }
}