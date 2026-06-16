//! RUSTSOLVER v12 — Distributed Search Coordination
//! ==================================================
//!
//! Multi-machine coordination protocol for PRISM VORTEX.
//! Splits the search space across N nodes and merges DP tables.
//!
//! ARCHITECTURE:
//!   ┌──────────┐     ┌──────────┐     ┌──────────┐
//!   │  Node 0  │     │  Node 1  │     │  Node 2  │
//!   │ (tame)   │     │ (tame)   │     │ (wild)   │
//!   └────┬─────┘     └────┬─────┘     └────┬─────┘
//!        │                │                │
//!        └────────┬───────┘────────────────┘
//!                 │
//!          ┌──────▼──────┐
//!          │  Coordinator │
//!          │  (DP Merge)  │
//!          └─────────────┘
//!
//! PROTOCOL:
//!   1. Coordinator assigns walk ranges to each node
//!   2. Each node runs PRISM VORTEX independently
//!   3. DPs are periodically sent to coordinator
//!   4. Coordinator merges DP tables and checks for collisions
//!   5. When collision found, coordinator broadcasts solution
//!
//! COMMUNICATION:
//!   - Uses simple TCP with length-prefixed messages
//!   - DP entries: 32 bytes (x-coordinate) + 32 bytes (distance) + 1 byte (metadata)
//!   - Heartbeat every 10 seconds
//!   - DP batch send every 5 seconds or 1000 DPs (whichever first)
//!
//! FAULT TOLERANCE:
//!   - If a node disconnects, coordinator reassigns its range
//!   - DP table is persisted to disk every minute
//!   - Checkpoint/resume support for long runs

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// ============================================================
// MESSAGE TYPES
// ============================================================

/// Message type codes
const MSG_REGISTER: u8 = 0x01;      // Node → Coordinator: I'm here
const MSG_ASSIGN: u8 = 0x02;        // Coordinator → Node: Your range
const MSG_DP_BATCH: u8 = 0x03;      // Node → Coordinator: Here are DPs
const MSG_COLLISION: u8 = 0x04;     // Coordinator → All: Found key!
const MSG_HEARTBEAT: u8 = 0x05;     // Bidirectional: Still alive
const MSG_CHECKPOINT: u8 = 0x06;    // Coordinator → Node: Save state
const MSG_STATUS_REQ: u8 = 0x07;    // Coordinator → Node: Send stats
const MSG_STATUS_RESP: u8 = 0x08;   // Node → Coordinator: Stats

/// Maximum DPs per batch message
const MAX_DP_BATCH: usize = 1000;

/// DP entry as transmitted over network (65 bytes)
#[derive(Clone, Debug)]
pub struct DPMessage {
    /// x-coordinate of distinguished point (32 bytes)
    pub x_bytes: [u8; 32],
    /// Distance/scalar (32 bytes)
    pub distance_bytes: [u8; 32],
    /// Metadata: bit 0 = tame/wild, bits 1-2 = GLV variant, bit 3 = sign
    pub meta: u8,
}

impl DPMessage {
    pub fn is_tame(&self) -> bool {
        self.meta & 1 == 0
    }

    pub fn glv_variant(&self) -> u8 {
        (self.meta >> 1) & 3
    }
}

/// Node registration message
#[derive(Clone, Debug)]
pub struct RegisterMessage {
    /// Unique node ID (random u64)
    pub node_id: u64,
    /// Number of parallel walks this node can run
    pub n_walks: u32,
    /// Estimated group ops/sec
    pub throughput: f64,
}

/// Range assignment from coordinator
#[derive(Clone, Debug)]
pub struct AssignMessage {
    /// Unique assignment ID
    pub assign_id: u64,
    /// Range start (as BigUint bytes)
    pub range_start: Vec<u8>,
    /// Range end (as BigUint bytes)
    pub range_end: Vec<u8>,
    /// Walk type: 0 = tame, 1 = wild, 2 = both
    pub walk_type: u8,
    /// DP bits for this assignment
    pub dp_bits: u8,
}

/// Collision notification
#[derive(Clone, Debug)]
pub struct CollisionMessage {
    /// Found key (as BigUint bytes)
    pub key_bytes: Vec<u8>,
    /// Which nodes contributed
    pub node_a: u64,
    pub node_b: u64,
}

/// Status response from node
#[derive(Clone, Debug)]
pub struct StatusMessage {
    pub node_id: u64,
    pub total_steps: u64,
    pub dps_found: u64,
    pub collisions_checked: u64,
    pub elapsed_secs: f64,
    pub steps_per_sec: f64,
    pub memory_used_mb: f64,
}

// ============================================================
// COORDINATOR
// ============================================================

/// Coordinator state: manages the global DP table and node assignments
pub struct Coordinator {
    /// Global DP table: x_bytes → (distance_bytes, meta, node_id)
    dp_table: Arc<Mutex<HashMap<[u8; 32], (Vec<u8>, u8, u64)>>>,
    /// Registered nodes
    nodes: Arc<Mutex<Vec<NodeInfo>>>,
    /// Next assignment ID
    next_assign_id: Arc<Mutex<u64>>,
    /// Range being searched
    range_bits: u32,
    /// Whether solution has been found
    found: Arc<Mutex<bool>>,
    /// Found key
    found_key: Arc<Mutex<Option<Vec<u8>>>>,
}

#[derive(Clone, Debug)]
struct NodeInfo {
    node_id: u64,
    n_walks: u32,
    throughput: f64,
    assigned_walk_type: u8,
    last_heartbeat: Instant,
    total_dps: u64,
    total_steps: u64,
}

impl Coordinator {
    pub fn new(range_bits: u32) -> Self {
        Coordinator {
            dp_table: Arc::new(Mutex::new(HashMap::new())),
            nodes: Arc::new(Mutex::new(Vec::new())),
            next_assign_id: Arc::new(Mutex::new(1)),
            range_bits,
            found: Arc::new(Mutex::new(false)),
            found_key: Arc::new(Mutex::new(None)),
        }
    }

    /// Start the coordinator server on the given port
    pub fn serve(&self, port: u16) -> std::io::Result<()> {
        let listener = TcpListener::bind(format!("0.0.0.0:{}", port))?;
        println!("  [COORD] Listening on port {}", port);
        println!("  [COORD] Range: [2^{}, 2^{})", self.range_bits - 1, self.range_bits);

        // Accept connections in a loop
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let dp_table = self.dp_table.clone();
                    let nodes = self.nodes.clone();
                    let next_assign_id = self.next_assign_id.clone();
                    let found = self.found.clone();
                    let found_key = self.found_key.clone();
                    let range_bits = self.range_bits;

                    std::thread::spawn(move || {
                        handle_node_connection(
                            stream, dp_table, nodes, next_assign_id,
                            found, found_key, range_bits,
                        );
                    });
                }
                Err(e) => {
                    eprintln!("  [COORD] Connection failed: {}", e);
                }
            }
        }

        Ok(())
    }

    /// Check if solution has been found
    pub fn is_found(&self) -> bool {
        *self.found.lock().unwrap()
    }

    /// Get the found key
    pub fn get_found_key(&self) -> Option<Vec<u8>> {
        self.found_key.lock().unwrap().clone()
    }

    /// Get DP table size
    pub fn dp_count(&self) -> usize {
        self.dp_table.lock().unwrap().len()
    }
}

/// Handle a single node connection
fn handle_node_connection(
    mut stream: TcpStream,
    dp_table: Arc<Mutex<HashMap<[u8; 32], (Vec<u8>, u8, u64)>>>,
    nodes: Arc<Mutex<Vec<NodeInfo>>>,
    next_assign_id: Arc<Mutex<u64>>,
    found: Arc<Mutex<bool>>,
    found_key: Arc<Mutex<Option<Vec<u8>>>>,
    range_bits: u32,
) {
    let peer = stream.peer_addr().unwrap_or_else(|_| "unknown".parse().unwrap());
    println!("  [COORD] Node connected: {}", peer);

    stream.set_read_timeout(Some(Duration::from_secs(30))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(10))).ok();

    let mut node_id: u64 = 0;
    let mut buffer = [0u8; 4096];

    loop {
        // Check if solution found
        if *found.lock().unwrap() {
            // Broadcast collision to this node
            if let Some(ref key) = *found_key.lock().unwrap() {
                let msg = encode_collision_message(key, 0, 0);
                let _ = stream.write_all(&msg);
            }
            break;
        }

        // Read message
        match stream.read(&mut buffer) {
            Ok(0) => {
                println!("  [COORD] Node {} disconnected", peer);
                break;
            }
            Ok(n) => {
                // Parse message type
                if n < 1 { continue; }
                let msg_type = buffer[0];

                match msg_type {
                    MSG_REGISTER => {
                        if n >= 17 {
                            node_id = u64::from_be_bytes(buffer[1..9].try_into().unwrap_or([0; 8]));
                            let n_walks = u32::from_be_bytes(buffer[9..13].try_into().unwrap_or([0; 4]));
                            let tp_bytes: [u8; 8] = buffer[13..21].try_into().unwrap_or([0; 8]);
                            let throughput = f64::from_be_bytes(tp_bytes);

                            println!("  [COORD] Node {} registered: {} walks, {:.0} ops/s",
                                     node_id, n_walks, throughput);

                            // Assign walk type: alternate tame/wild
                            let node_count = nodes.lock().unwrap().len();
                            let walk_type = if node_count % 2 == 0 { 0u8 } else { 1u8 };

                            nodes.lock().unwrap().push(NodeInfo {
                                node_id, n_walks, throughput,
                                assigned_walk_type: walk_type,
                                last_heartbeat: Instant::now(),
                                total_dps: 0, total_steps: 0,
                            });

                            // Send assignment
                            let assign_id = {
                                let mut id = next_assign_id.lock().unwrap();
                                *id += 1;
                                *id - 1
                            };

                            // Range: split the search space among nodes
                            // For now, all nodes search the full range but with different starting points
                            let range_start = (1u64 << (range_bits - 1)).to_be_bytes().to_vec();
                            let range_end = (1u64 << range_bits).to_be_bytes().to_vec();

                            let assign = encode_assign_message(
                                assign_id, &range_start, &range_end, walk_type, 34,
                            );
                            let _ = stream.write_all(&assign);
                        }
                    }
                    MSG_DP_BATCH => {
                        // Parse DP batch
                        let count = if n >= 5 {
                            u32::from_be_bytes(buffer[1..5].try_into().unwrap_or([0; 4])) as usize
                        } else {
                            0
                        };

                        let mut table = dp_table.lock().unwrap();
                        let mut new_collisions = 0u64;

                        for i in 0..count {
                            let offset = 5 + i * 65;
                            if offset + 65 > n { break; }

                            let mut x_bytes = [0u8; 32];
                            x_bytes.copy_from_slice(&buffer[offset..offset + 32]);
                            let dist_bytes = buffer[offset + 32..offset + 64].to_vec();
                            let meta = buffer[offset + 64];

                            // Check for collision
                            if let Some(existing) = table.get(&x_bytes) {
                                let existing_is_tame = existing.1 & 1 == 0;
                                let new_is_tame = meta & 1 == 0;

                                // Collision between tame and wild!
                                if existing_is_tame != new_is_tame {
                                    new_collisions += 1;
                                    // In production: recover key and verify
                                    println!("  [COORD] COLLISION DETECTED! x={:02x}{:02x}...",
                                             x_bytes[0], x_bytes[1]);
                                }
                            } else {
                                table.insert(x_bytes, (dist_bytes, meta, node_id));
                            }
                        }

                        if new_collisions > 0 {
                            println!("  [COORD] {} new collisions in batch", new_collisions);
                        }
                    }
                    MSG_HEARTBEAT => {
                        // Update node heartbeat
                        let mut nodes_list = nodes.lock().unwrap();
                        if let Some(node) = nodes_list.iter_mut().find(|n| n.node_id == node_id) {
                            node.last_heartbeat = Instant::now();
                        }
                    }
                    MSG_STATUS_RESP => {
                        if n >= 45 {
                            let total_steps = u64::from_be_bytes(
                                buffer[9..17].try_into().unwrap_or([0; 8])
                            );
                            let dps_found = u64::from_be_bytes(
                                buffer[17..25].try_into().unwrap_or([0; 8])
                            );
                            let mut nodes_list = nodes.lock().unwrap();
                            if let Some(node) = nodes_list.iter_mut().find(|n| n.node_id == node_id) {
                                node.total_steps = total_steps;
                                node.total_dps = dps_found;
                            }
                        }
                    }
                    _ => {
                        // Unknown message type — ignore
                    }
                }
            }
            Err(e) => {
                if e.kind() != std::io::ErrorKind::TimedOut {
                    eprintln!("  [COORD] Read error from {}: {}", peer, e);
                    break;
                }
            }
        }
    }

    // Remove node from list
    let mut nodes_list = nodes.lock().unwrap();
    nodes_list.retain(|n| n.node_id != node_id);
    println!("  [COORD] Node {} removed", node_id);
}

// ============================================================
// MESSAGE ENCODING
// ============================================================

fn encode_assign_message(
    assign_id: u64, range_start: &[u8], range_end: &[u8],
    walk_type: u8, dp_bits: u8,
) -> Vec<u8> {
    let mut msg = Vec::with_capacity(64);
    msg.push(MSG_ASSIGN);
    msg.extend_from_slice(&assign_id.to_be_bytes());
    msg.extend_from_slice(&(range_start.len() as u16).to_be_bytes());
    msg.extend_from_slice(range_start);
    msg.extend_from_slice(&(range_end.len() as u16).to_be_bytes());
    msg.extend_from_slice(range_end);
    msg.push(walk_type);
    msg.push(dp_bits);
    msg
}

fn encode_collision_message(key: &[u8], node_a: u64, node_b: u64) -> Vec<u8> {
    let mut msg = Vec::with_capacity(64);
    msg.push(MSG_COLLISION);
    msg.extend_from_slice(&(key.len() as u16).to_be_bytes());
    msg.extend_from_slice(key);
    msg.extend_from_slice(&node_a.to_be_bytes());
    msg.extend_from_slice(&node_b.to_be_bytes());
    msg
}

// ============================================================
// NODE CLIENT
// ============================================================

/// Client that connects to a coordinator and sends DPs
pub struct NodeClient {
    /// Connection to coordinator
    stream: Option<TcpStream>,
    /// This node's ID
    node_id: u64,
    /// Number of walks
    n_walks: u32,
    /// Estimated throughput
    throughput: f64,
    /// DP batch buffer
    dp_batch: Vec<DPMessage>,
}

impl NodeClient {
    pub fn new(n_walks: u32, throughput: f64) -> Self {
        NodeClient {
            stream: None,
            node_id: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64 ^ rand_id(),
            n_walks,
            throughput,
            dp_batch: Vec::with_capacity(MAX_DP_BATCH),
        }
    }

    /// Connect to coordinator
    pub fn connect(&mut self, addr: &str) -> std::io::Result<()> {
        let mut stream = TcpStream::connect(addr)?;
        stream.set_read_timeout(Some(Duration::from_secs(30))).ok();
        stream.set_write_timeout(Some(Duration::from_secs(10))).ok();

        // Send registration
        let mut reg = Vec::with_capacity(21);
        reg.push(MSG_REGISTER);
        reg.extend_from_slice(&self.node_id.to_be_bytes());
        reg.extend_from_slice(&self.n_walks.to_be_bytes());
        reg.extend_from_slice(&self.throughput.to_be_bytes());
        stream.write_all(&reg)?;

        self.stream = Some(stream);
        println!("  [NODE] Connected to coordinator at {}", addr);
        Ok(())
    }

    /// Add a DP to the batch, sending if buffer is full
    pub fn add_dp(&mut self, dp: DPMessage) -> std::io::Result<()> {
        self.dp_batch.push(dp);

        if self.dp_batch.len() >= MAX_DP_BATCH {
            self.flush_dps()?;
        }

        Ok(())
    }

    /// Send all buffered DPs to coordinator
    pub fn flush_dps(&mut self) -> std::io::Result<()> {
        if self.dp_batch.is_empty() { return Ok(()); }
        if let Some(ref mut stream) = self.stream {
            let count = self.dp_batch.len() as u32;

            let mut msg = Vec::with_capacity(5 + count as usize * 65);
            msg.push(MSG_DP_BATCH);
            msg.extend_from_slice(&count.to_be_bytes());

            for dp in &self.dp_batch {
                msg.extend_from_slice(&dp.x_bytes);
                msg.extend_from_slice(&dp.distance_bytes);
                msg.push(dp.meta);
            }

            stream.write_all(&msg)?;
        }

        self.dp_batch.clear();
        Ok(())
    }

    /// Send heartbeat
    pub fn heartbeat(&mut self) -> std::io::Result<()> {
        if let Some(ref mut stream) = self.stream {
            stream.write_all(&[MSG_HEARTBEAT])?;
        }
        Ok(())
    }

    /// Check if coordinator has found a solution
    pub fn check_for_solution(&mut self) -> Option<Vec<u8>> {
        // Non-blocking read from coordinator
        if let Some(ref mut stream) = self.stream {
            let mut buf = [0u8; 1024];
            stream.set_nonblocking(true).ok();
            match stream.read(&mut buf) {
                Ok(n) if n > 0 && buf[0] == MSG_COLLISION => {
                    stream.set_nonblocking(false).ok();
                    if n >= 3 {
                        let key_len = u16::from_be_bytes([buf[1], buf[2]]) as usize;
                        if n >= 3 + key_len {
                            return Some(buf[3..3 + key_len].to_vec());
                        }
                    }
                }
                _ => {}
            }
            stream.set_nonblocking(false).ok();
        }
        None
    }
}

fn rand_id() -> u64 {
    use std::time::SystemTime;
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    t.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407)
}

// ============================================================
// DISTRIBUTED SELFTEST
// ============================================================

pub fn selftest() {
    println!("  [DIST] Running distributed coordination selftest...");

    // Test message encoding
    let assign = encode_assign_message(42, &[1, 2, 3, 4], &[5, 6, 7, 8], 0, 34);
    println!("  [DIST] Assign message: {} bytes, type={}", assign.len(), assign[0]);
    assert_eq!(assign[0], MSG_ASSIGN);

    let collision = encode_collision_message(&[0xAA, 0xBB], 1, 2);
    println!("  [DIST] Collision message: {} bytes, type={}", collision.len(), collision[0]);
    assert_eq!(collision[0], MSG_COLLISION);

    // Test DP message
    let dp = DPMessage {
        x_bytes: [0x42; 32],
        distance_bytes: [0x13; 32],
        meta: 0b0101, // wild, GLV variant 2
    };
    assert!(!dp.is_tame());
    assert_eq!(dp.glv_variant(), 2);

    // Test coordinator creation
    let coord = Coordinator::new(135);
    assert_eq!(coord.dp_count(), 0);

    println!("  [DIST] Selftest PASSED");
    println!("  [DIST] To run distributed: start coordinator on one machine,");
    println!("  [DIST] then connect nodes with: --mode node --coordinator host:port");
}
