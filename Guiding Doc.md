## Master Technical Specification: Project Swarm

This document serves as the foundational blueprint for a peer-to-peer (P2P), masterless, locally hosted communication suite. It is designed to be highly resilient, aggressively secure, and entirely independent of cloud infrastructure.

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

The system bypasses standard DNS and static IP routing to ensure users can connect seamlessly without configuring routers or exposing their home networks.

* **Node Discovery:** Utilizes a Kademlia Distributed Hash Table (DHT). When an instance comes online, it announces its encrypted routing details to the DHT using its Public Key as the address.
* **NAT Traversal:** Implements STUN/TURN concepts natively via libp2p's AutoNAT and UDP Hole Punching. Routers are tricked into opening direct point-to-point tunnels without manual port forwarding.
* **Invite Links:** Formatted as `app://connect/[Target-Public-Key]#[Symmetric-Decryption-Key]`. The hash fragment ensures the decryption key never touches a DNS server or external network.
* **Real-Time Media (WebRTC):** Voice and video streams bypass the DHT and flow directly over the established UDP tunnels. For group calls exceeding 4-5 people, the swarm dynamically elects the node with the highest bandwidth to act as a temporary Selective Forwarding Unit (SFU) to distribute the streams efficiently.

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

* **Containerization:** The core Rust daemon is fully containerized. Deploying it via Docker makes it frictionless to run as an always-on node within a home lab environment, smoothly integrating with hypervisors running on Dell server architectures or management dashboards like CasaOS.
* **Headless Mode:** The daemon can run independently of the GUI. Users can leave a lightweight daemon running on a Raspberry Pi or a NAS to act as a permanent anchor and blind courier for their swarm, while interfacing with it via the GUI on their phone or main PC.
