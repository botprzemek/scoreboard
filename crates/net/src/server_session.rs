//! Transport-agnostic server-side session state machine (`no_std`).
//!
//! Enforces worker registration lifecycle at compile time:
//! `Unregistered` -> `Registered` -> `Unregistered`.

use crate::data::{Device, HandshakeAck, Heartbeat, HeartbeatAck};
use crate::event::{ClientEvent, ServerEvent};
use crate::layout::DATA_SIZE;
use crate::node::NetError;
use crate::packet::{ClientPacket, ServerPacket};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unregistered;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Registered {
    pub session_token: u32,
    pub assigned_game_id: u64,
    pub sequence_id: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerSession<S> {
    state: S,
}

impl Default for ServerSession<Unregistered> {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerSession<Unregistered> {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: Unregistered,
        }
    }

    pub fn accept_handshake(
        self,
        packet: &ClientPacket,
        session_token: u32,
        assigned_game_id: u64,
    ) -> Result<(ServerSession<Registered>, ServerPacket, Device), NetError> {
        if packet.event() != ClientEvent::HandshakeRequest {
            return Err(NetError::InvalidPacket("expected HandshakeRequest"));
        }

        let device = Device::try_from(*packet)
            .map_err(|_| NetError::InvalidPacket("invalid Device handshake payload"))?;

        let seq = packet.sequence_id().wrapping_add(1);
        let ack_payload = HandshakeAck::new(assigned_game_id, 0);

        let ack_packet = ServerPacket::with_session(
            ServerEvent::HandshakeAck,
            seq,
            session_token,
            HandshakeAck::PAYLOAD_LEN,
            ack_payload.to_bytes(),
        );

        let next_session = ServerSession {
            state: Registered {
                session_token,
                assigned_game_id,
                sequence_id: seq,
            },
        };

        Ok((next_session, ack_packet, device))
    }
}

impl ServerSession<Registered> {
    #[must_use]
    pub const fn session_token(&self) -> u32 {
        self.state.session_token
    }

    pub fn receive_heartbeat(&mut self, packet: &ClientPacket) -> Result<Heartbeat, NetError> {
        if packet.event() != ClientEvent::HandshakeHeartbeat {
            return Err(NetError::InvalidPacket("expected HandshakeHeartbeat"));
        }
        if packet.session_token() != self.state.session_token {
            return Err(NetError::InvalidPacket("session token mismatch"));
        }

        Heartbeat::from_bytes(&packet.data())
            .map_err(|_| NetError::InvalidPacket("invalid Heartbeat payload"))
    }

    pub fn send_heartbeat_ack(&mut self, latest_game_seq: u16) -> ServerPacket {
        self.state.sequence_id = self.state.sequence_id.wrapping_add(1);
        let ack_payload = HeartbeatAck::new(latest_game_seq, 0);
        ServerPacket::with_session(
            ServerEvent::HeartbeatAck,
            self.state.sequence_id,
            self.state.session_token,
            HeartbeatAck::PAYLOAD_LEN,
            ack_payload.to_bytes(),
        )
    }

    pub fn send_game_update(&mut self, game_bytes: [u8; DATA_SIZE]) -> ServerPacket {
        self.state.sequence_id = self.state.sequence_id.wrapping_add(1);
        ServerPacket::with_session(
            ServerEvent::GameUpdate,
            self.state.sequence_id,
            self.state.session_token,
            DATA_SIZE as u8,
            game_bytes,
        )
    }

    pub fn expire(self) -> ServerSession<Unregistered> {
        ServerSession::new()
    }
}
