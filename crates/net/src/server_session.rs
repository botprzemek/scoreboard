//! Transport-agnostic server-side session state machine (`no_std`).
//!
//! Enforces worker registration lifecycle at compile time:
//! `Unregistered` -> `Registered` -> `Unregistered`.

use crate::data::{Device, HandshakeAck, Heartbeat, HeartbeatAck};
use crate::event::{ClientEvent, ServerEvent};
use crate::layout::DATA_SIZE;
use crate::node::NetError;
use crate::packet::{ClientPacket, ServerPacket};

/// Marker types representing server worker-registration states.
pub mod server_state {
    /// Worker is not registered or session has timed out.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Unregistered;

    /// Worker is registered and authenticated by `session_token`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Registered {
        pub session_token: u32,
        pub assigned_game_id: u16,
        pub sequence_id: u16,
    }
}

/// Transport-agnostic server-side session tracker for a remote worker node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerSession<S> {
    state: S,
}

impl Default for ServerSession<server_state::Unregistered> {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerSession<server_state::Unregistered> {
    /// Creates a new unregistered server session tracker.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: server_state::Unregistered,
        }
    }

    /// Validates an inbound `HandshakeRequest` packet, builds `HandshakeAck`,
    /// and transitions into [`server_state::Registered`].
    pub fn accept_handshake(
        self,
        packet: &ClientPacket,
        session_token: u32,
        assigned_game_id: u16,
    ) -> Result<(ServerSession<server_state::Registered>, ServerPacket, Device), NetError> {
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
            state: server_state::Registered {
                session_token,
                assigned_game_id,
                sequence_id: seq,
            },
        };

        Ok((next_session, ack_packet, device))
    }
}

impl ServerSession<server_state::Registered> {
    /// Returns the session token for this worker.
    #[must_use]
    pub const fn session_token(&self) -> u32 {
        self.state.session_token
    }

    /// Validates an inbound heartbeat packet against the active session token.
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

    /// Generates a `HeartbeatAck` packet in response to a heartbeat.
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

    /// Generates a `GameUpdate` packet containing serialized game state.
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

    /// Terminates the worker session on timeout or disconnect.
    pub fn expire(self) -> ServerSession<server_state::Unregistered> {
        ServerSession::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::DeviceType;

    #[test]
    fn test_server_session_lifecycle() {
        let device = Device::try_new(DeviceType::Worker, "sn-srv-01", "worker-01").unwrap();
        let hs_req = ClientPacket::new(ClientEvent::HandshakeRequest, 1, device.to_bytes());

        let (mut session, ack, parsed_dev) = ServerSession::new()
            .accept_handshake(&hs_req, 0xABCD_1234, 7)
            .expect("valid handshake");
        assert_eq!(ack.event(), ServerEvent::HandshakeAck);
        assert_eq!(ack.session_token(), 0xABCD_1234);
        assert_eq!(parsed_dev.game_id(), 0);
        assert_eq!(session.session_token(), 0xABCD_1234);

        let hb = ClientPacket::with_session(
            ClientEvent::HandshakeHeartbeat,
            2,
            0xABCD_1234,
            Heartbeat::PAYLOAD_LEN,
            Heartbeat::new(0).to_bytes(),
        );
        assert!(session.receive_heartbeat(&hb).is_ok());

        let hb_ack = session.send_heartbeat_ack(10);
        assert_eq!(hb_ack.event(), ServerEvent::HeartbeatAck);
        assert_eq!(hb_ack.session_token(), 0xABCD_1234);

        let game_upd = session.send_game_update([0x11; DATA_SIZE]);
        assert_eq!(game_upd.event(), ServerEvent::GameUpdate);
        assert_eq!(game_upd.session_token(), 0xABCD_1234);

        let _ = session.expire();
    }
}
