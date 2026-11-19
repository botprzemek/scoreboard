use crate::common::state::{GAME_STATE, IS_CONNECTED_TO_MASTER};

use core::sync::atomic::Ordering;
use scoreboard_basketball::Game;
use scoreboard_net::event::{ClientEvent, ServerEvent};
use scoreboard_net::layout::{DATA_SIZE, PACKET_SIZE};
use scoreboard_net::{
    ClientPacket, Device, DeviceConfigPacket, DeviceType, HandshakeAck, Heartbeat, ServerPacket,
};

pub const MASTER_UDP_PORT: u16 = 9000;
pub const WORKER_UDP_PORT: u16 = 9001;
pub const DEFAULT_MAX_MISSED_HEARTBEATS: u8 = 3;

#[derive(Debug)]
pub enum HandshakePacketResult {
    DeviceConfig(DeviceConfigPacket),
    Connected {
        session_token: u32,
        assigned_game_id: u64,
    },
    Ignored,
}

#[derive(Debug)]
pub enum ConnectedPacketResult {
    DeviceConfig(DeviceConfigPacket),
    HeartbeatAck,
    HandshakeAck,
    GameUpdated,
    InvalidToken,
    Ignored,
}

/// Platform-independent UDP protocol state machine for a worker node.
pub struct WorkerProtocol {
    device_bytes: [u8; DATA_SIZE],
    session_token: u32,
    sequence_id: u16,
    missed_heartbeats: u8,
    max_missed_heartbeats: u8,
}

impl WorkerProtocol {
    pub fn new(device: Device) -> Self {
        Self {
            device_bytes: device.to_bytes(),
            session_token: 0,
            sequence_id: 0,
            missed_heartbeats: 0,
            max_missed_heartbeats: DEFAULT_MAX_MISSED_HEARTBEATS,
        }
    }

    pub fn fallback_device(serial: &str, id: &str) -> Device {
        Device::try_new(DeviceType::Worker, serial, id).expect("valid default worker device")
    }

    pub fn session_token(&self) -> u32 {
        self.session_token
    }

    pub fn reset_session(&mut self) {
        self.session_token = 0;
        self.sequence_id = 0;
        self.missed_heartbeats = 0;
        IS_CONNECTED_TO_MASTER.store(false, Ordering::Release);
    }

    pub fn next_handshake_request(&mut self) -> [u8; PACKET_SIZE] {
        self.sequence_id = self.sequence_id.wrapping_add(1);
        ClientPacket::new(
            ClientEvent::HandshakeRequest,
            self.sequence_id,
            self.device_bytes,
        )
        .to_bytes()
    }

    pub fn next_heartbeat(&mut self) -> [u8; PACKET_SIZE] {
        self.sequence_id = self.sequence_id.wrapping_add(1);
        let heartbeat_bytes = Heartbeat::new(0).to_bytes();
        ClientPacket::with_session(
            ClientEvent::HandshakeHeartbeat,
            self.sequence_id,
            self.session_token,
            Heartbeat::PAYLOAD_LEN,
            heartbeat_bytes,
        )
        .to_bytes()
    }

    /// Record a transmitted heartbeat and return `true` if the master connection timed out.
    pub fn record_heartbeat_sent(&mut self) -> bool {
        self.missed_heartbeats = self.missed_heartbeats.saturating_add(1);
        if self.missed_heartbeats >= self.max_missed_heartbeats {
            self.reset_session();
            true
        } else {
            false
        }
    }

    /// Process a packet received while searching for Master (handshake phase).
    pub fn handle_handshake_packet(&mut self, raw: &[u8]) -> HandshakePacketResult {
        if let Ok(config_pkt) = DeviceConfigPacket::from_bytes(raw) {
            return HandshakePacketResult::DeviceConfig(config_pkt);
        }

        if raw.len() != PACKET_SIZE {
            return HandshakePacketResult::Ignored;
        }

        let Ok(bytes): Result<[u8; PACKET_SIZE], _> = raw.try_into() else {
            return HandshakePacketResult::Ignored;
        };
        let Ok(server_packet) = ServerPacket::from_bytes(bytes) else {
            return HandshakePacketResult::Ignored;
        };

        if server_packet.event() != ServerEvent::HandshakeAck {
            return HandshakePacketResult::Ignored;
        }

        let session_token = server_packet.session_token();
        let assigned_game_id = HandshakeAck::from_bytes(&server_packet.data())
            .map(|ack| ack.assigned_game_id())
            .unwrap_or(0);

        self.session_token = session_token;
        self.missed_heartbeats = 0;
        self.sequence_id = 0;
        IS_CONNECTED_TO_MASTER.store(true, Ordering::Release);

        HandshakePacketResult::Connected {
            session_token,
            assigned_game_id,
        }
    }

    /// Process a packet received while connected to Master.
    pub fn handle_connected_packet(&mut self, raw: &[u8]) -> ConnectedPacketResult {
        if let Ok(config_pkt) = DeviceConfigPacket::from_bytes(raw) {
            return ConnectedPacketResult::DeviceConfig(config_pkt);
        }

        if raw.len() != PACKET_SIZE {
            return ConnectedPacketResult::Ignored;
        }

        let Ok(bytes): Result<[u8; PACKET_SIZE], _> = raw.try_into() else {
            return ConnectedPacketResult::Ignored;
        };
        let Ok(packet) = ServerPacket::from_bytes(bytes) else {
            return ConnectedPacketResult::Ignored;
        };

        let packet_token = packet.session_token();
        if self.session_token != 0 && packet_token != self.session_token {
            return ConnectedPacketResult::InvalidToken;
        }

        match packet.event() {
            ServerEvent::HeartbeatAck => {
                self.missed_heartbeats = 0;
                ConnectedPacketResult::HeartbeatAck
            }
            ServerEvent::HandshakeAck => {
                self.missed_heartbeats = 0;
                if packet_token != 0 {
                    self.session_token = packet_token;
                }
                ConnectedPacketResult::HandshakeAck
            }
            ServerEvent::GameUpdate => {
                self.missed_heartbeats = 0;
                if let Ok(game) = Game::try_from(packet) {
                    GAME_STATE.set(game);
                    ConnectedPacketResult::GameUpdated
                } else {
                    ConnectedPacketResult::Ignored
                }
            }
            _ => ConnectedPacketResult::Ignored,
        }
    }

    pub fn device_config_ack_bytes() -> [u8; PACKET_SIZE] {
        ServerPacket::new(ServerEvent::DeviceConfigAck, 0, [0u8; DATA_SIZE]).to_bytes()
    }

    pub fn device_config_success_bytes() -> [u8; PACKET_SIZE] {
        ServerPacket::new(ServerEvent::DeviceConfigSuccess, 0, [0u8; DATA_SIZE]).to_bytes()
    }
}
