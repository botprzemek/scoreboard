use core::time::Duration;
use std::future;
use std::ops::ControlFlow;
use std::sync::Arc;

use crate::adapters::net::handler::PacketHandler;
use crate::services::{DeviceGameAssignment, GameFrame, Observable, Services};

use basketball::Game;
use config::DeviceConfig;
use net::layout::PACKET_SIZE;
use net::{
    ClientEvent, Device, NetError, PacketReceiver, PacketSender, Server, ServerEvent, ServerPacket,
    SplitTransport,
};
use tokio::sync::{broadcast, mpsc, watch};
use tokio::time::{self, MissedTickBehavior};

const OUTBOUND_CAPACITY: usize = 256;

const BROWSER_SYNC_INTERVAL: Duration = Duration::from_millis(100);

const PERIODIC_SYNC_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Session {
    pub validated: bool,
    pub browser: bool,
    pub game_id: u16,
}

enum Outbound {
    Packet([u8; PACKET_SIZE]),

    BrowserResync,
}

struct QueueSender {
    sender: mpsc::Sender<Outbound>,
    overflowed: bool,
}

impl QueueSender {
    fn push(&mut self, message: Outbound) {
        if self.sender.try_send(message).is_err() {
            self.overflowed = true;
        }
    }
}

impl PacketSender for QueueSender {
    async fn send_packet(&mut self, data: &[u8; PACKET_SIZE]) {
        self.push(Outbound::Packet(*data));
    }
}

pub async fn run<R, W>(receiver: R, sender: W, services: Arc<Services>)
where
    R: PacketReceiver,
    W: PacketSender,
{
    let (outbound_tx, outbound_rx) = mpsc::channel(OUTBOUND_CAPACITY);
    let (session_tx, session_rx) = watch::channel(Session::default());

    tokio::select! {
        () = read_loop(receiver, outbound_tx, session_tx, Arc::clone(&services)) => {}
        () = write_loop(sender, outbound_rx, session_rx, services) => {}
    }
}

async fn read_loop<R>(
    receiver: R,
    outbound_tx: mpsc::Sender<Outbound>,
    session_tx: watch::Sender<Session>,
    services: Arc<Services>,
) where
    R: PacketReceiver,
{
    let handler = PacketHandler::new(Arc::clone(&services));
    let queue = QueueSender {
        sender: outbound_tx,
        overflowed: false,
    };
    let mut server = Server::new(SplitTransport::new(queue, receiver), handler);

    let mut disconnect_rx = services.devices().subscribe_disconnect();
    let mut game_assign_rx = services.devices().subscribe_game_assign();

    loop {
        let stopped = read_step(
            &mut server,
            &mut disconnect_rx,
            &mut game_assign_rx,
            &session_tx,
            &services,
        )
        .await
        .is_break();

        if stopped {
            return;
        }
    }
}

async fn read_step<R>(
    server: &mut Server<SplitTransport<QueueSender, R>, PacketHandler>,
    disconnect_rx: &mut broadcast::Receiver<String>,
    game_assign_rx: &mut broadcast::Receiver<DeviceGameAssignment>,
    session_tx: &watch::Sender<Session>,
    services: &Arc<Services>,
) -> ControlFlow<()>
where
    R: PacketReceiver,
{
    let was_validated = server.handler().is_validated();

    tokio::select! {
        result = disconnect_rx.recv() => {
            if let Ok(target_serial_number) = result
                && server.handler().is_device(&target_serial_number)
            {
                println!("INFO: Closing WebSocket connection for disconnected device sn={target_serial_number}");
                return ControlFlow::Break(());
            }
        }

        result = game_assign_rx.recv() => {
            match result {
                Ok(assignment) if server.handler().is_device(&assignment.serial_number) => {
                    server.handler_mut().sync_assignment();
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    server.handler_mut().sync_assignment();
                }
                _ => {}
            }
        }

        result = server.poll() => {
            match result {
                Ok(packet) => {
                    if server.handler().is_web_browser()
                        && (!was_validated || packet.event() == ClientEvent::HandshakeRequest)
                    {
                        session_tx.send_if_modified(|session| {
                            replace(session, server.handler().session())
                        });
                        server
                            .transport_mut()
                            .sender_mut()
                            .push(Outbound::BrowserResync);
                    }
                }
                Err(NetError::InvalidPacket(error)) => {
                    eprintln!("WARN: Invalid client packet: {error}");
                }
                Err(NetError::Disconnected | _) => {
                    if let Some(device) = server.handler().validated_device() {
                        if server.handler().is_web_browser() {
                            println!(
                                "INFO: Client disconnected: {:?} (id={}, sn={})",
                                device.device_type(),
                                device.device_id().unwrap_or("unknown"),
                                device.serial_number().unwrap_or("unknown")
                            );
                        }
                        services.devices().unregister(&device);
                    } else {
                        println!("INFO: Unvalidated client disconnected");
                    }
                    return ControlFlow::Break(());
                }
            }

            if server.transport().sender().overflowed {
                eprintln!("WARN: Outbound queue overflow, dropping slow client");
                if let Some(device) = server.handler().validated_device() {
                    services.devices().unregister(&device);
                }
                return ControlFlow::Break(());
            }
        }
    }

    session_tx.send_if_modified(|session| replace(session, server.handler().session()));
    ControlFlow::Continue(())
}

fn replace(current: &mut Session, next: Session) -> bool {
    if *current == next {
        return false;
    }

    *current = next;

    true
}

struct WriteState {
    sequence_id: u16,
    session: Session,
    frame_rx: Option<watch::Receiver<Option<GameFrame>>>,
    game_rx: Option<watch::Receiver<Vec<Game>>>,
    devices_rx: Option<watch::Receiver<Vec<Device>>>,
    known_devices: Vec<Device>,
    known_games: Vec<Game>,
    devices_dirty: bool,
    games_dirty: bool,
    browser_sync_interval: time::Interval,
    periodic_sync_interval: time::Interval,
}

impl WriteState {
    fn new() -> Self {
        let mut browser_sync_interval = time::interval(BROWSER_SYNC_INTERVAL);
        browser_sync_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        let mut periodic_sync_interval = time::interval(PERIODIC_SYNC_INTERVAL);
        periodic_sync_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        Self {
            sequence_id: 0,
            session: Session::default(),
            frame_rx: None,
            game_rx: None,
            devices_rx: None,
            known_devices: Vec::new(),
            known_games: Vec::new(),
            devices_dirty: false,
            games_dirty: false,
            browser_sync_interval,
            periodic_sync_interval,
        }
    }
}

async fn write_loop<W>(
    mut sender: W,
    mut outbound_rx: mpsc::Receiver<Outbound>,
    mut session_rx: watch::Receiver<Session>,
    services: Arc<Services>,
) where
    W: PacketSender,
{
    let mut state = WriteState::new();

    loop {
        if write_step(
            &mut state,
            &mut sender,
            &mut outbound_rx,
            &mut session_rx,
            &services,
        )
        .await
        .is_break()
        {
            return;
        }
    }
}

async fn write_step<W>(
    state: &mut WriteState,
    sender: &mut W,
    outbound_rx: &mut mpsc::Receiver<Outbound>,
    session_rx: &mut watch::Receiver<Session>,
    services: &Arc<Services>,
) -> std::ops::ControlFlow<()>
where
    W: PacketSender,
{
    tokio::select! {
        message = outbound_rx.recv() => {
            match message {
                Some(Outbound::Packet(bytes)) => sender.send_packet(&bytes).await,
                Some(Outbound::BrowserResync) => {
                    let games = state.game_rx.get_or_insert_with(|| services.game().watch());
                    let devices = state.devices_rx.get_or_insert_with(|| services.devices().watch());

                    state.known_devices.clear();
                    let current_devices = devices.borrow_and_update().clone();
                    sync_devices_to_browser(sender, &mut state.known_devices, current_devices, &mut state.sequence_id).await;

                    state.known_games.clear();
                    let current_games = games.borrow_and_update().clone();
                    sync_games_to_browser(sender, &mut state.known_games, current_games, &mut state.sequence_id).await;

                    state.devices_dirty = false;
                    state.games_dirty = false;
                }
                None => return std::ops::ControlFlow::Break(()),
            }
        }

        result = session_rx.changed() => {
            if result.is_err() {
                return std::ops::ControlFlow::Break(());
            }
            let next = *session_rx.borrow_and_update();
            let stream_game = next.validated && next.game_id != 0;
            if !stream_game {
                state.frame_rx = None;
            } else if state.frame_rx.is_none() || next.game_id != state.session.game_id {
                state.frame_rx = Some(services.game().subscribe_frames(next.game_id));
            }

            state.session = next;
        }

        result = changed(&mut state.frame_rx) => {
            match result {
                Ok(()) => {
                    let frame = state.frame_rx.as_mut().and_then(|rx| rx.borrow_and_update().clone());
                    if let Some(frame) = frame {
                        sender.send_packet(&frame).await;
                    }
                }
                Err(_) => state.frame_rx = None,
            }
        }

        result = changed(&mut state.game_rx) => {
            if result.is_ok() {
                state.games_dirty = true;
            }
        }

        result = changed(&mut state.devices_rx) => {
            if result.is_ok() {
                state.devices_dirty = true;
            }
        }

        _ = state.browser_sync_interval.tick(), if state.session.browser => {
            if state.games_dirty && let Some(rx) = &mut state.game_rx {
                state.games_dirty = false;
                let current_games = rx.borrow_and_update().clone();
                sync_games_to_browser(sender, &mut state.known_games, current_games, &mut state.sequence_id).await;
            }

            if state.devices_dirty && let Some(rx) = &mut state.devices_rx {
                state.devices_dirty = false;
                let current_devices = rx.borrow_and_update().clone();
                sync_devices_to_browser(sender, &mut state.known_devices, current_devices, &mut state.sequence_id).await;
            }
        }

        _ = state.periodic_sync_interval.tick(), if state.session.browser => {
            if let Some(rx) = &mut state.game_rx {
                let current_games = rx.borrow().clone();

                state.known_games.clear();
                sync_games_to_browser(sender, &mut state.known_games, current_games, &mut state.sequence_id).await;
            }
            if let Some(rx) = &mut state.devices_rx {
                let current_devices = rx.borrow().clone();

                state.known_devices.clear();
                sync_devices_to_browser(sender, &mut state.known_devices, current_devices, &mut state.sequence_id).await;
            }
        }
    }

    std::ops::ControlFlow::Continue(())
}

async fn changed<T>(rx: &mut Option<watch::Receiver<T>>) -> Result<(), watch::error::RecvError> {
    match rx {
        Some(rx) => rx.changed().await,
        None => future::pending().await,
    }
}

async fn sync_devices_to_browser<W>(
    sender: &mut W,
    known_devices: &mut Vec<Device>,
    current_devices: Vec<Device>,
    sequence_id: &mut u16,
) where
    W: PacketSender,
{
    let disconnects = known_devices
        .iter()
        .filter(|previous| {
            !current_devices
                .iter()
                .any(|device| device.serial_number() == previous.serial_number())
        })
        .map(|previous| (ServerEvent::DeviceDisconnect, previous.to_bytes()));

    let updates = current_devices
        .iter()
        .filter(|device| !known_devices.contains(device))
        .map(|device| (ServerEvent::DeviceUpdate, device.to_bytes()));

    let packets: Vec<ServerPacket> = disconnects
        .chain(updates)
        .map(|(event, payload)| {
            *sequence_id = sequence_id.wrapping_add(1);
            ServerPacket::new(event, *sequence_id, payload)
        })
        .collect();

    for packet in packets {
        sender.send_packet(&packet.to_bytes()).await;
    }

    *known_devices = current_devices;
}

async fn sync_games_to_browser<W>(
    sender: &mut W,
    known_games: &mut Vec<Game>,
    current_games: Vec<Game>,
    sequence_id: &mut u16,
) where
    W: PacketSender,
{
    let packets: Vec<ServerPacket> = current_games
        .iter()
        .filter(|game| !known_games.contains(game))
        .map(|game| {
            *sequence_id = sequence_id.wrapping_add(1);
            ServerPacket::new(ServerEvent::GameListUpdate, *sequence_id, game.to_bytes())
        })
        .collect();

    for packet in packets {
        sender.send_packet(&packet.to_bytes()).await;
    }

    *known_games = current_games;
}
