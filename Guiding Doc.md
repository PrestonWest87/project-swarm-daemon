## Master Technical Specification: Project Swarm

This document serves as the foundational blueprint for a peer-to-peer (P2P), masterless, locally hosted communication suite. It is designed to be highly resilient, aggressively secure, and entirely independent of central cloud infrastructure.

---

### 1. Core Architecture & Administrative Authority

The network operates as a decentralized mesh of sovereign nodes (clients). There are no central servers; the "server" is a shared state maintained by all connected peers.

* **The Genesis Node:** The user who creates the instance generates a unique Root Cryptographic Keypair. This user is the Genesis Node.
* **Administrative Actions:** Commands such as kicking a user, deleting messages, or changing channel permissions are broadcast to the swarm as cryptographic payloads signed by the Root Private Key.
* **Swarm Consensus:** Every node in the swarm verifies the signature against the Genesis Public Key. If valid, the nodes independently execute the command (e.g., dropping the kicked user's connection and ignoring their future packets).
* **Delegation:** The Genesis Node can sign tokens granting temporary or permanent administrative rights to other public keys in the swarm.

---

### 2. The Technology Stack

To achieve bare-metal efficiency and massive scalability without memory bloat, the stack strictly avoids heavy garbage-collected frameworks in the core routing logic.

| Component | Technology | Rationale |
| :--- | :--- | :--- |
| **Core Daemon / Backend** | Rust | Memory safety, fearless concurrency, and minimal footprint. Handles P2P routing, cryptography, and state management. |
| **Networking Stack** | libp2p (Rust implementation) | Industry standard for decentralized network routing, DHT management, and NAT traversal. |
| **Desktop GUI** | Tauri + React/Svelte | Native OS webviews driven by a Rust backend. Uses fractions of the RAM required by Electron. |
| **CLI / TUI Version** | Ratatui (Rust) | Fully featured, keyboard-driven terminal interface running directly on the core daemon API. |
| **Local Database** | SQLite | Fast, file-based relational database for storing the local DAG, message history, and settings. |

---

### 3. Cryptography & Post-Quantum Security

Given the experimental nature of hybrid post-quantum protocols, the architecture is designed with rigorous penetration testing and continuous security auditing in mind from day one. All communications are End-to-End Encrypted (E2EE).

* **Hybrid Key Exchange:** Combines classical elliptic-curve cryptography (X25519) with NIST-approved post-quantum algorithms (ML-KEM/Kyber). This protects against "harvest now, decrypt later" quantum attacks while maintaining a classical fallback.
* **Group Encryption (MLS):** Utilizes the Messaging Layer Security (MLS) protocol. Instead of encrypting a message 50 times for 50 users, MLS uses a highly efficient cryptographic tree structure (Ratchet Tree) to secure large group chats with minimal computational overhead.
* **Forward Secrecy:** Session keys rotate automatically with every message sent. A compromised key in the future cannot decrypt past traffic.
* **Cryptographic Identity:** Users do not have usernames at the protocol level. A user's true identity is their Public Key. Display names are simply local aliases tied to that key.

---

### 4. Networking & Swarm Protocols

The system relies on an emergent, opportunistic mesh architecture. It bypasses standard DNS routing and central rendezvous servers, allowing the network to heal and scale autonomously.

* **Global DHT as a Phonebook:** The network utilizes the public IPFS Kademlia DHT strictly for address resolution. Nodes query public bootstrap nodes (like `bootstrap.libp2p.io`) only to ask for provider records mapped to rendezvous hashes. Once an IP is retrieved, the public node connection is dropped, and the swarm nodes dial each other directly.
* **Emergent Relays & AutoNAT:** The network automatically provisions its own infrastructure. Upon boot, nodes use AutoNAT to test their network environments. If a node determines it is publicly reachable (via open ports or UPnP), it silently promotes itself to a Circuit Relay Server, advertising its bandwidth to the DHT to route traffic for peers trapped behind strict NATs or carrier-grade firewalls.
* **Hole Punching (DCUtR):** When two NAT-trapped peers communicate through an emergent relay, they execute Direct Connection Upgrade through Relay (DCUtR) to punch a direct UDP tunnel, dropping the relay entirely once established.
* **Fat Invites (Out-of-Band Bootstrapping):** To form instant micro-meshes without relying on the global DHT, invite links are generated as base64-encoded strings. These "fat" invites contain the user's exact `Multiaddr` (IP, port, and PeerID), allowing instant point-to-point dialing the moment a peer pastes the code.

---

### 5. Data Structures & State Management

Because there is no central database, the state of the chat must be mathematically provable and self-healing.

* **The Message DAG:** Messages are not ordered by timestamps. They are structured as a Directed Acyclic Graph (DAG). Every new message contains the cryptographic hash of the previous message state.
* **Conflict Resolution:** If two users send a message simultaneously, the DAG forks momentarily. The protocol uses Conflict-Free Replicated Data Types (CRDTs) to deterministically merge the timelines for all users without data loss.
* **Blind Store-and-Forward:** When a user is offline, online peers act as blind couriers. Encrypted message payloads bound for the offline user are held in a temporary local cache. Upon reconnection, peers automatically push these locked payloads to the user, who decrypts and slots them into their local DAG.
* **Media Pointers:** Large files are not forcibly synced. The sender uploads the file to an integrated, encrypted IPFS-style local node. The chat receives a lightweight hash pointer. Peers only fetch the heavy data if the user actively clicks "download."

---

### 6. Deployment & Infrastructure

The application runs purely client-side, but the underlying daemon is designed to run anywhere, from a primary desktop to headless server environments.

* **Containerization:** The core Rust daemon is fully containerized. Deploying it via Docker makes it frictionless to run as an always-on node within a homelab environment. Because of the AutoNAT emergent relay feature, dropping this container on a machine with open ports automatically strengthens the global mesh.
* **Headless Mode:** The daemon can run independently of the GUI. Users can leave a lightweight daemon running on dedicated hardware to act as a permanent anchor and blind courier for their swarm, while interfacing with it via the GUI on their phone or main workstation.
