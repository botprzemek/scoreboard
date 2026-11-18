use crate::adapters::data::{DataProvider, Table};
use crate::adapters::repositories::Repository;

use basketball::Game;

#[derive(Clone)]
pub struct GameRepository {
    database: DataProvider,
}

impl Repository<Game> for GameRepository {
    type Output = Vec<Game>;

    fn select(&self) -> Result<Vec<Game>, String> {
        let mut games: Vec<Game> = self
            .database
            .read_all(Table::Games)?
            .into_iter()
            .filter_map(Self::decode_entry)
            .collect();

        games.sort_by_key(Game::id);
        games.dedup_by_key(|game| game.id());
        Ok(games)
    }

    fn upsert_many(&self, entities: &[Game]) -> Result<(), String> {
        if entities.is_empty() {
            return Ok(());
        }

        let entries = entities
            .iter()
            .map(Self::to_entry)
            .collect::<Vec<(String, Vec<u8>)>>();

        self.database.upsert_many(Table::Games, &entries)
    }

    fn delete_many(&self, keys: &[String]) -> Result<(), String> {
        self.database.delete_many(Table::Games, keys)
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

    fn decode_entry((key, value): (String, Vec<u8>)) -> Option<Game> {
        match Game::from_bytes(&value) {
            Ok(game) => Some(game),
            Err(error) => {
                tracing::warn!("Skipping invalid game record for key '{key}': {error}");
                None
            }
        }
    }
}
