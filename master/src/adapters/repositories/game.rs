use crate::adapters::data::{DataProvider, GAMES};
use crate::adapters::repositories::Repository;

use basketball::Game;

#[derive(Clone)]
pub struct GameRepository {
    database: DataProvider,
}

impl Repository<[Game]> for GameRepository {
    type Output = Vec<Game>;

    fn select(&self) -> Vec<Game> {
        let mut games: Vec<Game> = self
            .database
            .read_all(GAMES)
            .into_iter()
            .filter_map(Self::decode_entry)
            .map(Self::with_entry_id)
            .collect();

        games.sort_by_key(Game::id);
        games.dedup_by_key(|game| game.id());
        games
    }

    fn upsert(&self, entities: &[Game]) {
        let entries = entities
            .iter()
            .map(Self::to_entry)
            .collect::<Vec<(String, Vec<u8>)>>();

        self.database.replace_all(GAMES, &entries);
    }
}

impl GameRepository {
    #[must_use]
    pub fn new(database: impl Into<DataProvider>) -> Self {
        Self {
            database: database.into(),
        }
    }

    fn to_entry(game: &Game) -> (String, Vec<u8>) {
        (game.id().to_string(), game.to_bytes().to_vec())
    }

    fn decode_entry((key, value): (String, Vec<u8>)) -> Option<(String, Game)> {
        let game = Game::from_bytes(&value).ok()?;

        Some((key, game))
    }

    fn with_entry_id((key, mut game): (String, Game)) -> Game {
        let key = match key.parse::<u16>() {
            Ok(key) => key,
            Err(_) => return game,
        };

        if key == 0 {
            return game;
        }

        game.set_id(key);
        game
    }
}
