use super::active_game::ActiveGame;
use super::sync;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use crate::adapters::repositories::{GameRepository, Repository};

use basketball::{Discipline, Game, Score, ScoreRead, ScoreWrite, Scores, State, Team};
use net::layout::PACKET_SIZE;
use tokio::sync::{broadcast, watch};

pub type GameFrame = Arc<[u8; PACKET_SIZE]>;

pub(super) type FrameChannels = Arc<RwLock<HashMap<u16, watch::Sender<Option<GameFrame>>>>>;

#[derive(Clone)]
pub struct GameService {
    #[allow(dead_code)]
    game_repository: GameRepository,
    pub(super) state: watch::Sender<Vec<Game>>,
    cache: Arc<RwLock<HashMap<u16, ActiveGame>>>,
    database_dirty: Arc<AtomicBool>,
    pub(super) broadcast: broadcast::Sender<Vec<Game>>,
    pub(super) frames: FrameChannels,
}

impl GameService {
    #[must_use]
    pub fn new(game_repository: GameRepository, db_flush_interval: Duration) -> Self {
        let mut initial_games = game_repository.select();
        if initial_games.is_empty() {
            let default_game = Game::new_with_id(1, Discipline::Fiba5v5);
            initial_games.push(default_game);
            game_repository.upsert(&initial_games);
        }

        let now = Instant::now();
        let cache_map: HashMap<u16, ActiveGame> = initial_games
            .iter()
            .map(|&game| (game.id(), ActiveGame::new(game, now)))
            .collect();
        let cache = Arc::new(RwLock::new(cache_map));
        let database_dirty = Arc::new(AtomicBool::new(false));
        let frames: FrameChannels = Arc::new(RwLock::new(HashMap::new()));

        let (state, _state_rx) = watch::channel(initial_games);
        let (broadcast, _broadcast_rx) = broadcast::channel(64);

        sync::spawn_db_flusher(
            game_repository.clone(),
            Arc::clone(&cache),
            Arc::clone(&database_dirty),
            db_flush_interval,
        );
        sync::spawn_clock_loop(
            Arc::clone(&cache),
            state.clone(),
            broadcast.clone(),
            Arc::clone(&database_dirty),
            Arc::clone(&frames),
        );

        Self {
            game_repository,
            state,
            cache,
            database_dirty,
            broadcast,
            frames,
        }
    }

    #[must_use]
    pub fn all(&self) -> Vec<Game> {
        let cache = self
            .cache
            .read()
            .expect("game cache lock should not be poisoned");
        Self::collect_sorted_games(&cache)
    }

    pub(super) fn collect_sorted_games(cache: &HashMap<u16, ActiveGame>) -> Vec<Game> {
        let mut games: Vec<Game> = cache.values().map(|entry| entry.game).collect();
        games.sort_by_key(Game::id);
        games
    }

    #[must_use]
    pub fn by_id(&self, id: u16) -> Option<Game> {
        let cache = self
            .cache
            .read()
            .expect("game cache lock should not be poisoned");
        cache.get(&id).map(|active| active.game)
    }

    #[must_use]
    pub fn default_game(&self) -> Game {
        self.by_id(1)
            .unwrap_or_else(|| Game::new_with_id(1, Discipline::Fiba5v5))
    }

    #[must_use]
    pub fn create_game(&self, discipline: Discipline) -> Game {
        let now = Instant::now();
        let created;
        let snapshot: Vec<Game>;

        {
            let mut cache = self
                .cache
                .write()
                .expect("game cache lock should not be poisoned");
            let next_id = Self::next_game_id(&cache);
            created = Game::new_with_id(next_id, discipline);
            cache.insert(next_id, ActiveGame::new(created, now));
            snapshot = Self::collect_sorted_games(&cache);
        }

        self.database_dirty.store(true, Ordering::Release);
        let _ = self.state.send(snapshot.clone());

        if self.broadcast.receiver_count() > 0 {
            let _ = self.broadcast.send(snapshot);
        }

        created
    }

    fn next_game_id(cache: &HashMap<u16, ActiveGame>) -> u16 {
        cache
            .keys()
            .copied()
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .or_else(|| (1..=u16::MAX).find(|id| !cache.contains_key(id)))
            .expect("game ID space exhausted")
    }

    pub fn modify_game<F>(&self, game_id: u16, modifier: F) -> Option<Game>
    where
        F: FnOnce(&mut Game),
    {
        let now = Instant::now();
        let updated_game;
        let snapshot: Vec<Game>;

        {
            let mut cache = self
                .cache
                .write()
                .expect("game cache lock should not be poisoned");
            let entry = cache.get_mut(&game_id)?;

            entry.advance_clock(now);
            let previous_state = entry.game.state();
            let previous_regulation = entry.game.regulation_millis();

            modifier(&mut entry.game);

            if entry.game.state() == State::Running {
                if previous_state != State::Running
                    || previous_regulation != entry.game.regulation_millis()
                    || entry.last_tick.is_none()
                {
                    entry.last_tick = Some(now);
                }
            } else {
                entry.last_tick = None;
            }

            updated_game = entry.game;
            snapshot = Self::collect_sorted_games(&cache);
        }

        self.database_dirty.store(true, Ordering::Release);
        let _ = self.state.send(snapshot.clone());

        if self.broadcast.receiver_count() > 0
            && let Err(error) = self.broadcast.send(snapshot)
        {
            eprintln!("ERROR: Failed to broadcast: {error}");
        }

        Some(updated_game)
    }

    pub async fn scores(&self) -> Scores {
        let game = self.default_game();
        game.scores()
    }

    pub async fn score(&self, team: Team) -> u16 {
        let game = self.default_game();
        game.score(team)
    }

    pub async fn add_score(&self, team: Team, score: Score) {
        let id = self.default_game().id();
        self.add_score_for(id, team, score).await;
    }

    pub async fn add_score_for(&self, game_id: u16, team: Team, score: Score) {
        self.modify_game(game_id, |game| game.add_score(team, score));
    }

    pub async fn resume(&self) {
        let id = self.default_game().id();
        self.resume_for(id).await;
    }

    pub async fn resume_for(&self, game_id: u16) {
        self.modify_game(game_id, Game::resume);
    }

    pub async fn pause(&self) {
        let id = self.default_game().id();
        self.pause_for(id).await;
    }

    pub async fn pause_for(&self, game_id: u16) {
        self.modify_game(game_id, Game::pause);
    }

    pub async fn end_for(&self, game_id: u16) {
        self.modify_game(game_id, |game| game.set_state(State::GameEnd));
    }

    pub async fn reset_for(&self, game_id: u16) {
        self.modify_game(game_id, |game| {
            *game = Game::new_with_id(game.id(), game.discipline());
        });
    }
}
