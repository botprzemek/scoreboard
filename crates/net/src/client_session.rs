use core::marker::PhantomData;

use crate::data::{Handshake, Heartbeat};
use crate::event::{ClientEvent, ServerEvent};
use crate::game::{GameIdle, GameSession};
use crate::layout::DATA_SIZE;
use crate::packet::{ClientPacket, ServerPacket};
use crate::transport::{PacketReceiver, PacketSender};

pub struct Disconnected;
pub struct Connected;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum SessionError {
    InvalidPacket(&'static str),
    UnexpectedEvent(ServerEvent),
}

pub struct ClientSession<T, State> {
    transport: T,
    sequence_id: u16,
    session_token: u32,
    _state: PhantomData<State>,
}

impl<T, State> ClientSession<T, State> {
    #[must_use]
    pub fn transport(&self) -> &T {
        &self.transport
    }

    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }

    #[must_use]
    pub fn into_transport(self) -> T {
        self.transport
    }

    #[must_use]
    pub fn sequence_id(&self) -> u16 {
        self.sequence_id
    }

    pub(crate) fn next_sequence_id(&mut self) -> u16 {
        let id = self.sequence_id;

        self.sequence_id = self.sequence_id.wrapping_add(1);

        id
    }
}

impl<T> ClientSession<T, Disconnected> {
    #[must_use]
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            sequence_id: 0,
            session_token: 0,
            _state: PhantomData,
        }
    }
}

impl<T: PacketSender + PacketReceiver> ClientSession<T, Disconnected> {
    pub async fn handshake(
        mut self,
        handshake: &Handshake,
    ) -> Result<ClientSession<T, Connected>, (Self, SessionError)> {
        let seq = self.next_sequence_id();
        let (len, data) = pad_payload(&handshake.to_bytes());
        let packet = ClientPacket::with_session(ClientEvent::HandshakeRequest, seq, 0, len, data);
        self.transport.send_packet(&packet.to_bytes()).await;

        match self.receive_handshake_ack().await {
            Ok(session_token) => Ok(ClientSession {
                transport: self.transport,
                sequence_id: self.sequence_id,
                session_token,
                _state: PhantomData,
            }),
            Err(err) => Err((self, err)),
        }
    }

    async fn receive_handshake_ack(&mut self) -> Result<u32, SessionError> {
        let raw = self
            .transport
            .receive_packet()
            .await
            .map_err(SessionError::InvalidPacket)?;
        let response =
            ServerPacket::from_bytes(raw.bytes).map_err(SessionError::InvalidPacket)?;

        if response.event() != ServerEvent::HandshakeAck {
            return Err(SessionError::UnexpectedEvent(response.event()));
        }

        Ok(response.session_token())
    }
}

fn pad_payload(bytes: &[u8]) -> (u8, [u8; DATA_SIZE]) {
    let mut data = [0u8; DATA_SIZE];
    let len = bytes.len().min(DATA_SIZE);
    data[..len].copy_from_slice(&bytes[..len]);
    (len as u8, data)
}

impl<T> ClientSession<T, Connected> {
    #[must_use]
    pub fn session_token(&self) -> u32 {
        self.session_token
    }

    #[must_use]
    pub fn into_game(self, game_id: u64) -> GameSession<T, GameIdle> {
        GameSession::new(self, game_id)
    }

    pub(crate) fn build_packet(
        &mut self,
        event: ClientEvent,
        payload_len: u8,
        data: [u8; DATA_SIZE],
    ) -> ClientPacket {
        let seq = self.next_sequence_id();

        ClientPacket::with_session(event, seq, self.session_token, payload_len, data)
    }
}

impl<T: PacketSender> ClientSession<T, Connected> {
    pub(crate) async fn send_event(
        &mut self,
        event: ClientEvent,
        payload_len: u8,
        data: [u8; DATA_SIZE],
    ) {
        let packet = self.build_packet(event, payload_len, data);

        self.transport.send_packet(&packet.to_bytes()).await;
    }

    pub async fn heartbeat(&mut self, heartbeat: &Heartbeat) {
        let mut data = [0u8; DATA_SIZE];
        let bytes = heartbeat.to_bytes();
        let len = bytes.len().min(DATA_SIZE);

        data[..len].copy_from_slice(&bytes[..len]);

        self.send_event(ClientEvent::HandshakeHeartbeat, len as u8, data)
            .await;
    }

    pub async fn create_game(&mut self, discipline: u8) {
        let mut data = [0u8; DATA_SIZE];

        data[0] = discipline;

        self.send_event(ClientEvent::GameCreate, 1, data).await;
    }

    pub async fn delete_game(&mut self, game_id: u64) {
        let data = Self::game_id_payload(game_id);

        self.send_event(ClientEvent::GameDelete, 8, data).await;
    }

    pub async fn select_game(&mut self, game_id: u64) {
        let data = Self::game_id_payload(game_id);

        self.send_event(ClientEvent::GameSelect, 8, data).await;
    }

    fn game_id_payload(game_id: u64) -> [u8; DATA_SIZE] {
        let mut data = [0u8; DATA_SIZE];

        data[..8].copy_from_slice(&game_id.to_le_bytes());

        data
    }

    pub async fn disconnect(mut self) -> ClientSession<T, Disconnected> {
        self.send_event(ClientEvent::DeviceDisconnect, 0, [0u8; DATA_SIZE])
            .await;

        ClientSession {
            transport: self.transport,
            sequence_id: self.sequence_id,
            session_token: 0,
            _state: PhantomData,
        }
    }
}
