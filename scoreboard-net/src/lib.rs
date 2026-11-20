#![no_std]

#[macro_use]
mod macros;

pub mod client_session;
mod data;
mod error;
pub mod event;
pub mod game;
pub mod layout;
mod node;
mod packet;
mod protocol;
mod server_session;
mod transport;

pub use data::{
    Device, DeviceConfig, DeviceConfig as DeviceConfigPacket, DeviceType, Handshake, HandshakeAck,
    Heartbeat, HeartbeatAck,
};
pub use error::NetError;
pub use event::{ClientEvent, ServerEvent};
pub use node::{Client, ClientHandler, Node, PacketHandler, Server, ServerHandler};
pub use packet::{ClientPacket, Packet, ServerPacket, crc16_ccitt};
pub use protocol::{ClientReaderProtocol, ClientWriterProtocol, ServerReaderProtocol};
pub use server_session::{Registered, ServerSession, Unregistered};
pub use transport::{
    PacketReceiver, PacketSender, RawConnection, RawPacket, SplitTransport, Transport,
};

pub const MASTER_UDP_PORT: u16 = 9000;
pub const WORKER_UDP_PORT: u16 = 9000;
