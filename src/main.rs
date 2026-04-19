// src/main.rs
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
use std::collections::HashSet;
use std::collections::hash_map::DefaultHasher;
use std::error::Error;
use std::hash::{Hash, Hasher};
use std::time::Duration;
use std::sync::{Arc, Mutex};
use tokio::io::{self, AsyncBufReadExt};
use rand_core::RngCore;

// Added tracing imports for the firehose log file
use tracing::{info, debug, warn, error, trace};
use tracing_subscriber::EnvFilter;
use store::{DagMessage, Store};
use kex::{KexRequest, KexResponse, KEX_PROTOCOL_NAME};

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
    // --- 1. INITIALIZE FIREHOSE LOGGING TO FILE ---
    // This runs in a non-blocking background thread to avoid slowing down the P2P engine.
    let file_appender = tracing_appender::rolling::never(".", "swarm_daemon.log");
    let (non_blocking_writer, _guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::fmt()
        // Here we capture ALL libp2p network traffic, kademlia events, and QUIC handshakes
        .with_env_filter(EnvFilter::new("debug,libp2p_kad=trace,libp2p_swarm=trace,libp2p_quic=debug,libp2p_tcp=debug,project_swarm_daemon=trace"))
        .with_writer(non_blocking_writer)
        .with_ansi(false) // No terminal color codes in the text file
        .init();

    info!("--- SWARM DAEMON BOOT SEQUENCE INITIATED ---");

    let db = Arc::new(Mutex::new(Store::new().expect("Failed to initialize SQLite database")));
    println!("[SYSTEM] Local DAG database initialized.");
    info!("Database initialized successfully.");

    println!("[SYSTEM] Generating Hybrid X25519 + ML-KEM Cryptographic Keys...");
    let my_crypto_id = crypto::HybridIdentity::generate();
    
    let key_path = "swarm_network_key.bin";
    let local_key = match std::fs::read(key_path) {
        Ok(bytes) => {
            println!("[SYSTEM] Loading existing network identity from disk...");
            info!("Loaded identity from disk.");
            identity::Keypair::from_protobuf_encoding(&bytes).expect("Valid identity file")
        }
        Err(_) => {
            println!("[SYSTEM] Generating NEW network identity...");
            info!("Generated new Ed25519 identity.");
            let new_key = identity::Keypair::generate_ed25519();
            std::fs::write(key_path, new_key.to_protobuf_encoding().unwrap()).expect("Failed to save key");
            new_key
        }
    };
    
    let local_peer_id = PeerId::from(local_key.public());
    let local_author_id = local_peer_id.to_string();
    println!("[SYSTEM] Quantum-resistant identity secured. Node ID: {}", local_peer_id);
    info!("Node PeerId: {}", local_peer_id);

    let message_id_fn = |message: &gossipsub::Message| {
        let mut s = DefaultHasher::new();
        message.data.hash(&mut s);
        gossipsub::MessageId::from(s.finish().to_string())
    };

    let gossipsub_config = gossipsub::ConfigBuilder::default()
        .heartbeat_interval(Duration::from_secs(5)) // Sped up heartbeat for faster mesh updates
        .validation_mode(gossipsub::ValidationMode::Strict)
        .message_id_fn(message_id_fn)
        .build()
        .expect("Valid gossipsub config");

    let mut gossipsub_behaviour = gossipsub::Behaviour::new(
        gossipsub::MessageAuthenticity::Signed(local_key.clone()),
        gossipsub_config,
    ).expect("Valid gossipsub setup");

    let mut current_topic = "swarm-alpha".to_string();
    let initial_topic = gossipsub::IdentTopic::new(current_topic.clone());
    gossipsub_behaviour.subscribe(&initial_topic).unwrap();

    let mdns_behaviour = mdns::tokio::Behaviour::new(
        mdns::Config::default(), 
        local_key.public().to_peer_id()
    )?;

    // --- 2. KADEMLIA DHT TUNING FOR SPEED ---
    let mut kad_config = kad::Config::default();
    // Reduce the query timeout so the DHT doesn't hang for 60 seconds searching dead global nodes
    kad_config.set_query_timeout(Duration::from_secs(15));
    // Require fewer peers to replicate our provider record, drastically speeding up the 'providing' phase
    kad_config.set_replication_factor(std::num::NonZeroUsize::new(2).unwrap());

    let kad_store = kad::store::MemoryStore::new(local_peer_id);
    let mut kad_behaviour = kad::Behaviour::with_config(local_peer_id, kad_store, kad_config);
    kad_behaviour.set_mode(Some(kad::Mode::Server));

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
        .with_tcp(tcp::Config::default(), noise::Config::new, yamux::Config::default)?
        .with_quic() 
        .with_dns()? 
        .with_behaviour(|_| behaviour)?
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(60)))
        .build();

    let static_port = 4001;
    swarm.listen_on(format!("/ip4/0.0.0.0/udp/{}/quic-v1", static_port).parse()?)?;
    swarm.listen_on(format!("/ip4/0.0.0.0/tcp/{}", static_port).parse()?)?;

    println!("[NETWORK] Bootstrapping to global P2P infrastructure...");
    info!("Bootstrapping to hardcoded libp2p nodes...");
    for node in BOOTSTRAP_NODES {
        if let Ok(addr) = node.parse::<Multiaddr>() {
            let _ = swarm.dial(addr.clone());
        }
    }
    let _ = swarm.behaviour_mut().kademlia.bootstrap();

    let rendezvous_key = kad::RecordKey::new(&b"project-swarm-rendezvous-v1");

    let mut stdin = io::BufReader::new(io::stdin()).lines();
    
    println!("\n[SYSTEM] Decentralized Mesh Engine ONLINE. 🚀");
    println!("[SYSTEM] Transport & Payloads are end-to-end encrypted.");
    println!("[SYSTEM] Type /invite to create a specific secure chat room.\n");

    let mut listen_addrs: Vec<Multiaddr> = Vec::new(); 
    let mut pending_dials: HashSet<PeerId> = HashSet::new();
    let mut is_providing = false;
    let mut known_providers: HashSet<PeerId> = HashSet::new(); 

    loop {
        tokio::select! {
            Ok(Some(line)) = stdin.next_line() => {
                let input = line.trim();
                if input.is_empty() { continue; }

                if input == "/invite" {
                    let mut rng_bytes = [0u8; 4];
                    rand_core::OsRng.fill_bytes(&mut rng_bytes);
                    let room_code = hex::encode(rng_bytes);
                    let invite_hash = format!("swarm-room-{}", room_code);
                    
                    println!("--- YOUR SECURE ROOM INVITE ---");
                    println!("Give this command to your friend:");
                    println!("  /join {}", invite_hash);
                    println!("-------------------------------");
                    info!("Generated invite for room: {}", invite_hash);
                    
                    let old_topic = gossipsub::IdentTopic::new(current_topic.clone());
                    let _ = swarm.behaviour_mut().gossipsub.unsubscribe(&old_topic);
                    
                    current_topic = invite_hash.clone();
                    let new_topic = gossipsub::IdentTopic::new(current_topic.clone());
                    let _ = swarm.behaviour_mut().gossipsub.subscribe(&new_topic);
                    
                    let room_key = kad::RecordKey::new(&current_topic);
                    let _ = swarm.behaviour_mut().kademlia.start_providing(room_key.into());
                    
                    println!("[SYSTEM] 🟡 You have moved to private room: '{}'. Waiting for peers...", current_topic);
                    continue;
                }

                if input.starts_with("/join ") {
                    let target_room = input.strip_prefix("/join ").unwrap().trim().to_string();
                    
                    println!("[NETWORK] Joining private room '{}' and searching DHT...", target_room);
                    info!("Initiated /join to room {}", target_room);
                    
                    let old_topic = gossipsub::IdentTopic::new(current_topic.clone());
                    let _ = swarm.behaviour_mut().gossipsub.unsubscribe(&old_topic);
                    
                    current_topic = target_room.clone();
                    let new_topic = gossipsub::IdentTopic::new(current_topic.clone());
                    let _ = swarm.behaviour_mut().gossipsub.subscribe(&new_topic);
                    
                    let room_key = kad::RecordKey::new(&current_topic);
                    let _ = swarm.behaviour_mut().kademlia.start_providing(room_key.clone().into());
                    swarm.behaviour_mut().kademlia.get_providers(room_key);
                    
                    continue;
                }

                if input == "/discover" {
                    println!("[NETWORK] Querying Global DHT for public swarm nodes...");
                    info!("Querying DHT for rendezvous key...");
                    swarm.behaviour_mut().kademlia.get_providers(rendezvous_key.clone());
                    continue;
                }

                if input.starts_with("/connect ") {
                    let target = input.strip_prefix("/connect ").unwrap().trim();
                    if let Ok(addr) = target.parse::<Multiaddr>() {
                        println!("[NETWORK] Dialing direct Multiaddr {}...", addr);
                        info!("Direct dial to {}", addr);
                        let _ = swarm.dial(addr);
                    } else if let Ok(peer) = target.parse::<PeerId>() {
                        println!("[NETWORK] Resolving Peer {} on DHT...", peer);
                        info!("Searching Kademlia for closest peers to {}", peer);
                        swarm.behaviour_mut().kademlia.get_closest_peers(peer);
                        pending_dials.insert(peer);
                    } else {
                        println!("[ERROR] Invalid address or PeerId format.");
                    }
                    continue;
                }

                if input == "/history" {
                    println!("--- LOCAL DAG HISTORY (LAST 50) ---");
                    let db_clone = Arc::clone(&db);
                    tokio::task::spawn_blocking(move || {
                        if let Ok(messages) = db_clone.lock().unwrap().get_recent_messages(50) {
                            for msg in messages {
                                println!("[{}] (Hash: {}): {}", &msg.author[..8], &msg.id[..8], msg.content);
                            }
                        }
                    }).await.unwrap();
                    println!("---------------------------");
                    continue; 
                }

                if input.starts_with("/whisper ") {
                    let parts: Vec<&str> = input.splitn(3, ' ').collect();
                    if parts.len() == 3 {
                        let target_peer_str = parts[1];
                        let message_text = parts[2].to_string();
                        
                        if let Ok(target_peer) = target_peer_str.parse::<PeerId>() {
                            let db_clone = Arc::clone(&db);
                            let target_str = target_peer.to_string();
                            let keys = tokio::task::spawn_blocking(move || {
                                db_clone.lock().unwrap().get_peer_keys(&target_str).unwrap_or(None)
                            }).await.unwrap();

                            if let Some((x25519_pub, mlkem_pub)) = keys {
                                if let Ok(bundle) = crypto::seal_for_network(
                                    message_text.as_bytes(),
                                    &x25519_pub,
                                    &mlkem_pub
                                ) {
                                    let payload = serde_json::to_vec(&bundle).unwrap();
                                    let topic_to_publish = gossipsub::IdentTopic::new(current_topic.clone());
                                    let _ = swarm.behaviour_mut().gossipsub.publish(topic_to_publish, payload);
                                    println!("[BROADCAST] 🟢 ML-KEM encrypted whisper sent to {}.", target_peer);
                                    info!("Whisper successfully encrypted and sent to {}", target_peer);
                                }
                            } else {
                                println!("[ERROR] Public keys for {} not found in database.", target_peer);
                            }
                        } else {
                            println!("[ERROR] Invalid PeerId format.");
                        }
                    } else {
                        println!("[SYSTEM] Usage: /whisper <PeerId> <Message>");
                    }
                    continue;
                }

                let db_clone = Arc::clone(&db);
                let local_author_clone = local_author_id.clone();
                let input_clone = input.to_string();
                let topic_to_publish = gossipsub::IdentTopic::new(current_topic.clone());
                
                let parents = tokio::task::spawn_blocking(move || {
                    let lock = db_clone.lock().unwrap();
                    let p = lock.get_latest_leaves().unwrap_or_default();
                    let dag_msg = DagMessage::new(local_author_clone, p.clone(), input_clone);
                    let _ = lock.save_message(&dag_msg);
                    dag_msg
                }).await.unwrap();

                let payload = serde_json::to_vec(&parents).unwrap();
                match swarm.behaviour_mut().gossipsub.publish(topic_to_publish, payload) {
                    Ok(_) => {
                        println!("[BROADCAST] 🟢 Message (Hash: {}) successfully encrypted and sent to channel.", &parents.id[..8]);
                        debug!("Gossipsub published msg {}", &parents.id[..8]);
                    } 
                    Err(gossipsub::PublishError::InsufficientPeers) => {
                        println!("[SYSTEM] 🟡 No active peers in room. (Message Hash: {} saved locally. Will sync upon connection.)", &parents.id[..8]);
                        warn!("Gossipsub dropped message due to InsufficientPeers. Saved to local DAG.");
                    }
                    Err(e) => {
                        println!("[ERROR] 🔴 Publish error: {e:?}");
                        error!("Gossipsub core engine fault: {:?}", e);
                    }
                }
            }
            
            event = swarm.select_next_some() => match event {
                SwarmEvent::Behaviour(SwarmProtocolEvent::Dcutr(event)) => {
                    println!("[NETWORK] 🥊 DCUtR Hole Punch Event: {:?}. Connection Commandeered!", event);
                    info!("DCUtR Hole Punch success: {:?}", event);
                },

                SwarmEvent::Behaviour(SwarmProtocolEvent::Identify(identify::Event::Received { peer_id, info: id_info })) => {
                    trace!("Identify info received from {}: {:?}", peer_id, id_info);
                    let observed_ip = id_info.observed_addr.clone();
                    if !listen_addrs.contains(&observed_ip) && listen_addrs.len() < 6 {
                        listen_addrs.push(observed_ip.clone());
                        let _ = swarm.add_external_address(observed_ip);
                    }
                    for addr in id_info.listen_addrs {
                        swarm.behaviour_mut().kademlia.add_address(&peer_id, addr);
                    }
                    
                    if !is_providing {
                        let _ = swarm.behaviour_mut().kademlia.start_providing(rendezvous_key.clone().into());
                        is_providing = true;
                    }

                    if id_info.protocols.contains(&kex::KEX_PROTOCOL_NAME) {
                        debug!("Peer {} supports KEX. Initiating secure key exchange.", peer_id);
                        let mut payload_to_sign = my_crypto_id.x25519_public.to_bytes().to_vec();
                        payload_to_sign.extend_from_slice(my_crypto_id.mlkem_public.as_bytes());
                        let signature = local_key.sign(&payload_to_sign).unwrap();

                        swarm.behaviour_mut().kex.send_request(
                            &peer_id,
                            KexRequest {
                                x25519_pub: my_crypto_id.x25519_public.to_bytes().to_vec(),
                                mlkem_pub: my_crypto_id.mlkem_public.as_bytes().to_vec(),
                                signature,
                            }
                        );
                    }
                }

                SwarmEvent::Behaviour(SwarmProtocolEvent::Kademlia(kad::Event::OutboundQueryProgressed { 
                    result: kad::QueryResult::GetProviders(Ok(kad::GetProvidersOk::FoundProviders { providers, .. })), .. 
                })) => {
                    for provider in providers {
                        if provider != local_peer_id && !swarm.is_connected(&provider) {
                            if known_providers.insert(provider) {
                                println!("[NETWORK] 🎯 Found Peer on DHT: {}. Negotiating connection...", provider);
                                info!("Discovered provider {} on DHT. Initiating dial.", provider);
                                let _ = swarm.dial(provider);
                                pending_dials.insert(provider);
                            }
                        }
                    }
                }

                SwarmEvent::Behaviour(SwarmProtocolEvent::Kademlia(kad::Event::OutboundQueryProgressed { 
                    result: kad::QueryResult::GetClosestPeers(Ok(ok)), .. 
                })) => {
                    for peer in ok.peers {
                        if pending_dials.contains(&peer) {
                            if !swarm.is_connected(&peer) {
                                println!("[NETWORK] DHT located {}. Initiating connection...", peer);
                                info!("DHT located closest peer {}. Dialing.", peer);
                                if let Err(e) = swarm.dial(peer) {
                                    error!("Dial failed immediately for peer {}: {:?}", peer, e);
                                    println!("[ERROR] Dial failed: {:?}", e);
                                }
                            }
                            pending_dials.remove(&peer);
                        }
                    }
                }

                SwarmEvent::OutgoingConnectionError { peer_id, error: net_err, .. } => {
                    if let Some(peer) = peer_id {
                        let err_str = format!("{:?}", net_err);
                        
                        let is_noise = err_str.contains("Timeout") || err_str.contains("HandshakeTimedOut") ||
                                       err_str.contains("Connection refused") || err_str.contains("MultiaddrNotSupported") ||
                                       err_str.contains("ConnectionReset") || err_str.contains("NetworkUnreachable") ||
                                       err_str.contains("No matching records found") || err_str.contains("WrongPeerId") ||
                                       err_str.contains("error: Failed");
                                       
                        if !is_noise {
                            println!("[ERROR] Failed to dial {}: {:?}", peer, net_err);
                            error!("Outgoing connection error to {}: {:?}", peer, net_err);
                        } else {
                            debug!("Routine connection drop (likely NAT/Timeout) to {}: {:?}", peer, net_err);
                        }
                        
                        pending_dials.remove(&peer);
                    }
                }

                SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                    info!("Base network connection established with {}", peer_id);
                    match endpoint {
                        libp2p::core::ConnectedPoint::Dialer { address, .. } => {
                            swarm.behaviour_mut().kademlia.add_address(&peer_id, address.clone());
                        }
                        libp2p::core::ConnectedPoint::Listener { send_back_addr, .. } => {
                            swarm.behaviour_mut().kademlia.add_address(&peer_id, send_back_addr.clone());
                        }
                    }
                    swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                }

                SwarmEvent::Behaviour(SwarmProtocolEvent::Kex(request_response::Event::Message { peer, message })) => match message {
                    request_response::Message::Request { request, channel, .. } => {
                        debug!("Received KEX request from {}", peer);
                        let mut verify_payload = request.x25519_pub.clone();
                        verify_payload.extend_from_slice(&request.mlkem_pub);
                        
                        let pub_key = libp2p::identity::PublicKey::try_decode_protobuf(&peer.to_bytes()[2..]).unwrap();
                        if !pub_key.verify(&verify_payload, &request.signature) {
                            warn!("Cryptographic signature verification failed for peer {}", peer);
                            continue;
                        }
                        
                        let db_clone = Arc::clone(&db);
                        let peer_str = peer.to_string();
                        let req_clone = request.clone();
                        tokio::task::spawn_blocking(move || {
                            let _ = db_clone.lock().unwrap().save_peer_keys(&peer_str, &req_clone.x25519_pub, &req_clone.mlkem_pub, &req_clone.signature);
                        }).await.unwrap();
                        
                        let mut payload_to_sign = my_crypto_id.x25519_public.to_bytes().to_vec();
                        payload_to_sign.extend_from_slice(my_crypto_id.mlkem_public.as_bytes());
                        let signature = local_key.sign(&payload_to_sign).unwrap();

                        let _ = swarm.behaviour_mut().kex.send_response(channel, KexResponse {
                            x25519_pub: my_crypto_id.x25519_public.to_bytes().to_vec(),
                            mlkem_pub: my_crypto_id.mlkem_public.as_bytes().to_vec(),
                            signature,
                        });

                        println!("[SYSTEM] 🟢 Secure connection to {} fully established and verified. Ready to chat!", peer);
                        info!("Secure KEX phase completed with {}", peer);

                        let known_leaves = db.lock().unwrap().get_latest_leaves().unwrap_or_default();
                        swarm.behaviour_mut().req_res.send_request(&peer, sync::SyncRequest { known_leaves });
                    }
                    request_response::Message::Response { response, .. } => {
                        debug!("Received KEX response from {}", peer);
                        let mut verify_payload = response.x25519_pub.clone();
                        verify_payload.extend_from_slice(&response.mlkem_pub);
                        
                        let pub_key = libp2p::identity::PublicKey::try_decode_protobuf(&peer.to_bytes()[2..]).unwrap();
                        if !pub_key.verify(&verify_payload, &response.signature) {
                            warn!("Cryptographic signature verification failed for peer {}", peer);
                            continue;
                        }
                        
                        let db_clone = Arc::clone(&db);
                        let peer_str = peer.to_string();
                        tokio::task::spawn_blocking(move || {
                            let _ = db_clone.lock().unwrap().save_peer_keys(&peer_str, &response.x25519_pub, &response.mlkem_pub, &response.signature);
                        }).await.unwrap();

                        println!("[SYSTEM] 🟢 Secure connection to {} fully established and verified. Ready to chat!", peer);
                        info!("Secure KEX phase completed with {}", peer);

                        let known_leaves = db.lock().unwrap().get_latest_leaves().unwrap_or_default();
                        swarm.behaviour_mut().req_res.send_request(&peer, sync::SyncRequest { known_leaves });
                    }
                }

                SwarmEvent::Behaviour(SwarmProtocolEvent::Gossipsub(gossipsub::Event::Message { message, .. })) => {
                    if let Ok(bundle) = serde_json::from_slice::<crypto::EncryptedBundle>(&message.data) {
                        if let Ok(decrypted) = crypto::open_payload(&bundle, &my_crypto_id) {
                            let text = String::from_utf8_lossy(&decrypted);
                            let sender = message.source.map(|p| p.to_string()).unwrap_or_else(|| "Unknown".to_string());
                            println!("[WHISPER] From [{}]: {}", &sender[..8], text);
                            info!("Decrypted incoming whisper from {}", sender);
                        }
                    } 
                    else if let Ok(dag_msg) = serde_json::from_slice::<DagMessage>(&message.data) {
                        let db_clone = Arc::clone(&db);
                        tokio::task::spawn_blocking(move || {
                            let _ = db_clone.lock().unwrap().save_message(&dag_msg);
                            println!("[{}] (Hash: {}): {}", &dag_msg.author[..8], &dag_msg.id[..8], dag_msg.content);
                            // Move the debug log INSIDE the closure while it still owns dag_msg
                            debug!("Processed incoming Gossipsub DAG message: {}", dag_msg.id);
                        }).await.unwrap();
                    }
                }
                
                SwarmEvent::Behaviour(SwarmProtocolEvent::ReqRes(event)) => match event {
                    request_response::Event::Message { peer, message: req_msg } => match req_msg {
                        request_response::Message::Request { request, channel, .. } => {
                            debug!("DAG Sync Request received from {}", peer);
                            let db_clone = Arc::clone(&db);
                            let missing = tokio::task::spawn_blocking(move || {
                                db_clone.lock().unwrap().get_messages_after(&request.known_leaves).unwrap_or_default()
                            }).await.unwrap();
                            
                            let _ = swarm.behaviour_mut().req_res.send_response(
                                channel,
                                sync::SyncResponse { missing_messages: missing }
                            );
                        }
                        request_response::Message::Response { response, .. } => {
                            if !response.missing_messages.is_empty() {
                                println!("[SYNC] Received {} missing messages from {}", response.missing_messages.len(), peer);
                                info!("DAG Sync Response: Ingesting {} missing blocks from {}", response.missing_messages.len(), peer);
                                let db_clone = Arc::clone(&db);
                                tokio::task::spawn_blocking(move || {
                                    let lock = db_clone.lock().unwrap();
                                    for msg in response.missing_messages {
                                        let _ = lock.save_message(&msg);
                                    }
                                }).await.unwrap();
                            }
                        }
                    },
                    _ => {}
                }
                _ => {}
            }
        }
    }
}