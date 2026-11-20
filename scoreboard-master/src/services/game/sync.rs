use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::adapters::repositories::{GameRepository, Repository};

use scoreboard_basketball::{Game, GameMachine, Score, ScoreWrite, State, Team};
use scoreboard_net::{ServerEvent, ServerPacket};
use tokio::sync::{broadcast, mpsc, watch};
use tokio::task::{JoinHandle, JoinSet};
use tokio::time::{self, MissedTickBehavior};
use tokio_util::sync::CancellationToken;

use super::active_game::ActiveGame;
use super::service::{GameFrame, GameHandle, GameService};

const TICK_RATE: Duration = Duration::from_nanos(1_000_000_000 / 60);
const CMD_QUEUE_CAPACITY: usize = 256;
pub(super) const EVENT_QUEUE_CAPACITY: usize = 1024;

pub(super) enum GameCommand {
    AddScore(Team, Score),
    SubScore(Team, Score),
    ResetShotClock,
    ResetShotClock14s,
    Resume,
    Pause,
    End,
    Reset,
}

impl GameService {
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<Vec<Game>> {
        self.broadcast.subscribe()
    }

    #[must_use]
    pub fn watch(&self) -> watch::Receiver<Vec<Game>> {
        self.state.subscribe()
    }

    #[must_use]
    pub fn subscribe_frames(&self, game_id: u64) -> broadcast::Receiver<GameFrame> {
        if let Some(handle) = self
            .actors
            .read()
            .expect("game actors lock should not be poisoned")
            .get(&game_id)
        {
            return handle.frame_tx.subscribe();
        }

        let (_tx, rx) = broadcast::channel(64);
        rx
    }
}

pub(super) fn spawn_game_actor(
    initial: Game,
    dirty_ids: Arc<Mutex<HashSet<u64>>>,
    events_tx: mpsc::Sender<Game>,
    shutdown: CancellationToken,
    tasks: &mut JoinSet<()>,
) -> GameHandle {
    let (cmd_tx, mut cmd_rx) = mpsc::channel(CMD_QUEUE_CAPACITY);
    let (latest_tx, latest_rx) = watch::channel(initial);
    let (frame_tx, _) = broadcast::channel(64);

    let handle = GameHandle {
        cmd_tx,
        latest_rx,
        frame_tx: frame_tx.clone(),
    };

    tasks.spawn(async move {
        let mut actor = GameActor {
            active: ActiveGame::new(initial, Instant::now()),
            latest_tx,
            dirty_ids,
            events_tx,
            frame_tx,
            last_published: initial,
            sequence_id: 0,
        };
        let mut interval = time::interval(TICK_RATE);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            let is_in_progress = actor.active.game.state() == State::InProgress;

            tokio::select! {
                () = shutdown.cancelled() => {
                    actor.on_shutdown(&mut cmd_rx);
                    return;
                }

                _ = interval.tick(), if is_in_progress => {
                    actor.on_tick();
                }

                cmd = cmd_rx.recv() => {
                    let Some(cmd) = cmd else {
                        return;
                    };
                    actor.on_receive_command(cmd);
                }
            }
        }
    });

    handle
}

struct GameActor {
    active: ActiveGame,
    latest_tx: watch::Sender<Game>,
    dirty_ids: Arc<Mutex<HashSet<u64>>>,
    events_tx: mpsc::Sender<Game>,
    frame_tx: broadcast::Sender<GameFrame>,
    last_published: Game,
    sequence_id: u16,
}

impl GameActor {
    fn on_shutdown(&mut self, cmd_rx: &mut mpsc::Receiver<GameCommand>) {
        while let Ok(cmd) = cmd_rx.try_recv() {
            self.on_receive_command(cmd);
        }
    }

    fn on_tick(&mut self) {
        let now = Instant::now();
        let outcome = self.active.advance_clock(now);
        if outcome.ticked {
            let _ = self.latest_tx.send(self.active.game);
            self.dirty_ids
                .lock()
                .expect("dirty game IDs lock should not be poisoned")
                .insert(self.active.game.id());
        }
        if outcome.state_changed
            && let Err(error) = self.events_tx.try_send(self.active.game)
        {
            tracing::warn!(
                "Failed to publish state change for game id={}: {error}",
                self.active.game.id()
            );
        }
        publish_frame_if_changed(
            self.active.game,
            &mut self.last_published,
            &self.frame_tx,
            &mut self.sequence_id,
        );
    }

    fn on_receive_command(&mut self, cmd: GameCommand) {
        let now = Instant::now();
        self.active.advance_clock(now);
        let previous_state = self.active.game.state();
        let previous_regulation = self.active.game.regulation_millis();

        apply_command(&mut self.active.game, cmd);

        if self.active.game.state() == State::InProgress {
            if previous_state != State::InProgress
                || previous_regulation != self.active.game.regulation_millis()
                || self.active.last_tick.is_none()
            {
                self.active.last_tick = Some(now);
            }
        } else {
            self.active.last_tick = None;
        }

        let _ = self.latest_tx.send(self.active.game);
        self.dirty_ids
            .lock()
            .expect("dirty game IDs lock should not be poisoned")
            .insert(self.active.game.id());
        if (self.active.game.state() != previous_state || self.active.game != self.last_published)
            && let Err(error) = self.events_tx.try_send(self.active.game)
        {
            tracing::warn!(
                "Failed to publish command update for game id={}: {error}",
                self.active.game.id()
            );
        }
        publish_frame_if_changed(
            self.active.game,
            &mut self.last_published,
            &self.frame_tx,
            &mut self.sequence_id,
        );
    }
}

fn apply_command(game: &mut Game, cmd: GameCommand) {
    match cmd {
        GameCommand::AddScore(team, score) => {
            if let Some(mut machine) = GameMachine::from_in_progress(*game) {
                machine.add_score(team, score);
                *game = machine.into_game();
            } else if let Some(mut machine) = GameMachine::from_paused(*game) {
                machine.add_score(team, score);
                *game = machine.into_game();
            }
        }
        GameCommand::SubScore(team, score) => {
            if let Some(mut machine) = GameMachine::from_in_progress(*game) {
                machine.sub_score(team, score);
                *game = machine.into_game();
            } else if let Some(mut machine) = GameMachine::from_paused(*game) {
                machine.sub_score(team, score);
                *game = machine.into_game();
            }
        }
        GameCommand::ResetShotClock => {
            if let Some(mut machine) = GameMachine::from_in_progress(*game) {
                machine.reset_shot_clock();
                *game = machine.into_game();
            } else if let Some(mut machine) = GameMachine::from_paused(*game) {
                machine.reset_shot_clock();
                *game = machine.into_game();
            }
        }
        GameCommand::ResetShotClock14s => {
            if let Some(mut machine) = GameMachine::from_in_progress(*game) {
                machine.reset_shot_clock_14s();
                *game = machine.into_game();
            } else if let Some(mut machine) = GameMachine::from_paused(*game) {
                machine.reset_shot_clock_14s();
                *game = machine.into_game();
            }
        }
        GameCommand::Resume => {
            if let Some(machine) = GameMachine::from_paused(*game) {
                *game = machine.resume().into_game();
            } else if let Some(machine) = GameMachine::from_quarter_start(*game) {
                *game = machine.start_quarter().into_game();
            } else if let Some(machine) = GameMachine::from_quarter_end(*game) {
                *game = match machine.advance_quarter() {
                    Ok(qs) => qs.start_quarter().into_game(),
                    Err(fin) => fin.into_game(),
                };
            } else if game.state().is_scheduled() {
                *game = GameMachine::new(game.id(), game.discipline())
                    .start()
                    .start_quarter()
                    .into_game();
            }
        }
        GameCommand::Pause => {
            if let Some(machine) = GameMachine::from_in_progress(*game) {
                *game = machine.pause().into_game();
            }
        }
        GameCommand::End => {
            if let Some(machine) = GameMachine::from_in_progress(*game) {
                *game = machine.end().into_game();
            } else if let Some(machine) = GameMachine::from_paused(*game) {
                *game = machine.end().into_game();
            } else if let Some(machine) = GameMachine::from_quarter_end(*game) {
                *game = machine.end().into_game();
            }
        }
        GameCommand::Reset => {
            if let Some(machine) = GameMachine::from_final(*game) {
                *game = machine.reset().into_game();
            } else {
                *game = GameMachine::new(game.id(), game.discipline()).into_game();
            }
        }
    }
}

fn publish_frame_if_changed(
    current: Game,
    last_published: &mut Game,
    frame_tx: &broadcast::Sender<GameFrame>,
    sequence_id: &mut u16,
) {
    if frame_tx.receiver_count() == 0 || current == *last_published {
        return;
    }

    *last_published = current;
    *sequence_id = sequence_id.wrapping_add(1);
    let packet = ServerPacket::new(ServerEvent::GameUpdate, *sequence_id, current.to_bytes());
    let _ = frame_tx.send(Arc::new(packet.to_bytes()));
}

pub(super) fn spawn_db_flusher(
    repository_writer: GameRepository,
    actors: Arc<std::sync::RwLock<HashMap<u64, GameHandle>>>,
    dirty_ids: Arc<Mutex<HashSet<u64>>>,
    db_flush_interval: Duration,
    shutdown: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut database_interval = time::interval(db_flush_interval);
        database_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = database_interval.tick() => {
                    flush_dirty_games(&repository_writer, &actors, &dirty_ids).await;
                }
                () = shutdown.cancelled() => {
                    flush_dirty_games(&repository_writer, &actors, &dirty_ids).await;
                    return;
                }
            }
        }
    })
}

async fn flush_dirty_games(
    repository_writer: &GameRepository,
    actors: &std::sync::RwLock<HashMap<u64, GameHandle>>,
    dirty_ids: &Mutex<HashSet<u64>>,
) {
    let ids: Vec<u64> = {
        let mut guard = dirty_ids
            .lock()
            .expect("dirty game IDs lock should not be poisoned");
        if guard.is_empty() {
            return;
        }
        guard.drain().collect()
    };

    let dirty_games: Vec<Game> = {
        let actors_guard = actors
            .read()
            .expect("game actors lock should not be poisoned");
        ids.iter()
            .filter_map(|id| actors_guard.get(id).map(|h| *h.latest_rx.borrow()))
            .collect()
    };

    if dirty_games.is_empty() {
        return;
    }

    let repo = repository_writer.clone();
    if crate::services::retry_blocking("Game DB flush", move || repo.upsert_many(&dirty_games))
        .await
    {
        return;
    }

    dirty_ids
        .lock()
        .expect("dirty game IDs lock should not be poisoned")
        .extend(ids);
}

pub(super) fn spawn_snapshot_aggregator(
    initial_games: Vec<Game>,
    mut events_rx: mpsc::Receiver<Game>,
    state: watch::Sender<Vec<Game>>,
    broadcast: broadcast::Sender<Vec<Game>>,
    shutdown: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut games: BTreeMap<u64, Game> = initial_games
            .into_iter()
            .map(|game| (game.id(), game))
            .collect();

        loop {
            tokio::select! {
                () = shutdown.cancelled() => return,
                event = events_rx.recv() => {
                    let Some(game) = event else {
                        return;
                    };
                    games.insert(game.id(), game);
                    let snapshot: Vec<Game> = games.values().copied().collect();
                    let _ = state.send(snapshot.clone());
                    if broadcast.receiver_count() > 0 {
                        let _ = broadcast.send(snapshot);
                    }
                }
            }
        }
    })
}
