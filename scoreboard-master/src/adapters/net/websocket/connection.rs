use core::fmt::Display;
use core::time::Duration;
use std::collections::{HashMap, HashSet};
use std::future::{self, Future};
use std::ops::ControlFlow;

use crate::adapters::net::handler::PacketHandler;
use crate::services::{DeviceGameAssignment, DeviceService, GameFrame, GameService};

use futures::FutureExt;
use scoreboard_basketball::Game;
use scoreboard_config::DeviceConfig;
use scoreboard_net::layout::PACKET_SIZE;
use scoreboard_net::{
    ClientEvent, ClientPacket, Device, PacketReceiver, RawPacket, ServerEvent, ServerHandler,
    ServerPacket,
};
use tokio::sync::{broadcast, mpsc, watch};
use tokio::time::{self, MissedTickBehavior};
use tokio_util::sync::CancellationToken;

const BROWSER_SYNC_INTERVAL: Duration = Duration::from_millis(100);
const SEND_TIMEOUT: Duration = Duration::from_millis(250);
const OUTBOUND_QUEUE_CAPACITY: usize = 4096;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Session {
    pub validated: bool,
    pub browser: bool,
    pub game_id: u64,
}

pub(super) trait FalliblePacketSender: Send {
    type Error: Display;

    fn send_packet(
        &mut self,
        data: &[u8; PACKET_SIZE],
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

pub(super) async fn run<R, W>(
    mut receiver: R,
    mut sender: W,
    game: GameService,
    devices: DeviceService,
    shutdown: CancellationToken,
) where
    R: PacketReceiver,
    W: FalliblePacketSender + 'static,
{
    let mut handler = PacketHandler::new(game.clone(), devices.clone());
    let mut state = SyncState::new();
    let mut disconnect_rx = Some(devices.subscribe_disconnect());
    let mut game_assign_rx = Some(devices.subscribe_game_assign());

    let (outbound_tx, mut outbound_rx) =
        mpsc::channel::<[u8; PACKET_SIZE]>(OUTBOUND_QUEUE_CAPACITY);
    let writer_handle = tokio::spawn(async move {
        while let Some(bytes) = outbound_rx.recv().await {
            if send_packet(&mut sender, &bytes).await.is_break() {
                break;
            }
        }
    });

    loop {
        let step = connection_step(
            &mut receiver,
            &outbound_tx,
            &mut handler,
            &mut state,
            &mut disconnect_rx,
            &mut game_assign_rx,
            &game,
            &devices,
            &shutdown,
        )
        .await;

        if step.is_break() {
            drop(outbound_tx);
            let _ = writer_handle.await;
            log_and_unregister_disconnected(&handler, &devices);
            return;
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn connection_step<R>(
    receiver: &mut R,
    outbound_tx: &mpsc::Sender<[u8; PACKET_SIZE]>,
    handler: &mut PacketHandler,
    state: &mut SyncState,
    disconnect_rx: &mut Option<broadcast::Receiver<String>>,
    game_assign_rx: &mut Option<broadcast::Receiver<DeviceGameAssignment>>,
    game: &GameService,
    devices: &DeviceService,
    shutdown: &CancellationToken,
) -> ControlFlow<()>
where
    R: PacketReceiver,
{
    tokio::select! {
        () = shutdown.cancelled() => {
            return ControlFlow::Break(());
        }

        result = recv_broadcast(disconnect_rx) => {
            match result {
                Ok(target_serial_number) if handler.is_device(&target_serial_number) => {
                    tracing::info!("Closing WebSocket connection for disconnected device sn={target_serial_number}");
                    return ControlFlow::Break(());
                }
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::warn!("WebSocket disconnect_rx lagged {skipped} events");
                }
                Err(broadcast::error::RecvError::Closed) => *disconnect_rx = None,
                _ => {}
            }
        }

        result = recv_broadcast(game_assign_rx) => {
            match result {
                Ok(assignment) if handler.is_device(&assignment.serial_number) => {
                    handler.sync_assignment();
                    sync_frame_subscription(state, handler.session(), game);
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    handler.sync_assignment();
                    sync_frame_subscription(state, handler.session(), game);
                }
                Err(broadcast::error::RecvError::Closed) => *game_assign_rx = None,
                _ => {}
            }
        }

        inbound = receiver.receive_packet() => {
            handle_inbound(inbound, outbound_tx, handler, state, game, devices)?;
        }

        result = recv_frame(&mut state.frame_rx) => {
            match result {
                Ok(frame) => {
                    enqueue_packet(outbound_tx, *frame)?;
                }
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::warn!("WebSocket lagged {skipped} game frames, sending full snapshot");
                    if let Some(current) = game.by_id(state.session.game_id) {
                        state.sequence_id = state.sequence_id.wrapping_add(1);
                        let packet = ServerPacket::new(
                            ServerEvent::GameUpdate,
                            state.sequence_id,
                            current.to_bytes(),
                        );
                        enqueue_packet(outbound_tx, packet.to_bytes())?;
                    }
                }
                Err(broadcast::error::RecvError::Closed) => state.frame_rx = None,
            }
        }

        result = changed(&mut state.game_rx) => {
            match result {
                Ok(()) => state.games_dirty = true,
                Err(_) => state.game_rx = None,
            }
        }

        result = changed(&mut state.devices_rx) => {
            match result {
                Ok(()) => state.devices_dirty = true,
                Err(_) => state.devices_rx = None,
            }
        }

        _ = state.browser_sync_interval.tick(), if state.session.browser => {
            if state.games_dirty {
                sync_games(state, outbound_tx, false)?;
            }
            if state.devices_dirty {
                sync_devices(state, outbound_tx, false)?;
            }
        }
    }

    ControlFlow::Continue(())
}

fn handle_inbound(
    inbound: Result<RawPacket, &'static str>,
    outbound_tx: &mpsc::Sender<[u8; PACKET_SIZE]>,
    handler: &mut PacketHandler,
    state: &mut SyncState,
    game: &GameService,
    devices: &DeviceService,
) -> ControlFlow<()> {
    let Ok(raw) = inbound else {
        return ControlFlow::Break(());
    };

    let Ok(packet) = ClientPacket::from_bytes(raw.bytes) else {
        tracing::warn!("Invalid client packet");
        return ControlFlow::Continue(());
    };

    let was_validated = handler.is_validated();
    let event = packet.event();
    if let Some(response) = handler
        .handle_packet(packet, raw.address)
        .now_or_never()
        .flatten()
    {
        enqueue_packet(outbound_tx, response.to_bytes())?;
    }

    let next_session = handler.session();
    sync_frame_subscription(state, next_session, game);

    if next_session.browser && (!was_validated || event == ClientEvent::HandshakeRequest) {
        state.game_rx.get_or_insert_with(|| game.watch());
        state.devices_rx.get_or_insert_with(|| devices.watch());
        sync_devices(state, outbound_tx, true)?;
        sync_games(state, outbound_tx, true)?;
    }

    ControlFlow::Continue(())
}

fn enqueue_packet(
    outbound_tx: &mpsc::Sender<[u8; PACKET_SIZE]>,
    bytes: [u8; PACKET_SIZE],
) -> ControlFlow<()> {
    match outbound_tx.try_send(bytes) {
        Ok(()) => ControlFlow::Continue(()),
        Err(mpsc::error::TrySendError::Full(_)) => {
            tracing::warn!("WebSocket outbound queue full, dropping slow client");
            ControlFlow::Break(())
        }
        Err(mpsc::error::TrySendError::Closed(_)) => ControlFlow::Break(()),
    }
}

async fn send_packet<W: FalliblePacketSender>(
    sender: &mut W,
    bytes: &[u8; PACKET_SIZE],
) -> ControlFlow<()> {
    match time::timeout(SEND_TIMEOUT, sender.send_packet(bytes)).await {
        Ok(Ok(())) => ControlFlow::Continue(()),
        Ok(Err(error)) => {
            tracing::warn!("WebSocket send failed, closing connection: {error}");
            ControlFlow::Break(())
        }
        Err(_) => {
            tracing::warn!("WebSocket send timed out, dropping slow client");
            ControlFlow::Break(())
        }
    }
}

fn log_and_unregister_disconnected(handler: &PacketHandler, devices: &DeviceService) {
    let Some(device) = handler.validated_device() else {
        tracing::info!("Unvalidated client disconnected");
        return;
    };

    if handler.session().browser {
        tracing::info!(
            "Client disconnected: {:?} (id={}, sn={})",
            device.device_type(),
            device.device_id().unwrap_or("unknown"),
            device.serial_number().unwrap_or("unknown")
        );
    }
    devices.unregister(&device);
}

fn sync_frame_subscription(state: &mut SyncState, next: Session, game: &GameService) {
    let stream_game = next.validated && next.game_id != 0;
    if !stream_game {
        state.frame_rx = None;
    } else if state.frame_rx.is_none() || next.game_id != state.session.game_id {
        state.frame_rx = Some(game.subscribe_frames(next.game_id));
    }

    state.session = next;
}

struct SyncState {
    sequence_id: u16,
    session: Session,
    frame_rx: Option<broadcast::Receiver<GameFrame>>,
    game_rx: Option<watch::Receiver<Vec<Game>>>,
    devices_rx: Option<watch::Receiver<Vec<Device>>>,
    known_devices: HashMap<String, Device>,
    known_games: HashMap<u64, Game>,
    devices_dirty: bool,
    games_dirty: bool,
    browser_sync_interval: time::Interval,
}

impl SyncState {
    fn new() -> Self {
        let mut browser_sync_interval = time::interval(BROWSER_SYNC_INTERVAL);
        browser_sync_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        Self {
            sequence_id: 0,
            session: Session::default(),
            frame_rx: None,
            game_rx: None,
            devices_rx: None,
            known_devices: HashMap::new(),
            known_games: HashMap::new(),
            devices_dirty: false,
            games_dirty: false,
            browser_sync_interval,
        }
    }
}

fn sync_games(
    state: &mut SyncState,
    outbound_tx: &mpsc::Sender<[u8; PACKET_SIZE]>,
    full_resync: bool,
) -> ControlFlow<()> {
    let Some(rx) = &mut state.game_rx else {
        return ControlFlow::Continue(());
    };

    state.games_dirty = false;
    let current_games = rx.borrow_and_update().clone();
    if full_resync {
        state.known_games.clear();
    }
    sync_games_to_browser(
        outbound_tx,
        &mut state.known_games,
        current_games,
        &mut state.sequence_id,
    )
}

fn sync_devices(
    state: &mut SyncState,
    outbound_tx: &mpsc::Sender<[u8; PACKET_SIZE]>,
    full_resync: bool,
) -> ControlFlow<()> {
    let Some(rx) = &mut state.devices_rx else {
        return ControlFlow::Continue(());
    };

    state.devices_dirty = false;
    let current_devices = rx.borrow_and_update().clone();
    if full_resync {
        state.known_devices.clear();
    }
    sync_devices_to_browser(
        outbound_tx,
        &mut state.known_devices,
        current_devices,
        &mut state.sequence_id,
    )
}

async fn recv_broadcast<T: Clone>(
    rx: &mut Option<broadcast::Receiver<T>>,
) -> Result<T, broadcast::error::RecvError> {
    match rx {
        Some(rx) => rx.recv().await,
        None => future::pending().await,
    }
}

async fn changed<T>(rx: &mut Option<watch::Receiver<T>>) -> Result<(), watch::error::RecvError> {
    match rx {
        Some(rx) => rx.changed().await,
        None => future::pending().await,
    }
}

async fn recv_frame(
    rx: &mut Option<broadcast::Receiver<GameFrame>>,
) -> Result<GameFrame, broadcast::error::RecvError> {
    match rx {
        Some(rx) => rx.recv().await,
        None => future::pending().await,
    }
}

fn sync_devices_to_browser(
    outbound_tx: &mpsc::Sender<[u8; PACKET_SIZE]>,
    known_devices: &mut HashMap<String, Device>,
    current_devices: Vec<Device>,
    sequence_id: &mut u16,
) -> ControlFlow<()> {
    let mut current_map = HashMap::with_capacity(current_devices.len());
    let mut current_serials = HashSet::with_capacity(current_devices.len());

    for device in current_devices {
        let Ok(serial) = device.serial_number() else {
            continue;
        };
        let key = serial.to_string();
        current_serials.insert(key.clone());
        let changed = known_devices.get(&key) != Some(&device);
        if changed {
            *sequence_id = sequence_id.wrapping_add(1);
            let packet =
                ServerPacket::new(ServerEvent::DeviceUpdate, *sequence_id, device.to_bytes());
            enqueue_packet(outbound_tx, packet.to_bytes())?;
        }
        current_map.insert(key, device);
    }

    for (serial, previous) in known_devices.iter() {
        if !current_serials.contains(serial) {
            *sequence_id = sequence_id.wrapping_add(1);
            let packet = ServerPacket::new(
                ServerEvent::DeviceDisconnect,
                *sequence_id,
                previous.to_bytes(),
            );
            enqueue_packet(outbound_tx, packet.to_bytes())?;
        }
    }

    *known_devices = current_map;
    ControlFlow::Continue(())
}

fn sync_games_to_browser(
    outbound_tx: &mpsc::Sender<[u8; PACKET_SIZE]>,
    known_games: &mut HashMap<u64, Game>,
    current_games: Vec<Game>,
    sequence_id: &mut u16,
) -> ControlFlow<()> {
    let mut current_map = HashMap::with_capacity(current_games.len());

    for game in current_games {
        let id = game.id();
        if known_games.get(&id) != Some(&game) {
            *sequence_id = sequence_id.wrapping_add(1);
            let packet =
                ServerPacket::new(ServerEvent::GameListUpdate, *sequence_id, game.to_bytes());
            enqueue_packet(outbound_tx, packet.to_bytes())?;
        }
        current_map.insert(id, game);
    }

    *known_games = current_map;
    ControlFlow::Continue(())
}
