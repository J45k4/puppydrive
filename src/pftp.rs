// pftp.rs - Puppy File Transfer Protocol State Machine in Rust
// This is a single-file implementation of PFTP as an async state machine.
// It uses Tokio for async UDP, ring for crypto, and byteorder for serialization.
// Run as: cargo run -- sender <file> <receiver_addr> or cargo run -- receiver <output_dir> <bind_addr>
// Dependencies: tokio = { version = "1", features = ["full"] }, ring = "0.17", byteorder = "1", sha2 = "0.10", rand = "0.8"

// Note: This is a simplified implementation for demonstration. Add error handling, FEC, and full resuming as needed.

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
//use rand::{rngs::OsRng, RngCore};
use ring::aead::{Aad, LessSafeKey, Nonce, AES_256_GCM, NONCE_LEN, TAG_LEN};
use ring::agreement::{self, EphemeralPrivateKey, UnparsedPublicKey, X25519};
use ring::digest::{Context, SHA256};
use ring::rand::{SecureRandom, SystemRandom};
use sha2::Digest;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use tokio::time::timeout;

// Constants
const MTU: usize = 1400;
const HEADER_SIZE: usize = 16;
const BLOCK_SIZE: usize = 1_048_576; // 1MB
const PAYLOAD_SIZE: usize = MTU - HEADER_SIZE - 10; // Approx, adjust for type-specific fields
const VERSION: u8 = 0x01;
const TIMEOUT: Duration = Duration::from_secs(5);

// Packet Types
#[derive(Debug, Clone, Copy, PartialEq)]
enum PacketType {
    Handshake = 0x00,
    Manifest = 0x01,
    DataBlock = 0x02,
    AckSack = 0x03,
    Nak = 0x04,
    Close = 0x05,
    ResumeRequest = 0x06,
}

// Flags
bitflags::bitflags! {
    struct Flags: u8 {
        const ENCRYPTION = 0b00000001;
        const FEC = 0b00000010;
        const RESUME = 0b00000100;
    }
}

// Protocol State
#[derive(Debug, Clone, Copy, PartialEq)]
enum PftpState {
    Init,
    Handshaking,
    ManifestSent,
    Transferring,
    Resuming,
    Closing,
    Done,
    Error,
}

// Session
struct PftpSession {
    state: PftpState,
    socket: Arc<UdpSocket>,
    peer_addr: SocketAddr,
    session_id: u32,
    private_key: Option<EphemeralPrivateKey>,
    key: Option<LessSafeKey>,
    nonce_counter: u64,
    sequence: u32,
    file_size: u64,
    num_blocks: u32,
    block_hashes: Vec<[u8; 32]>,
    blocks_sent: HashMap<u32, bool>, // Block ID -> sent
    blocks_acked: HashMap<u32, bool>, // Block ID -> acked
    cumulative_ack: u32,
    file: Option<BufReader<File>>, // For sender
    output_file: Option<BufWriter<File>>, // For receiver
    received_packets: HashMap<u32, Vec<u8>>, // Block ID -> reassembled data
}

impl PftpSession {
    async fn new(socket: Arc<UdpSocket>, peer_addr: SocketAddr, is_sender: bool) -> Self {
        let rng = SystemRandom::new();
        let mut session_id_bytes = [0u8; 4];
        rng.fill(&mut session_id_bytes).unwrap();
        let session_id = u32::from_be_bytes(session_id_bytes);

        Self {
            state: PftpState::Init,
            socket,
            peer_addr,
            session_id,
            private_key: None,
            key: None,
            nonce_counter: 0,
            sequence: 0,
            file_size: 0,
            num_blocks: 0,
            block_hashes: Vec::new(),
            blocks_sent: HashMap::new(),
            blocks_acked: HashMap::new(),
            cumulative_ack: 0,
            file: None,
            output_file: None,
            received_packets: HashMap::new(),
        }
    }

    async fn transition(&mut self, new_state: PftpState) {
        self.state = new_state;
    }

    async fn process(&mut self, is_sender: bool, file_path: &str, output_dir: &str) -> Result<(), io::Error> {
        while self.state != PftpState::Done && self.state != PftpState::Error {
            match self.state {
                PftpState::Init => {
                    if is_sender {
                        self.prepare_sender(file_path)?;
                    } else {
                        self.prepare_receiver(output_dir)?;
                    }
                    self.transition(PftpState::Handshaking).await;
                }
                PftpState::Handshaking => {
                    self.handle_handshake(is_sender).await?;
                    self.transition(PftpState::ManifestSent).await;
                }
                PftpState::ManifestSent => {
                    self.handle_manifest(is_sender).await?;
                    self.transition(PftpState::Transferring).await;
                }
                PftpState::Transferring => {
                    self.handle_transfer(is_sender).await?;
                    self.transition(PftpState::Closing).await;
                }
                PftpState::Resuming => {
                    self.handle_resuming(is_sender).await?;
                }
                PftpState::Closing => {
                    self.handle_close(is_sender).await?;
                    self.transition(PftpState::Done).await;
                }
                _ => break,
            }
        }
        Ok(())
    }

    fn prepare_sender(&mut self, file_path: &str) -> Result<(), io::Error> {
        let file = File::open(file_path)?;
        self.file = Some(BufReader::new(file));
        self.file_size = self.file.as_ref().unwrap().get_ref().metadata()?.len();
        self.num_blocks = ((self.file_size + BLOCK_SIZE as u64 - 1) / BLOCK_SIZE as u64) as u32;
        let mut buffer = vec![0u8; BLOCK_SIZE];
        for i in 0..self.num_blocks {
            let read_size = BLOCK_SIZE.min((self.file_size - (i as u64 * BLOCK_SIZE as u64)) as usize);
            self.file.as_mut().unwrap().read_exact(&mut buffer[0..read_size])?;
            let mut context = Context::new(&SHA256);
            context.update(&buffer[0..read_size]);
            let digest = context.finish();
            self.block_hashes.push(digest.as_ref().try_into().unwrap());
        }
        Ok(())
    }

    fn prepare_receiver(&mut self, output_dir: &str) -> Result<(), io::Error> {
        let path = Path::new(output_dir).join("received_file");
        let file = OpenOptions::new().write(true).create(true).open(path)?;
        self.output_file = Some(BufWriter::new(file));
        Ok(())
    }

    async fn handle_handshake(&mut self, is_sender: bool) -> Result<(), io::Error> {
        let rng = SystemRandom::new();
        let private_key = EphemeralPrivateKey::generate(&X25519, &rng)
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "key gen failed"))?;
        let public_key = private_key.compute_public_key().unwrap();
        self.private_key = Some(private_key);

        let mut packet = Vec::new();
        packet.write_u8(VERSION)?;
        packet.write_u8(PacketType::Handshake as u8)?;
        packet.write_u32::<BigEndian>(self.session_id)?;
        packet.write_u32::<BigEndian>(self.sequence)?;
        self.sequence += 1;
        packet.write_u32::<BigEndian>(Instant::now().elapsed().as_millis() as u32)?;
        packet.write_u16::<BigEndian>(0)?; // Checksum placeholder
        let flags = Flags::ENCRYPTION | Flags::RESUME;
        packet.write_u8(flags.bits())?;
        packet.extend_from_slice(public_key.as_ref());
        packet.write_u16::<BigEndian>(MTU as u16)?;

        let checksum = crc16(&packet[HEADER_SIZE..]);
        packet[14..16].copy_from_slice(&checksum.to_be_bytes());

        self.socket.send_to(&packet, self.peer_addr).await?;

        let mut buf = vec![0u8; MTU];
        let (len, _) = timeout(TIMEOUT, self.socket.recv_from(&mut buf)).await??;
        buf.truncate(len);

        // Parse handshake ACK (simplified)
        let peer_public = UnparsedPublicKey::new(&X25519, &buf[HEADER_SIZE + 1 + 32..]); // Adjust offset
        let shared = agreement::agree_ephemeral(
            self.private_key.take().unwrap(),
            &peer_public,
            |key_material| Ok::<_, ring::error::Unspecified>(key_material.to_vec()),
        )
        .map_err(|_| io::Error::new(io::ErrorKind::Other, "key agreement failed"))?;
        let key_bytes = derive_key(&shared); // Implement derive_key
        let unbound_key = ring::aead::UnboundKey::new(&AES_256_GCM, &key_bytes).unwrap();
        self.key = Some(LessSafeKey::new(unbound_key));

        Ok(())
    }

    async fn handle_manifest(&mut self, is_sender: bool) -> Result<(), io::Error> {
        if is_sender {
            let mut packet = Vec::new();
            packet.write_u8(VERSION)?;
            packet.write_u8(PacketType::Manifest as u8)?;
            packet.write_u32::<BigEndian>(self.session_id)?;
            packet.write_u32::<BigEndian>(self.sequence)?;
            self.sequence += 1;
            packet.write_u32::<BigEndian>(Instant::now().elapsed().as_millis() as u32)?;
            packet.write_u16::<BigEndian>(0)?;
            packet.write_u64::<BigEndian>(self.file_size)?;
            packet.write_u32::<BigEndian>(BLOCK_SIZE as u32)?;
            packet.write_u32::<BigEndian>(self.num_blocks)?;
            packet.write_u8(0x01)?; // SHA-256
            for hash in &self.block_hashes {
                packet.extend_from_slice(hash);
            }
            let nonce = self.get_nonce();
            if let Some(key) = &self.key {
                let mut data = packet[HEADER_SIZE..].to_vec();
                key.seal_in_place_separate_tag(nonce, Aad::empty(), &mut data).unwrap();
                packet.truncate(HEADER_SIZE);
                packet.extend_from_slice(&data);
            }

            let checksum = crc16(&packet[HEADER_SIZE..]);
            packet[14..16].copy_from_slice(&checksum.to_be_bytes());

            self.socket.send_to(&packet, self.peer_addr).await?;
        }

        // Wait for ACK (simplified for both sides)
        // ...
        Ok(())
    }

    async fn handle_transfer(&mut self, is_sender: bool) -> Result<(), io::Error> {
        // Simplified: Send blocks, handle SACK/NAK
        // For sender: read from file, encrypt, split into packets, send
        // For receiver: receive, decrypt, reassemble, verify hash, write, send SACK
        // ...
        Ok(())
    }

    async fn handle_resuming(&mut self, is_sender: bool) -> Result<(), io::Error> {
        // Send Resume Request, update from last ACK
        // ...
        Ok(())
    }

    async fn handle_close(&mut self, is_sender: bool) -> Result<(), io::Error> {
        // Send Close, confirm
        // ...
        Ok(())
    }

    fn get_nonce(&mut self) -> Nonce {
        let mut nonce_bytes = [0u8; NONCE_LEN];
        nonce_bytes[0..8].copy_from_slice(&self.nonce_counter.to_be_bytes());
        self.nonce_counter += 1;
        Nonce::assume_unique_for_key(nonce_bytes)
    }
}

// CRC16 placeholder
fn crc16(data: &[u8]) -> u16 {
    0 // Implement CRC-16-CCITT or similar
}

fn derive_key(shared: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(shared);
    let result = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&result);
    key
}