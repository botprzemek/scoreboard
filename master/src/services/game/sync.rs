use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::adapters::repositories::{GameRepository, Repository};

use basketball::{Game, Score, ScoreWrite, State, Team};
use net::{ServerEvent, ServerPacket};
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
        let mut active = ActiveGame::new(initial, Instant::now());
        let mut interval = time::interval(TICK_RATE);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut sequence_id: u16 = 0;
        let mut last_published = initial;

        loop {
            let is_running = active.game.state() == State::Running;

            tokio::select! {
                () = shutdown.cancelled() => {
                    while let Ok(cmd) = cmd_rx.try_recv() {
                        let now = Instant::now();
                        active.advance_clock(now);
                        apply_command(&mut active.game, cmd);
                        let _ = latest_tx.send(active.game);
                        dirty_ids
                            .lock()
                            .expect("dirty game IDs lock should not be poisoned")
                            .insert(active.game.id());
                    }
                    return;
                }

                _ = interval.tick(), if is_running => {
                    let now = Instant::now();
                    let outcome = active.advance_clock(now);
                    if outcome.ticked {
                        let _ = latest_tx.send(active.game);
                        dirty_ids
                            .lock()
                            .expect("dirty game IDs lock should not be poisoned")
                            .insert(active.game.id());
                    }
                    if outcome.state_changed
                        && let Err(error) = events_tx.try_send(active.game)
                    {
                        tracing::warn!(
                            "Failed to publish state change for game id={}: {error}",
                            active.game.id()
                        );
                    }
                    publish_frame_if_changed(
                        active.game,
                        &mut last_published,
                        &frame_tx,
                        &mut sequence_id,
                    );
                }

                cmd = cmd_rx.recv() => {
                    let Some(cmd) = cmd else {
                        return;
                    };
                    let now = Instant::now();
                    active.advance_clock(now);
                    let previous_state = active.game.state();
                    let previous_regulation = active.game.regulation_millis();

                    apply_command(&mut active.game, cmd);

                    if active.game.state() == State::Running {
                        if previous_state != State::Running
                            || previous_regulation != active.game.regulation_millis()
                            || active.last_tick.is_none()
                        {
                            active.last_tick = Some(now);
                        }
                    } else {
                        active.last_tick = None;
                    }

                    let _ = latest_tx.send(active.game);
                    dirty_ids
                        .lock()
                        .expect("dirty game IDs lock should not be poisoned")
                        .insert(active.game.id());
                    if (active.game.state() != previous_state || active.game != last_published)
                        && let Err(error) = events_tx.try_send(active.game)
                    {
                        tracing::warn!(
                            "Failed to publish command update for game id={}: {error}",
                            active.game.id()
                        );
                    }
                    publish_frame_if_changed(
                        active.game,
                        &mut last_published,
                        &frame_tx,
                        &mut sequence_id,
                    );
                }
            }
        }
    });

    handle
}

fn apply_command(game: &mut Game, cmd: GameCommand) {
    match cmd {
        GameCommand::AddScore(team, score) => game.add_score(team, score),
        GameCommand::Resume => game.resume(),
        GameCommand::Pause => game.pause(),
        GameCommand::End => game.set_state(State::GameEnd),
        GameCommand::Reset => *game = Game::new_with_id(game.id(), game.discipline()),
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
    if crate::services::retry_blocking("Game DB flush", move || repo.upsert_many(&dirty_games)).await
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
