use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use crate::config::Config;
use crate::services::Services;

use config::DeviceConfig;
use net::layout::PACKET_SIZE;
use net::{ClientEvent, ClientPacket, Device, ServerSession};
use tokio::net::UdpSocket;
use tokio::sync::mpsc;

use super::worker_registry::WorkerRegistry;

const OUTBOUND_QUEUE_CAPACITY: usize = 4096;

struct OutboundAck {
    packet_bytes: [u8; PACKET_SIZE],
    target: SocketAddr,
}

pub struct WorkerDriver {
    listener: Arc<UdpSocket>,
    worker_timeout: Duration,
}

impl WorkerDriver {
    #[must_use]
    pub fn new(listener: UdpSocket) -> Self {
        Self {
            listener: Arc::new(listener),
            worker_timeout: Config::default().worker_timeout,
        }
    }

    #[must_use]
    pub fn with_worker_timeout(mut self, worker_timeout: Duration) -> Self {
        self.worker_timeout = worker_timeout;
        self
    }

    pub fn spawn(self, services: Arc<Services>) {
        tokio::spawn(self.run(services));
    }

    async fn run(self, services: Arc<Services>) {
        let registry = WorkerRegistry::new();
        let (outbound_tx, outbound_rx) = mpsc::channel::<OutboundAck>(OUTBOUND_QUEUE_CAPACITY);

        registry.spawn_watchdog(Arc::clone(&services), self.worker_timeout);
        registry.spawn_tick_sync(Arc::clone(&self.listener), Arc::clone(&services));

        tokio::join!(
            Self::handle_outbound_queue(Arc::clone(&self.listener), outbound_rx),
            Self::handle_disconnects(registry.clone(), services.devices().subscribe_disconnect()),
            Self::handle_incoming(self.listener, services, registry, outbound_tx),
        );
    }

    async fn handle_outbound_queue(
        socket: Arc<UdpSocket>,
        mut outbound_rx: mpsc::Receiver<OutboundAck>,
    ) {
        while let Some(outbound) = outbound_rx.recv().await {
            let _ = socket
                .send_to(&outbound.packet_bytes, outbound.target)
                .await;
        }
    }

    async fn handle_disconnects(
        registry: WorkerRegistry,
        mut disconnect_rx: tokio::sync::broadcast::Receiver<String>,
    ) {
        while let Ok(target_serial_number) = disconnect_rx.recv().await {
            if registry.remove(&target_serial_number) {
                println!("INFO: [UDP] Forcefully disconnected worker sn={target_serial_number}");
            }
        }
    }

    async fn handle_incoming(
        socket: Arc<UdpSocket>,
        services: Arc<Services>,
        registry: WorkerRegistry,
        outbound_tx: mpsc::Sender<OutboundAck>,
    ) {
        let mut sequence_id: u16 = 0;
        let mut receive_buffer = [0u8; PACKET_SIZE];

        loop {
            let (size, remote_address) = match socket.recv_from(&mut receive_buffer).await {
                Ok(result) => result,
                Err(_) => continue,
            };

            Self::handle_datagram(
                &receive_buffer[..size],
                remote_address,
                &services,
                &registry,
                &outbound_tx,
                &mut sequence_id,
            );
        }
    }

    fn handle_datagram(
        data: &[u8],
        remote_address: SocketAddr,
        services: &Services,
        registry: &WorkerRegistry,
        outbound_tx: &mpsc::Sender<OutboundAck>,
        sequence_id: &mut u16,
    ) {
        let packet_array = match data.try_into() {
            Ok(array) => array,
            Err(_) => return,
        };
        let packet = match ClientPacket::from_bytes(packet_array) {
            Ok(packet) => packet,
            Err(_) => return,
        };

        match packet.event() {
            ClientEvent::HandshakeHeartbeat => {
                Self::handle_heartbeat(&packet, remote_address, registry, outbound_tx, sequence_id);
            }
            ClientEvent::HandshakeRequest => {
                Self::handle_handshake(packet, remote_address, services, registry, outbound_tx);
            }
            _ => {}
        }
    }

    fn handle_heartbeat(
        packet: &ClientPacket,
        remote_address: SocketAddr,
        registry: &WorkerRegistry,
        outbound_tx: &mpsc::Sender<OutboundAck>,
        sequence_id: &mut u16,
    ) {
        let token = packet.session_token();
        if token == 0 {
            return;
        }

        let mut worker = match registry.touch_heartbeat(remote_address, token) {
            Some(worker) => worker,
            None => return,
        };
        if worker.session.receive_heartbeat(packet).is_err() {
            return;
        }

        *sequence_id = sequence_id.wrapping_add(1);
        let ack_packet = worker.session.send_heartbeat_ack(*sequence_id);
        Self::send_packet(outbound_tx, remote_address, ack_packet.to_bytes());
    }

    fn handle_handshake(
        packet: ClientPacket,
        remote_address: SocketAddr,
        services: &Services,
        registry: &WorkerRegistry,
        outbound_tx: &mpsc::Sender<OutboundAck>,
    ) {
        let device = match Self::register_worker(&packet, remote_address, services) {
            Some(device) => device,
            None => return,
        };

        let assigned_game_id = device.game_id();
        let serial_ref = match device.serial_number() {
            Ok(serial) => serial,
            Err(_) => return,
        };

        let (session_token, is_new_or_moved) = registry
            .existing_token(serial_ref, remote_address)
            .unwrap_or_else(|| (Self::generate_session_token(), true));

        let (mut session, ack_packet, _) =
            match ServerSession::new().accept_handshake(&packet, session_token, assigned_game_id) {
                Ok(result) => result,
                Err(_) => return,
            };

        registry.upsert(serial_ref.to_string(), remote_address, session);

        if is_new_or_moved {
            println!(
                "INFO: [UDP {remote_address}] Connected worker {:?} (id={}, sn={serial_ref}, game_id={})",
                device.device_type(),
                device.device_id().unwrap_or("unknown"),
                device.game_id(),
            );
        }

        Self::send_packet(outbound_tx, remote_address, ack_packet.to_bytes());
        Self::send_initial_game_state(
            &mut session,
            assigned_game_id,
            remote_address,
            services,
            outbound_tx,
        );
    }

    fn register_worker(
        packet: &ClientPacket,
        remote_address: SocketAddr,
        services: &Services,
    ) -> Option<Device> {
        let device = match Device::try_from(*packet) {
            Ok(device) => device,
            Err(error) => {
                eprintln!("WARN: [UDP {remote_address}] Invalid worker handshake payload: {error}");
                return None;
            }
        };

        match crate::adapters::net::handler::PacketHandler::register_device(services, device, 0) {
            Ok(registered) => Some(registered),
            Err(error) => {
                eprintln!("WARN: [UDP {remote_address}] Worker device validation failed: {error}");
                None
            }
        }
    }

    fn send_initial_game_state(
        session: &mut ServerSession<net::server_state::Registered>,
        game_id: u16,
        remote_address: SocketAddr,
        services: &Services,
        outbound_tx: &mpsc::Sender<OutboundAck>,
    ) {
        if game_id == 0 {
            return;
        }
        let game = match services.game().by_id(game_id) {
            Some(game) => game,
            None => return,
        };

        let game_packet = session.send_game_update(game.to_bytes());
        Self::send_packet(outbound_tx, remote_address, game_packet.to_bytes());
    }

    fn send_packet(
        outbound_tx: &mpsc::Sender<OutboundAck>,
        target: SocketAddr,
        packet_bytes: [u8; PACKET_SIZE],
    ) {
        let _ = outbound_tx.try_send(OutboundAck {
            packet_bytes,
            target,
        });
    }

    fn generate_session_token() -> u32 {
        (SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u32)
            | 0x8000_0000
    }
}
