use crate::adapters::data::DataProvider;
use crate::adapters::repositories::{DeviceRepository, GameRepository};

pub struct Registry {
    game: GameRepository,
    devices: DeviceRepository,
}

pub trait Repository<Entity: Sized> {
    type Output;

    fn select(&self) -> Result<Self::Output, String>;

    fn upsert_many(&self, entities: &[Entity]) -> Result<(), String>;

    fn delete_many(&self, keys: &[String]) -> Result<(), String>;
}

impl Registry {
    #[must_use]
    pub fn new(database: impl Into<DataProvider>) -> Self {
        let database = database.into();
        let game = GameRepository::new(database.clone());
        let devices = DeviceRepository::new(database);

        Self { game, devices }
    }

    #[must_use]
    pub fn game(&self) -> &GameRepository {
        &self.game
    }

    #[must_use]
    pub fn devices(&self) -> &DeviceRepository {
        &self.devices
    }
}
