mod store;
mod sync;
mod crypto;
mod kex;

use futures::StreamExt;
use libp2p::{
    gossipsub, kad, mdns, request_response, identify, autonat, dcutr, relay, upnp,
    swarm::{NetworkBehaviour, SwarmEvent},
    identity, PeerId,
};
use libp2p::multiaddr::Multiaddr;
use std::collections::{HashMap, HashSet};
use std::collections::hash_map::DefaultHasher;
use std::error::Error;
use std::hash::{Hash, Hasher};
use std::time::Duration;
use tokio::io::{self, AsyncBufReadExt};

use store::{DagMessage, Store};
use kex::{KexRequest, KexResponse, KEX_PROTOCOL_NAME};

async fn try_aggressive_nat(ports: Vec<u16>) -> Option<u16> {
    println!("[NETWORK] Tier 2: Initiating Aggressive NAT-PMP sequence across {} ports...", ports.len());
    
    for port in ports {
        match crab_nat::PortMapping::new(
            std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED), 
            std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED), 
            crab_nat::InternetProtocol::Udp, 
            std::num::NonZeroU16::new(port).unwrap(),
            crab_nat::PortMappingOptions::default(),
        ).await {
            Ok(mapping) => {
                let ext_port = mapping.external_port().get();
                println!("[NETWORK] Tier 2 Success! NAT-PMP opened external port: {}", ext_port);
                return Some(ext_port);
            }
            Err(_) => {
                println!("[NETWORK] Tier 2: Port {} mapping rejected.", port);
            }
        }
    }
    
    println!("[NETWORK] Tier 2 Failed: Router refused NAT-PMP protocol on all ports.");
    None
}

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
    let db = Store::new().expect("Failed to initialize SQLite database");
    println!("[SYSTEM] Local DAG database initialized.");

    println!("[SYSTEM] Generating Hybrid X25519 + ML-KEM Cryptographic Keys...");
    let my_crypto_id = crypto::HybridIdentity::generate();
    println!("[SYSTEM] Quantum-resistant identity secured.");

    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());
    let local_author_id = local_peer_id.to_string();
    println!("[SYSTEM] Node ID: {}", local_peer_id);

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
    let kad_behaviour = kad::Behaviour::new(local_peer_id, kad_store);

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

    let autonat_behaviour = autonat::Behaviour::new(
        local_peer_id,
        autonat::Config::default(),
    );

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

    let mut swarm = libp2p::SwarmBuilder::with_existing_identity(local_key)
        .with_tokio()
        .with_quic()
        .with_dns()? 
        .with_behaviour(|_| behaviour)?
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(60)))
        .build();

    let static_port = 4001;
    swarm.listen_on(format!("/ip4/0.0.0.0/udp/{}/quic-v1", static_port).parse()?)?;

    let mut stdin = io::BufReader::new(io::stdin()).lines();
    println!("[SYSTEM] Network Engine Online. Type a message to broadcast.");

    let mut synced_peers = HashSet::new();
    let mut listen_addrs: Vec<Multiaddr> = Vec::new(); 
    let mut cascade_triggered = false;
    
    let mut key_ring: HashMap<PeerId, KexResponse> = HashMap::new();

    loop {
        tokio::select! {
            Ok(Some(line)) = stdin.next_line() => {
                let input = line.trim();
                if input.is_empty() { continue; }

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

                if input.starts_with("/connect ") {
                    let addr_str = input.strip_prefix("/connect ").unwrap().trim();
                    match addr_str.parse::<Multiaddr>() {
                        Ok(addr) => {
                            println!("[NETWORK] Dialing out to {}...", addr);
                            if let Err(e) = swarm.dial(addr) {
                                println!("[ERROR] Dial error: {:?}", e);
                            }
                        }
                        Err(e) => println!("[ERROR] Invalid address format: {}", e),
                    }
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

                let parents = db.get_latest_leaves().unwrap_or_default();
                let dag_msg = DagMessage::new(local_author_id.clone(), parents, input.to_string());
                
                if let Err(e) = db.save_message(&dag_msg) {
                    println!("[ERROR] Failed to save to local DAG: {}", e);
                }

                let payload = serde_json::to_vec(&dag_msg).unwrap();
                match swarm.behaviour_mut().gossipsub.publish(topic.clone(), payload) {
                    Ok(_) => {} 
                    Err(gossipsub::PublishError::InsufficientPeers) => {
                        println!("[SYSTEM] (Saved locally. Will sync when peers connect.)");
                    }
                    Err(e) => println!("[ERROR] Publish error: {e:?}"),
                }
            }
            
            event = swarm.select_next_some() => match event {
                SwarmEvent::NewListenAddr { address, .. } => {
                    if !listen_addrs.contains(&address) {
                        listen_addrs.push(address);
                    }
                }

                SwarmEvent::Behaviour(SwarmProtocolEvent::Upnp(upnp::Event::NewExternalAddr(addr))) => {
                    println!("[NETWORK] Tier 1 Success! UPnP mapped external port: {}", addr);
                    if !listen_addrs.contains(&addr) {
                        listen_addrs.push(addr);
                    }
                }
                
                SwarmEvent::Behaviour(SwarmProtocolEvent::Upnp(upnp::Event::GatewayNotFound | upnp::Event::NonRoutableGateway)) => {
                    if !cascade_triggered {
                        cascade_triggered = true;
                        println!("[NETWORK] Tier 1 Failed: Local router rejected UPnP.");
                        let ports_to_try = vec![4001, 50000, 50001, 51000, 60000];
                        tokio::spawn(async move {
                            if let Some(_ext_port) = try_aggressive_nat(ports_to_try).await {
                            } else {
                                println!("[NETWORK] Awaiting manual port forward or local peer connections.");
                            }
                        });
                    }
                }

                SwarmEvent::OutgoingConnectionError { error, .. } => {
                    println!("[ERROR] Outgoing connection failed: {:?}", error);
                }

                SwarmEvent::Behaviour(SwarmProtocolEvent::Mdns(mdns::Event::Discovered(list))) => {
                    for (peer_id, multiaddr) in list {
                        swarm.behaviour_mut().kademlia.add_address(&peer_id, multiaddr.clone());
                        swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                    }
                }

                SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                    println!("[NETWORK] Secure tunnel established with {}", peer_id);
                    
                    if endpoint.is_dialer() {
                        swarm.behaviour_mut().kademlia.add_address(&peer_id, endpoint.get_remote_address().clone());
                        swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                    }

                    println!("[SECURITY] Initiating Post-Quantum Key Exchange with {}...", peer_id);
                    swarm.behaviour_mut().kex.send_request(
                        &peer_id,
                        my_crypto_id.to_kex_request()
                    );
                }

                SwarmEvent::Behaviour(SwarmProtocolEvent::Kex(request_response::Event::Message { peer, message })) => match message {
                    request_response::Message::Request { request, channel, .. } => {
                        println!("[SECURITY] Received KEX Request from {}", peer);
                        key_ring.insert(peer, KexResponse {
                            x25519_pub: request.x25519_pub,
                            mlkem_pub: request.mlkem_pub,
                        });
                        let _ = swarm.behaviour_mut().kex.send_response(channel, my_crypto_id.to_kex_response());

                        if synced_peers.insert(peer) {
                            let known_leaves = db.get_latest_leaves().unwrap_or_default();
                            swarm.behaviour_mut().req_res.send_request(&peer, sync::SyncRequest { known_leaves });
                        }
                    }
                    request_response::Message::Response { response, .. } => {
                        println!("[SECURITY] KEX Handshake successful with {}", peer);
                        key_ring.insert(peer, response);

                        if synced_peers.insert(peer) {
                            let known_leaves = db.get_latest_leaves().unwrap_or_default();
                            swarm.behaviour_mut().req_res.send_request(&peer, sync::SyncRequest { known_leaves });
                        }
                    }
                }

                SwarmEvent::Behaviour(SwarmProtocolEvent::Identify(identify::Event::Received { info, .. })) => {
                    let observed_ip = info.observed_addr.clone();
                    if !listen_addrs.contains(&observed_ip) {
                        println!("[NETWORK] External IP verified via STUN: {}", observed_ip);
                        listen_addrs.push(observed_ip);
                    }
                }

                SwarmEvent::Behaviour(SwarmProtocolEvent::Gossipsub(gossipsub::Event::Message { message, .. })) => {
                    if let Ok(bundle) = serde_json::from_slice::<crypto::EncryptedBundle>(&message.data) {
                        if let Ok(decrypted) = crypto::open_payload(&bundle, &my_crypto_id) {
                            let text = String::from_utf8_lossy(&decrypted);
                            let sender = message.source.map(|p| p.to_string()).unwrap_or_else(|| "Unknown".to_string());
                            println!("[WHISPER] From [{}]: {}", &sender[..8], text);
                        }
                    } 
                    else if let Ok(dag_msg) = serde_json::from_slice::<DagMessage>(&message.data) {
                        if let Err(e) = db.save_message(&dag_msg) {
                            println!("[ERROR] Failed to write incoming message: {}", e);
                        }
                        println!("[{}] (Hash: {}): {}", &dag_msg.author[..8], &dag_msg.id[..8], dag_msg.content);
                    }
                }
                
                SwarmEvent::Behaviour(SwarmProtocolEvent::ReqRes(request_response::Event::Message { peer, message })) => match message {
                    request_response::Message::Request { request, channel, .. } => {
                        let missing = db.get_messages_after(&request.known_leaves).unwrap_or_default();
                        let _ = swarm.behaviour_mut().req_res.send_response(
                            channel,
                            sync::SyncResponse { missing_messages: missing }
                        );
                    }
                    request_response::Message::Response { response, .. } => {
                        if !response.missing_messages.is_empty() {
                            println!("[SYNC] Received {} missing messages from {}", response.missing_messages.len(), peer);
                            for msg in response.missing_messages {
                                let _ = db.save_message(&msg);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
}