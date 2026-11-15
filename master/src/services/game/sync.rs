use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use crate::adapters::repositories::{GameRepository, Repository};
use crate::services::Observable;

use basketball::Game;
use net::{ServerEvent, ServerPacket};
use tokio::sync::{broadcast, watch};
use tokio::task;
use tokio::time::{self, MissedTickBehavior};

use super::active_game::ActiveGame;
use super::service::{FrameChannels, GameFrame, GameService};

const TICK_RATE: Duration = Duration::from_nanos(1_000_000_000 / 60);
const FRAME_PRUNE_TICKS: u32 = 600;

impl Observable<Vec<Game>> for GameService {
    fn subscribe(&self) -> broadcast::Receiver<Vec<Game>> {
        self.broadcast.subscribe()
    }

    fn watch(&self) -> watch::Receiver<Vec<Game>> {
        self.state.subscribe()
    }
}

impl GameService {
    #[must_use]
    pub fn subscribe_frames(&self, game_id: u16) -> watch::Receiver<Option<GameFrame>> {
        if let Some(sender) = self
            .frames
            .read()
            .expect("frame channels lock should not be poisoned")
            .get(&game_id)
        {
            return sender.subscribe();
        }

        self.frames
            .write()
            .expect("frame channels lock should not be poisoned")
            .entry(game_id)
            .or_insert_with(|| watch::channel(None).0)
            .subscribe()
    }
}

pub(super) fn spawn_db_flusher(
    repository_writer: GameRepository,
    database_cache: Arc<RwLock<HashMap<u16, ActiveGame>>>,
    database_flag: Arc<AtomicBool>,
    db_flush_interval: Duration,
) {
    tokio::spawn(async move {
        let mut database_interval = time::interval(db_flush_interval);
        database_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            database_interval.tick().await;
            if !database_flag.swap(false, Ordering::AcqRel) {
                continue;
            }

            let snapshot: Vec<Game> = {
                let games = database_cache
                    .read()
                    .expect("game cache lock should not be poisoned");
                games.values().map(|entry| entry.game).collect()
            };

            let repository = repository_writer.clone();
            let _ = task::spawn_blocking(move || {
                repository.upsert(&snapshot);
            })
            .await;
        }
    });
}

pub(super) fn spawn_clock_loop(
    clock_cache: Arc<RwLock<HashMap<u16, ActiveGame>>>,
    clock_state: watch::Sender<Vec<Game>>,
    clock_broadcast: broadcast::Sender<Vec<Game>>,
    clock_database_dirty: Arc<AtomicBool>,
    clock_frames: FrameChannels,
) {
    tokio::spawn(async move {
        let mut interval = time::interval(TICK_RATE);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut sequence_id: u16 = 0;
        let mut tick_counter: u32 = 0;

        loop {
            interval.tick().await;
            let now = Instant::now();
            let mut any_ticked = false;
            let mut any_state_changed = false;
            let mut snapshot = Vec::new();

            {
                let mut games = clock_cache
                    .write()
                    .expect("game cache lock should not be poisoned");
                for entry in games.values_mut() {
                    let outcome = entry.advance_clock(now);
                    any_ticked |= outcome.ticked;
                    any_state_changed |= outcome.state_changed;
                }
                if any_state_changed {
                    snapshot = GameService::collect_sorted_games(&games);
                }
            }

            if any_ticked {
                clock_database_dirty.store(true, Ordering::Release);
            }

            if any_state_changed {
                let _ = clock_state.send(snapshot.clone());
                if clock_broadcast.receiver_count() > 0 {
                    let _ = clock_broadcast.send(snapshot);
                }
            }

            publish_frames(&clock_cache, &clock_frames, sequence_id);
            sequence_id = sequence_id.wrapping_add(1);

            tick_counter = tick_counter.wrapping_add(1);
            if tick_counter.is_multiple_of(FRAME_PRUNE_TICKS) {
                clock_frames
                    .write()
                    .expect("frame channels lock should not be poisoned")
                    .retain(|_, sender| sender.receiver_count() > 0);
            }
        }
    });
}

fn publish_frames(
    cache: &RwLock<HashMap<u16, ActiveGame>>,
    frames: &RwLock<HashMap<u16, watch::Sender<Option<GameFrame>>>>,
    sequence_id: u16,
) {
    let frames = frames
        .read()
        .expect("frame channels lock should not be poisoned");
    if frames.is_empty() {
        return;
    }

    let cache = cache
        .read()
        .expect("game cache lock should not be poisoned");
    for (&game_id, sender) in frames.iter() {
        if sender.receiver_count() == 0 {
            continue;
        }

        if let Some(entry) = cache.get(&game_id) {
            let packet =
                ServerPacket::new(ServerEvent::GameUpdate, sequence_id, entry.game.to_bytes());
            sender.send_replace(Some(Arc::new(packet.to_bytes())));
        }
    }
}
