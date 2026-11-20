use super::sync::{self, GameCommand};

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use crate::adapters::repositories::{GameRepository, Repository};

use scoreboard_basketball::{Discipline, Game, Score, Team};
use scoreboard_net::layout::PACKET_SIZE;
use tokio::sync::{broadcast, mpsc, watch};
use tokio::task::{JoinHandle, JoinSet};
use tokio_util::sync::CancellationToken;

pub type GameFrame = Arc<[u8; PACKET_SIZE]>;

#[derive(Clone)]
pub(super) struct GameHandle {
    pub(super) cmd_tx: mpsc::Sender<GameCommand>,
    pub(super) latest_rx: watch::Receiver<Game>,
    pub(super) frame_tx: broadcast::Sender<GameFrame>,
}

#[derive(Clone)]
pub struct GameService {
    pub(super) actors: Arc<RwLock<HashMap<u64, GameHandle>>>,
    pub(super) state: watch::Sender<Vec<Game>>,
    pub(super) broadcast: broadcast::Sender<Vec<Game>>,
    dirty_ids: Arc<Mutex<HashSet<u64>>>,
    events_tx: mpsc::Sender<Game>,
    game_tasks: Arc<Mutex<JoinSet<()>>>,
    shutdown: CancellationToken,
}

impl GameService {
    pub fn new(
        game_repository: GameRepository,
        db_flush_interval: Duration,
        actors_shutdown: CancellationToken,
        db_shutdown: CancellationToken,
    ) -> Result<(Self, Vec<JoinHandle<()>>), String> {
        let mut initial_games = game_repository.select()?;
        if initial_games.is_empty() {
            let default_game = Game::new(1, Discipline::Fiba5v5);
            initial_games.push(default_game);
            game_repository.upsert_many(&initial_games)?;
        }

        let dirty_ids = Arc::new(Mutex::new(HashSet::new()));
        let (events_tx, events_rx) = mpsc::channel(sync::EVENT_QUEUE_CAPACITY);
        let (state, _state_rx) = watch::channel(initial_games.clone());
        let (broadcast, _broadcast_rx) = broadcast::channel(64);

        let actors = Arc::new(RwLock::new(HashMap::new()));
        let mut game_tasks = JoinSet::new();

        {
            let mut actors_guard = actors
                .write()
                .expect("game actors lock should not be poisoned");
            for &game in &initial_games {
                let handle = sync::spawn_game_actor(
                    game,
                    Arc::clone(&dirty_ids),
                    events_tx.clone(),
                    actors_shutdown.clone(),
                    &mut game_tasks,
                );
                actors_guard.insert(game.id(), handle);
            }
        }

        let handles = vec![
            sync::spawn_db_flusher(
                game_repository,
                Arc::clone(&actors),
                Arc::clone(&dirty_ids),
                db_flush_interval,
                db_shutdown,
            ),
            sync::spawn_snapshot_aggregator(
                initial_games,
                events_rx,
                state.clone(),
                broadcast.clone(),
                actors_shutdown.clone(),
            ),
        ];

        let service = Self {
            actors,
            state,
            broadcast,
            dirty_ids,
            events_tx,
            game_tasks: Arc::new(Mutex::new(game_tasks)),
            shutdown: actors_shutdown,
        };

        Ok((service, handles))
    }

    pub async fn shutdown(&self) {
        let mut tasks = {
            let mut guard = self
                .game_tasks
                .lock()
                .expect("game tasks lock should not be poisoned");
            std::mem::take(&mut *guard)
        };
        while let Some(result) = tasks.join_next().await {
            if let Err(error) = result {
                tracing::error!("Game actor task failed during shutdown: {error}");
            }
        }
    }

    #[must_use]
    pub fn by_id(&self, id: u64) -> Option<Game> {
        self.actors
            .read()
            .expect("game actors lock should not be poisoned")
            .get(&id)
            .map(|handle| *handle.latest_rx.borrow())
    }

    #[must_use]
    pub fn default_game(&self) -> Game {
        self.by_id(1)
            .unwrap_or_else(|| Game::new(1, Discipline::Fiba5v5))
    }

    pub fn create_game(&self, discipline: Discipline) -> Result<Game, &'static str> {
        let created = {
            let mut actors = self
                .actors
                .write()
                .expect("game actors lock should not be poisoned");
            let next_id = Self::next_game_id(&actors)?;
            let created = Game::new(next_id, discipline);
            let mut tasks = self
                .game_tasks
                .lock()
                .expect("game tasks lock should not be poisoned");
            let handle = sync::spawn_game_actor(
                created,
                Arc::clone(&self.dirty_ids),
                self.events_tx.clone(),
                self.shutdown.clone(),
                &mut tasks,
            );
            actors.insert(next_id, handle);
            created
        };

        self.dirty_ids
            .lock()
            .expect("dirty game IDs lock should not be poisoned")
            .insert(created.id());
        if let Err(error) = self.events_tx.try_send(created) {
            tracing::warn!(
                "Failed to publish created game id={} to snapshot aggregator: {error}",
                created.id()
            );
        }

        Ok(created)
    }

    pub fn delete_game(&self, game_id: u64) -> Result<Game, &'static str> {
        {
            let mut actors = self
                .actors
                .write()
                .expect("game actors lock should not be poisoned");
            if actors.len() <= 1 {
                return Err("Cannot delete the last remaining game");
            }
            if actors.remove(&game_id).is_none() {
                return Err("Game not found");
            }
        }

        self.dirty_ids
            .lock()
            .expect("dirty game IDs lock should not be poisoned")
            .remove(&game_id);

        self.state.send_modify(|games| {
            games.retain(|g| g.id() != game_id);
        });

        Ok(self
            .state
            .borrow()
            .first()
            .copied()
            .unwrap_or_else(|| Game::new(1, Discipline::Fiba5v5)))
    }

    fn next_game_id(actors: &HashMap<u64, GameHandle>) -> Result<u64, &'static str> {
        actors
            .keys()
            .copied()
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .or_else(|| (1..=u64::MAX).find(|id| !actors.contains_key(id)))
            .ok_or("game ID space exhausted")
    }

    fn send_command(&self, game_id: u64, cmd: GameCommand) {
        let cmd_tx = self
            .actors
            .read()
            .expect("game actors lock should not be poisoned")
            .get(&game_id)
            .map(|handle| handle.cmd_tx.clone());

        if let Some(cmd_tx) = cmd_tx
            && cmd_tx.try_send(cmd).is_err()
        {
            tracing::warn!("Failed to deliver command to game actor id={game_id}");
        }
    }

    pub fn add_score_for(&self, game_id: u64, team: Team, score: Score) {
        self.send_command(game_id, GameCommand::AddScore(team, score));
    }

    pub fn sub_score_for(&self, game_id: u64, team: Team, score: Score) {
        self.send_command(game_id, GameCommand::SubScore(team, score));
    }

    pub fn reset_shot_clock_for(&self, game_id: u64) {
        self.send_command(game_id, GameCommand::ResetShotClock);
    }

    pub fn reset_shot_clock_14s_for(&self, game_id: u64) {
        self.send_command(game_id, GameCommand::ResetShotClock14s);
    }

    pub fn resume_for(&self, game_id: u64) {
        self.send_command(game_id, GameCommand::Resume);
    }

    pub fn pause_for(&self, game_id: u64) {
        self.send_command(game_id, GameCommand::Pause);
    }

    pub fn end_for(&self, game_id: u64) {
        self.send_command(game_id, GameCommand::End);
    }

    pub fn reset_for(&self, game_id: u64) {
        self.send_command(game_id, GameCommand::Reset);
    }
}
