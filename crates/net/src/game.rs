use core::marker::PhantomData;

use crate::client_session::{ClientSession, Connected};
use crate::event::ClientEvent;
use crate::layout::DATA_SIZE;
use crate::transport::PacketSender;

pub struct GameIdle;
pub struct GameRunning;
pub struct GamePaused;
pub struct GameEnded;
pub struct GameSession<T, State> {
    session: ClientSession<T, Connected>,
    game_id: u64,
    _state: PhantomData<State>,
}

impl<T, State> GameSession<T, State> {
    pub(crate) fn new(session: ClientSession<T, Connected>, game_id: u64) -> Self {
        Self {
            session,
            game_id,
            _state: PhantomData,
        }
    }

    #[must_use]
    pub fn game_id(&self) -> u64 {
        self.game_id
    }

    #[must_use]
    pub fn session(&self) -> &ClientSession<T, Connected> {
        &self.session
    }

    pub fn session_mut(&mut self) -> &mut ClientSession<T, Connected> {
        &mut self.session
    }

    #[must_use]
    pub fn into_session(self) -> ClientSession<T, Connected> {
        self.session
    }

    fn game_id_payload(&self) -> [u8; DATA_SIZE] {
        let mut data = [0u8; DATA_SIZE];
        data[..8].copy_from_slice(&self.game_id.to_le_bytes());
        data
    }

    fn transition<Next>(self) -> GameSession<T, Next> {
        GameSession {
            session: self.session,
            game_id: self.game_id,
            _state: PhantomData,
        }
    }
}

impl<T: PacketSender, State> GameSession<T, State> {
    async fn send_game_event(&mut self, event: ClientEvent) {
        let payload = self.game_id_payload();
        self.session.send_event(event, 8, payload).await;
    }
}

impl<T: PacketSender> GameSession<T, GameIdle> {
    pub async fn start(mut self) -> GameSession<T, GameRunning> {
        self.send_game_event(ClientEvent::GameStart).await;
        self.transition()
    }
}

impl<T: PacketSender> GameSession<T, GameRunning> {
    pub async fn score_home_ft(&mut self) {
        self.send_game_event(ClientEvent::GameScoreHomeFT).await;
    }

    pub async fn score_home_2fg(&mut self) {
        self.send_game_event(ClientEvent::GameScoreHome2FG).await;
    }

    pub async fn score_home_3fg(&mut self) {
        self.send_game_event(ClientEvent::GameScoreHome3FG).await;
    }

    pub async fn score_away_ft(&mut self) {
        self.send_game_event(ClientEvent::GameScoreAwayFT).await;
    }

    pub async fn score_away_2fg(&mut self) {
        self.send_game_event(ClientEvent::GameScoreAway2FG).await;
    }

    pub async fn score_away_3fg(&mut self) {
        self.send_game_event(ClientEvent::GameScoreAway3FG).await;
    }

    pub async fn pause(mut self) -> GameSession<T, GamePaused> {
        self.send_game_event(ClientEvent::GamePause).await;
        self.transition()
    }

    pub async fn end(mut self) -> GameSession<T, GameEnded> {
        self.send_game_event(ClientEvent::GameEnd).await;
        self.transition()
    }
}

impl<T: PacketSender> GameSession<T, GamePaused> {
    pub async fn resume(mut self) -> GameSession<T, GameRunning> {
        self.send_game_event(ClientEvent::GameResume).await;
        self.transition()
    }

    pub async fn end(mut self) -> GameSession<T, GameEnded> {
        self.send_game_event(ClientEvent::GameEnd).await;
        self.transition()
    }
}

impl<T: PacketSender> GameSession<T, GameEnded> {
    pub async fn reset(mut self) -> GameSession<T, GameIdle> {
        self.send_game_event(ClientEvent::GameReset).await;
        self.transition()
    }
}
