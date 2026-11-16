use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use crate::services::{DeviceGameAssignment, DeviceService, GameService};

use basketball::{Discipline, Game};
use net::layout::PACKET_SIZE;
use net::{Registered, ServerSession};

pub(super) struct ActiveWorker {
    pub(super) address: SocketAddr,
    pub(super) serial_number: String,
    pub(super) game_id: u64,
    pub(super) session: ServerSession<Registered>,
    pub(super) last_seen: Instant,
}

impl ActiveWorker {
    fn token_and_moved(&self, remote_address: SocketAddr) -> (u32, bool) {
        (self.session.session_token(), self.address != remote_address)
    }
}

pub(super) struct WorkerRegistry {
    workers: HashMap<String, ActiveWorker>,
    addr_token_index: HashMap<(SocketAddr, u32), String>,
    workers_by_game: HashMap<u64, HashSet<String>>,
    last_sent_games: HashMap<String, Game>,
}

impl WorkerRegistry {
    pub(super) fn new() -> Self {
        Self {
            workers: HashMap::new(),
            addr_token_index: HashMap::new(),
            workers_by_game: HashMap::new(),
            last_sent_games: HashMap::new(),
        }
    }

    pub(super) fn touch_heartbeat(
        &mut self,
        remote_address: SocketAddr,
        packet_token: u32,
    ) -> Option<&mut ActiveWorker> {
        let serial = self.addr_token_index.get(&(remote_address, packet_token))?;
        let worker = self.workers.get_mut(serial)?;
        worker.last_seen = Instant::now();
        Some(worker)
    }

    pub(super) fn active_address(&self, serial_number: &str) -> Option<SocketAddr> {
        self.workers.get(serial_number).map(|w| w.address)
    }

    pub(super) fn existing_token(
        &self,
        serial_number: &str,
        remote_address: SocketAddr,
    ) -> Option<(u32, bool)> {
        self.workers
            .get(serial_number)
            .map(|existing| existing.token_and_moved(remote_address))
    }

    pub(super) fn upsert(
        &mut self,
        serial_number: String,
        address: SocketAddr,
        game_id: u64,
        session: ServerSession<Registered>,
    ) {
        if let Some((old_addr, old_token, old_game_id)) = self
            .workers
            .get(&serial_number)
            .map(|e| (e.address, e.session.session_token(), e.game_id))
        {
            self.addr_token_index.remove(&(old_addr, old_token));
            if old_game_id != game_id {
                self.remove_from_game_index(old_game_id, &serial_number);
            }
            if old_addr != address || old_token != session.session_token() {
                self.last_sent_games.remove(&serial_number);
            }
        }

        self.addr_token_index.insert(
            (address, session.session_token()),
            serial_number.clone(),
        );
        if game_id != 0 {
            self.workers_by_game
                .entry(game_id)
                .or_default()
                .insert(serial_number.clone());
        }

        self.workers.insert(
            serial_number.clone(),
            ActiveWorker {
                address,
                serial_number,
                game_id,
                session,
                last_seen: Instant::now(),
            },
        );
    }

    pub(super) fn remove(&mut self, serial_number: &str) -> bool {
        self.last_sent_games.remove(serial_number);
        let Some(removed) = self.workers.remove(serial_number) else {
            return false;
        };
        self.addr_token_index
            .remove(&(removed.address, removed.session.session_token()));
        self.remove_from_game_index(removed.game_id, serial_number);
        true
    }

    pub(super) fn prune_timed_out(&mut self, worker_timeout: Duration, devices: &DeviceService) {
        let now = Instant::now();
        let timed_out_serials: Vec<String> = self
            .workers
            .iter()
            .filter(|&(_, worker)| now.duration_since(worker.last_seen) > worker_timeout)
            .map(|(sn, _)| sn.clone())
            .collect();

        for sn in timed_out_serials {
            self.remove(&sn);
            tracing::info!(
                "[UDP] Worker sn={sn} timed out ({}s without heartbeat) -> offline",
                worker_timeout.as_secs()
            );
            devices.disconnect_by_serial(&sn);
        }
    }

    pub(super) fn collect_tick_updates(
        &mut self,
        game_service: &GameService,
        mut try_send: impl FnMut(SocketAddr, [u8; PACKET_SIZE]) -> bool,
    ) {
        let game_ids: Vec<u64> = self.workers_by_game.keys().copied().collect();
        for game_id in game_ids {
            let Some(game) = game_service.by_id(game_id) else {
                continue;
            };
            self.send_game_to_subscribers(game_id, game, &mut try_send);
        }
    }

    pub(super) fn collect_broadcast_updates(
        &mut self,
        games: &[Game],
        mut try_send: impl FnMut(SocketAddr, [u8; PACKET_SIZE]) -> bool,
    ) {
        for &game in games {
            let game_id = game.id();
            if self.workers_by_game.contains_key(&game_id) {
                self.send_game_to_subscribers(game_id, game, &mut try_send);
            }
        }
    }

    pub(super) fn collect_assignment_update(
        &mut self,
        game_service: &GameService,
        assignment: &DeviceGameAssignment,
        mut try_send: impl FnMut(SocketAddr, [u8; PACKET_SIZE]) -> bool,
    ) {
        let Some(old_game_id) = self.workers.get(&assignment.serial_number).map(|w| w.game_id)
        else {
            return;
        };

        if old_game_id != assignment.game_id {
            self.remove_from_game_index(old_game_id, &assignment.serial_number);
            if assignment.game_id != 0 {
                self.workers_by_game
                    .entry(assignment.game_id)
                    .or_default()
                    .insert(assignment.serial_number.clone());
            }
        }

        let Some(worker) = self.workers.get_mut(&assignment.serial_number) else {
            return;
        };
        worker.game_id = assignment.game_id;

        let game = (assignment.game_id != 0)
            .then(|| game_service.by_id(assignment.game_id))
            .flatten()
            .unwrap_or_else(|| Game::new_with_id(0, Discipline::Fiba5v5));

        let packet = worker.session.send_game_update(game.to_bytes());
        if try_send(worker.address, packet.to_bytes()) {
            self.last_sent_games
                .insert(worker.serial_number.clone(), game);
        }
    }

    fn send_game_to_subscribers(
        &mut self,
        game_id: u64,
        game: Game,
        try_send: &mut impl FnMut(SocketAddr, [u8; PACKET_SIZE]) -> bool,
    ) {
        let Some(serials) = self.workers_by_game.get(&game_id) else {
            return;
        };
        for serial in serials {
            if self.last_sent_games.get(serial) == Some(&game) {
                continue;
            }
            let Some(worker) = self.workers.get_mut(serial) else {
                continue;
            };
            let packet = worker.session.send_game_update(game.to_bytes());
            if try_send(worker.address, packet.to_bytes()) {
                self.last_sent_games.insert(serial.clone(), game);
            }
        }
    }

    fn remove_from_game_index(&mut self, game_id: u64, serial_number: &str) {
        if game_id == 0 {
            return;
        }
        if let Some(set) = self.workers_by_game.get_mut(&game_id) {
            set.remove(serial_number);
            if set.is_empty() {
                self.workers_by_game.remove(&game_id);
            }
        }
    }
}
