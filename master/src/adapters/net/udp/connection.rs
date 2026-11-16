use std::future;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use crate::services::{DeviceService, GameService};

use config::DeviceConfig;
use net::layout::PACKET_SIZE;
use net::{ClientEvent, ClientPacket, Device, Registered, ServerSession};
use tokio::net::UdpSocket;
use tokio::sync::{broadcast, mpsc};
use tokio::time::{self, MissedTickBehavior};
use tokio_util::sync::CancellationToken;

use super::worker_registry::WorkerRegistry;

pub(super) const INBOUND_QUEUE_CAPACITY: usize = 16384;
pub(super) const OUTBOUND_QUEUE_CAPACITY: usize = 16384;
const SYNC_HEARTBEAT_DURATION: Duration = Duration::from_nanos(1_000_000_000 / 60);

pub(super) struct InboundPacket {
    packet: ClientPacket,
    remote_address: SocketAddr,
}

pub(super) struct OutboundPacket {
    bytes: [u8; PACKET_SIZE],
    target: SocketAddr,
}

pub(super) async fn handle_incoming(
    socket: Arc<UdpSocket>,
    inbound_tx: mpsc::Sender<InboundPacket>,
    shutdown: CancellationToken,
) {
    let mut receive_buffer = [0u8; PACKET_SIZE + 1];

    loop {
        let received = tokio::select! {
            () = shutdown.cancelled() => return,
            result = socket.recv_from(&mut receive_buffer) => result,
        };

        let (len, remote_address) = match received {
            Ok(pair) => pair,
            Err(error) => {
                tracing::warn!("[UDP] recv_from failed: {error}");
                time::sleep(Duration::from_millis(50)).await;
                continue;
            }
        };
        let Ok(packet_array) = receive_buffer[..len].try_into() else {
            continue;
        };
        let Ok(packet) = ClientPacket::from_bytes(packet_array) else {
            continue;
        };
        if !matches!(
            packet.event(),
            ClientEvent::HandshakeHeartbeat | ClientEvent::HandshakeRequest
        ) {
            continue;
        }

        let inbound = InboundPacket {
            packet,
            remote_address,
        };
        if let Err(error) = inbound_tx.try_send(inbound) {
            match error {
                mpsc::error::TrySendError::Full(_) => {
                    tracing::warn!("[UDP {remote_address}] Inbound queue full, dropping packet");
                }
                mpsc::error::TrySendError::Closed(_) => return,
            }
        }
    }
}

pub(super) async fn handle_outbound_queue(
    socket: Arc<UdpSocket>,
    mut outbound_rx: mpsc::Receiver<OutboundPacket>,
) {
    while let Some(outbound) = outbound_rx.recv().await {
        if let Err(error) = socket.send_to(&outbound.bytes, outbound.target).await {
            tracing::warn!("[UDP {}] Failed to send outbound packet: {error}", outbound.target);
        }
    }
}

pub(super) async fn run_actor(
    socket: Arc<UdpSocket>,
    worker_timeout: Duration,
    game: GameService,
    devices: DeviceService,
    mut inbound_rx: mpsc::Receiver<InboundPacket>,
    outbound_tx: mpsc::Sender<OutboundPacket>,
    shutdown: CancellationToken,
) {
    let mut registry = WorkerRegistry::new();
    let mut sequence_id: u16 = 0;

    let mut watchdog_interval = time::interval(Duration::from_secs(1));
    watchdog_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

    let mut sync_interval = time::interval(SYNC_HEARTBEAT_DURATION);
    sync_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

    let mut disconnect_rx = Some(devices.subscribe_disconnect());
    let mut assignment_rx = Some(devices.subscribe_game_assign());
    let mut broadcast_rx = Some(game.subscribe());

    loop {
        tokio::select! {
            () = shutdown.cancelled() => return,

            inbound = inbound_rx.recv() => {
                let Some(inbound) = inbound else {
                    return;
                };
                handle_packet(
                    inbound.packet,
                    inbound.remote_address,
                    &game,
                    &devices,
                    &mut registry,
                    &socket,
                    &outbound_tx,
                    &mut sequence_id,
                );
            }

            _ = watchdog_interval.tick() => {
                registry.prune_timed_out(worker_timeout, &devices);
            }

            _ = sync_interval.tick() => {
                registry.collect_tick_updates(&game, |target, bytes| {
                    send_packet(&socket, &outbound_tx, target, bytes)
                });
            }

            result = recv_broadcast(&mut disconnect_rx) => {
                match result {
                    Ok(target_serial_number) => {
                        if registry.remove(&target_serial_number) {
                            tracing::info!("[UDP] Forcefully disconnected worker sn={target_serial_number}");
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!("[UDP] disconnect_rx lagged {skipped} events");
                    }
                    Err(broadcast::error::RecvError::Closed) => disconnect_rx = None,
                }
            }

            result = recv_broadcast(&mut assignment_rx) => {
                match result {
                    Ok(assignment) => {
                        registry.collect_assignment_update(&game, &assignment, |target, bytes| {
                            send_packet(&socket, &outbound_tx, target, bytes)
                        });
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!("[UDP] assignment_rx lagged {skipped} events, resyncing");
                        registry.collect_tick_updates(&game, |target, bytes| {
                            send_packet(&socket, &outbound_tx, target, bytes)
                        });
                    }
                    Err(broadcast::error::RecvError::Closed) => assignment_rx = None,
                }
            }

            result = recv_broadcast(&mut broadcast_rx) => {
                match result {
                    Ok(games) => {
                        registry.collect_broadcast_updates(&games, |target, bytes| {
                            send_packet(&socket, &outbound_tx, target, bytes)
                        });
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!("[UDP] broadcast_rx lagged {skipped} events, resyncing");
                        registry.collect_tick_updates(&game, |target, bytes| {
                            send_packet(&socket, &outbound_tx, target, bytes)
                        });
                    }
                    Err(broadcast::error::RecvError::Closed) => broadcast_rx = None,
                }
            }
        }
    }
}

async fn recv_broadcast<T: Clone>(
    rx: &mut Option<broadcast::Receiver<T>>,
) -> Result<T, broadcast::error::RecvError> {
    match rx {
        Some(rx) => rx.recv().await,
        None => future::pending().await,
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_packet(
    packet: ClientPacket,
    remote_address: SocketAddr,
    game: &GameService,
    devices: &DeviceService,
    registry: &mut WorkerRegistry,
    socket: &UdpSocket,
    outbound_tx: &mpsc::Sender<OutboundPacket>,
    sequence_id: &mut u16,
) {
    match packet.event() {
        ClientEvent::HandshakeHeartbeat => {
            handle_heartbeat(
                &packet,
                remote_address,
                registry,
                socket,
                outbound_tx,
                sequence_id,
            );
        }
        ClientEvent::HandshakeRequest => {
            handle_handshake(
                packet,
                remote_address,
                game,
                devices,
                registry,
                socket,
                outbound_tx,
            );
        }
        _ => {}
    }
}

fn handle_heartbeat(
    packet: &ClientPacket,
    remote_address: SocketAddr,
    registry: &mut WorkerRegistry,
    socket: &UdpSocket,
    outbound_tx: &mpsc::Sender<OutboundPacket>,
    sequence_id: &mut u16,
) {
    let token = packet.session_token();
    if token == 0 {
        return;
    }

    let Some(worker) = registry.touch_heartbeat(remote_address, token) else {
        return;
    };
    if worker.session.receive_heartbeat(packet).is_err() {
        return;
    }

    *sequence_id = sequence_id.wrapping_add(1);
    let ack_packet = worker.session.send_heartbeat_ack(*sequence_id);
    send_packet(socket, outbound_tx, remote_address, ack_packet.to_bytes());
}

fn handle_handshake(
    packet: ClientPacket,
    remote_address: SocketAddr,
    game: &GameService,
    devices: &DeviceService,
    registry: &mut WorkerRegistry,
    socket: &UdpSocket,
    outbound_tx: &mpsc::Sender<OutboundPacket>,
) {
    let Some(device) = register_worker(&packet, remote_address, game, devices) else {
        return;
    };

    let assigned_game_id = device.game_id();
    let Ok(serial_ref) = device.serial_number() else {
        return;
    };

    if let Some(existing_addr) = registry.active_address(serial_ref)
        && existing_addr != remote_address
    {
        tracing::warn!(
            "[UDP {remote_address}] Rejected handshake for active worker sn={serial_ref} (already bound to {existing_addr})"
        );
        return;
    }

    let (session_token, is_new_or_moved) = registry
        .existing_token(serial_ref, remote_address)
        .unwrap_or_else(|| (generate_session_token(), true));

    let Ok((mut session, ack_packet, _)) =
        ServerSession::new().accept_handshake(&packet, session_token, assigned_game_id)
    else {
        return;
    };

    send_packet(socket, outbound_tx, remote_address, ack_packet.to_bytes());
    send_initial_game_state(
        &mut session,
        assigned_game_id,
        remote_address,
        game,
        socket,
        outbound_tx,
    );

    registry.upsert(
        serial_ref.to_string(),
        remote_address,
        assigned_game_id,
        session,
    );

    if is_new_or_moved {
        tracing::info!(
            "[UDP {remote_address}] Connected worker {:?} (id={}, sn={serial_ref}, game_id={})",
            device.device_type(),
            device.device_id().unwrap_or("unknown"),
            device.game_id(),
        );
    }
}

fn register_worker(
    packet: &ClientPacket,
    remote_address: SocketAddr,
    game: &GameService,
    devices: &DeviceService,
) -> Option<Device> {
    let device = match Device::try_from(*packet) {
        Ok(device) => device,
        Err(error) => {
            tracing::warn!("[UDP {remote_address}] Invalid worker handshake payload: {error}");
            return None;
        }
    };

    match crate::adapters::net::handler::PacketHandler::register_device(devices, game, device, 0) {
        Ok(registered) => Some(registered),
        Err(error) => {
            tracing::warn!("[UDP {remote_address}] Worker device validation failed: {error}");
            None
        }
    }
}

fn send_initial_game_state(
    session: &mut ServerSession<Registered>,
    game_id: u64,
    remote_address: SocketAddr,
    game_service: &GameService,
    socket: &UdpSocket,
    outbound_tx: &mpsc::Sender<OutboundPacket>,
) {
    if game_id == 0 {
        return;
    }
    let Some(game) = game_service.by_id(game_id) else {
        return;
    };

    let game_packet = session.send_game_update(game.to_bytes());
    send_packet(socket, outbound_tx, remote_address, game_packet.to_bytes());
}

fn send_packet(
    socket: &UdpSocket,
    outbound_tx: &mpsc::Sender<OutboundPacket>,
    target: SocketAddr,
    bytes: [u8; PACKET_SIZE],
) -> bool {
    match socket.try_send_to(&bytes, target) {
        Ok(_) => true,
        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
            if let Err(error) = outbound_tx.try_send(OutboundPacket { bytes, target }) {
                tracing::warn!("[UDP {target}] Dropping outbound packet: {error}");
                return false;
            }
            true
        }
        Err(error) => {
            tracing::warn!("[UDP {target}] Failed to send UDP packet: {error}");
            false
        }
    }
}

fn generate_session_token() -> u32 {
    rand::random::<u32>() | 0x8000_0000
}
