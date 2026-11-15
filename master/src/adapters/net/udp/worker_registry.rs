use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::services::{Observable, Services};

use basketball::{Discipline, Game};
use net::ServerSession;
use net::server_state;
use tokio::net::UdpSocket;
use tokio::sync::watch;
use tokio::time::{self, MissedTickBehavior};

const SYNC_HEARTBEAT_DURATION: Duration = Duration::from_nanos(1_000_000_000 / 60);

#[derive(Clone)]
pub(crate) struct ActiveWorker {
    pub(crate) address: SocketAddr,
    pub(crate) serial_number: String,
    pub(crate) session: ServerSession<server_state::Registered>,
    pub(crate) last_seen: Instant,
}

impl ActiveWorker {
    fn matches_session(&self, remote_address: SocketAddr, packet_token: u32) -> bool {
        self.address == remote_address && self.session.session_token() == packet_token
    }

    fn token_and_moved(&self, remote_address: SocketAddr) -> (u32, bool) {
        (self.session.session_token(), self.address != remote_address)
    }
}

#[derive(Clone)]
pub(crate) struct WorkerRegistry {
    tx: watch::Sender<HashMap<String, ActiveWorker>>,
}

impl WorkerRegistry {
    pub(crate) fn new() -> Self {
        let (tx, _) = watch::channel(HashMap::new());
        Self { tx }
    }

    pub(crate) fn touch_heartbeat(
        &self,
        remote_address: SocketAddr,
        packet_token: u32,
    ) -> Option<ActiveWorker> {
        let now = Instant::now();
        let mut matched = None;
        self.tx.send_modify(|workers| {
            matched = Self::refresh_worker_heartbeat(workers, remote_address, packet_token, now);
        });
        matched
    }

    pub(crate) fn existing_token(
        &self,
        serial_number: &str,
        remote_address: SocketAddr,
    ) -> Option<(u32, bool)> {
        let workers = self.tx.borrow();
        workers
            .get(serial_number)
            .map(|existing| existing.token_and_moved(remote_address))
    }

    pub(crate) fn upsert(
        &self,
        serial_number: String,
        address: SocketAddr,
        session: ServerSession<server_state::Registered>,
    ) {
        let worker = ActiveWorker {
            address,
            serial_number: serial_number.clone(),
            session,
            last_seen: Instant::now(),
        };
        self.tx
            .send_modify(|workers| Self::insert_worker(workers, serial_number, worker));
    }

    fn insert_worker(
        workers: &mut HashMap<String, ActiveWorker>,
        serial_number: String,
        worker: ActiveWorker,
    ) {
        workers.insert(serial_number, worker);
    }

    pub(crate) fn remove(&self, serial_number: &str) -> bool {
        let mut removed = false;
        self.tx.send_modify(|workers| {
            removed = workers.remove(serial_number).is_some();
        });
        removed
    }

    pub(crate) fn spawn_watchdog(&self, services: Arc<Services>, worker_timeout: Duration) {
        tokio::spawn(Self::run_watchdog(
            self.tx.clone(),
            services,
            worker_timeout,
        ));
    }

    async fn run_watchdog(
        tx: watch::Sender<HashMap<String, ActiveWorker>>,
        services: Arc<Services>,
        worker_timeout: Duration,
    ) {
        let mut interval = time::interval(Duration::from_secs(1));
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            interval.tick().await;
            Self::prune_timed_out_workers(&tx, &services, worker_timeout);
        }
    }

    fn prune_timed_out_workers(
        tx: &watch::Sender<HashMap<String, ActiveWorker>>,
        services: &Services,
        worker_timeout: Duration,
    ) {
        let now = Instant::now();
        let mut timed_out_serials = Vec::new();

        tx.send_modify(|workers| {
            Self::retain_active_workers(workers, now, worker_timeout, &mut timed_out_serials);
        });

        for sn in timed_out_serials {
            println!(
                "INFO: [UDP] Worker sn={sn} timed out ({}s without heartbeat) -> offline",
                worker_timeout.as_secs()
            );
            services.devices().disconnect_by_serial(&sn);
        }
    }

    fn retain_active_workers(
        workers: &mut HashMap<String, ActiveWorker>,
        now: Instant,
        worker_timeout: Duration,
        timed_out_serials: &mut Vec<String>,
    ) {
        workers.retain(|sn, worker| {
            let expired = now.duration_since(worker.last_seen) > worker_timeout;
            if expired {
                timed_out_serials.push(sn.clone());
            }
            !expired
        });
    }

    pub(crate) fn spawn_tick_sync(&self, socket: Arc<UdpSocket>, services: Arc<Services>) {
        tokio::spawn(Self::run_tick_sync(self.tx.subscribe(), socket, services));
    }

    fn refresh_worker_heartbeat(
        workers: &mut HashMap<String, ActiveWorker>,
        remote_address: SocketAddr,
        packet_token: u32,
        now: Instant,
    ) -> Option<ActiveWorker> {
        let worker = workers
            .values_mut()
            .find(|worker| worker.matches_session(remote_address, packet_token))?;
        worker.last_seen = now;
        Some(worker.clone())
    }

    async fn run_tick_sync(
        mut worker_rx: watch::Receiver<HashMap<String, ActiveWorker>>,
        socket: Arc<UdpSocket>,
        services: Arc<Services>,
    ) {
        let mut interval = time::interval(SYNC_HEARTBEAT_DURATION);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut assignment_rx = services.devices().subscribe_game_assign();
        let mut broadcast_rx = services.game().subscribe();
        let mut active_workers: HashMap<String, ActiveWorker> = HashMap::new();
        let mut last_sent_games: HashMap<String, Game> = HashMap::new();

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    Self::sync_tick_workers(&socket, &services, &mut active_workers, &mut last_sent_games).await;
                }

                Ok(()) = worker_rx.changed() => {
                    let latest = worker_rx.borrow_and_update().clone();
                    Self::merge_active_workers(&mut active_workers, latest, &mut last_sent_games);
                }

                Ok(assignment) = assignment_rx.recv() => {
                    Self::sync_assignment(&socket, &services, &mut active_workers, &assignment, &mut last_sent_games).await;
                }

                Ok(games) = broadcast_rx.recv() => {
                    Self::sync_broadcast_games(&socket, &services, &mut active_workers, &games, &mut last_sent_games).await;
                }
            }
        }
    }

    fn merge_active_workers(
        active_workers: &mut HashMap<String, ActiveWorker>,
        latest: HashMap<String, ActiveWorker>,
        last_sent_games: &mut HashMap<String, Game>,
    ) {
        active_workers.retain(|serial, _| latest.contains_key(serial));
        last_sent_games.retain(|serial, _| latest.contains_key(serial));

        for (serial, incoming) in latest {
            match active_workers.get_mut(&serial) {
                Some(existing)
                    if existing.address == incoming.address
                        && existing.session.session_token()
                            == incoming.session.session_token() =>
                {
                    existing.last_seen = incoming.last_seen;
                }
                _ => {
                    last_sent_games.remove(&serial);
                    active_workers.insert(serial, incoming);
                }
            }
        }
    }

    async fn sync_tick_workers(
        socket: &UdpSocket,
        services: &Services,
        active_workers: &mut HashMap<String, ActiveWorker>,
        last_sent_games: &mut HashMap<String, Game>,
    ) {
        let updates: Vec<(&mut ActiveWorker, Game)> = active_workers
            .values_mut()
            .filter_map(|worker| {
                let game_id = services.devices().game_id_by_serial(&worker.serial_number)?;
                if game_id == 0 {
                    return None;
                }
                let game = services.game().by_id(game_id)?;
                Some((worker, game))
            })
            .collect();

        for (worker, game) in updates {
            Self::send_game_if_changed(socket, worker, game, last_sent_games).await;
        }
    }

    async fn sync_assignment(
        socket: &UdpSocket,
        services: &Services,
        active_workers: &mut HashMap<String, ActiveWorker>,
        assignment: &crate::services::DeviceGameAssignment,
        last_sent_games: &mut HashMap<String, Game>,
    ) {
        let worker = match active_workers.get_mut(&assignment.serial_number) {
            Some(worker) => worker,
            None => return,
        };

        let game = if assignment.game_id != 0 {
            services.game().by_id(assignment.game_id)
        } else {
            None
        }
        .unwrap_or_else(|| Game::new_with_id(0, Discipline::Fiba5v5));

        last_sent_games.insert(worker.serial_number.clone(), game);
        let packet = worker.session.send_game_update(game.to_bytes());
        let _ = socket.send_to(&packet.to_bytes(), worker.address).await;
    }

    async fn sync_broadcast_games(
        socket: &UdpSocket,
        services: &Services,
        active_workers: &mut HashMap<String, ActiveWorker>,
        games: &[Game],
        last_sent_games: &mut HashMap<String, Game>,
    ) {
        let updates: Vec<(&mut ActiveWorker, Game)> = active_workers
            .values_mut()
            .filter_map(|worker| {
                let game_id = services.devices().game_id_by_serial(&worker.serial_number)?;
                if game_id == 0 {
                    return None;
                }
                let game = games.iter().find(|entry| entry.id() == game_id).copied()?;
                Some((worker, game))
            })
            .collect();

        for (worker, game) in updates {
            Self::send_game_if_changed(socket, worker, game, last_sent_games).await;
        }
    }

    async fn send_game_if_changed(
        socket: &UdpSocket,
        worker: &mut ActiveWorker,
        game: Game,
        last_sent_games: &mut HashMap<String, Game>,
    ) {
        if last_sent_games.get(&worker.serial_number) == Some(&game) {
            return;
        }

        last_sent_games.insert(worker.serial_number.clone(), game);
        let packet = worker.session.send_game_update(game.to_bytes());
        let _ = socket.send_to(&packet.to_bytes(), worker.address).await;
    }
}
