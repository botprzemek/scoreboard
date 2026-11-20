use core::marker::PhantomData;
use core::time::Duration;

use crate::{Discipline, Game, Quarter, Score, ScoreRead, ScoreWrite, Team};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scheduled;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuarterStart;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InProgress;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Paused;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuarterEnd;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Final;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameMachine<S> {
    game: Game,
    _marker: PhantomData<S>,
}

impl Default for GameMachine<Scheduled> {
    fn default() -> Self {
        Self {
            game: Game::default(),
            _marker: PhantomData,
        }
    }
}

impl GameMachine<Scheduled> {
    #[must_use]
    pub fn new(id: u64, discipline: Discipline) -> Self {
        Self {
            game: Game::new(id, discipline),
            _marker: PhantomData,
        }
    }

    #[must_use]
    pub fn start(mut self) -> GameMachine<QuarterStart> {
        self.game.start_quarter();

        GameMachine {
            game: self.game,
            _marker: PhantomData,
        }
    }
}

impl GameMachine<QuarterStart> {
    #[must_use]
    pub fn from_quarter_start(game: Game) -> Option<Self> {
        game.state().has_quarter_started().then_some(Self {
            game,
            _marker: PhantomData,
        })
    }

    #[must_use]
    pub fn start_quarter(mut self) -> GameMachine<InProgress> {
        let config = self.game.discipline().config();
        let regulation_millis = if self.game.quarter() == Quarter::OT {
            300_000
        } else {
            config.regulation_millis
        };
        self.game
            .set_regulation_millis(regulation_millis)
            .set_clock_millis(config.clock_millis)
            .resume();

        GameMachine {
            game: self.game,
            _marker: PhantomData,
        }
    }
}

impl GameMachine<InProgress> {
    #[must_use]
    pub fn from_in_progress(game: Game) -> Option<Self> {
        game.state().is_in_progress().then_some(Self {
            game,
            _marker: PhantomData,
        })
    }

    #[must_use]
    pub fn pause(mut self) -> GameMachine<Paused> {
        self.game.pause();

        GameMachine {
            game: self.game,
            _marker: PhantomData,
        }
    }

    #[must_use]
    pub fn end(mut self) -> GameMachine<Final> {
        self.game.end();

        GameMachine {
            game: self.game,
            _marker: PhantomData,
        }
    }

    pub fn tick(
        mut self,
        delta: Duration,
    ) -> Result<GameMachine<InProgress>, GameMachine<QuarterEnd>> {
        let delta_ms = delta.as_millis() as u32;
        let remaining = self.game.regulation_millis().saturating_sub(delta_ms);
        let clock_remaining = self.game.clock_millis().saturating_sub(delta_ms);
        self.game
            .set_regulation_millis(remaining)
            .set_clock_millis(clock_remaining);

        if remaining > 0 {
            return Ok(self);
        }

        self.game.end_quarter();

        Err(GameMachine {
            game: self.game,
            _marker: PhantomData,
        })
    }
}

impl GameMachine<Paused> {
    #[must_use]
    pub fn from_paused(game: Game) -> Option<Self> {
        game.state().is_paused().then_some(Self {
            game,
            _marker: PhantomData,
        })
    }

    #[must_use]
    pub fn resume(mut self) -> GameMachine<InProgress> {
        self.game.resume();

        GameMachine {
            game: self.game,
            _marker: PhantomData,
        }
    }

    #[must_use]
    pub fn end(mut self) -> GameMachine<Final> {
        self.game.end();

        GameMachine {
            game: self.game,
            _marker: PhantomData,
        }
    }
}

impl GameMachine<QuarterEnd> {
    #[must_use]
    pub fn from_quarter_end(game: Game) -> Option<Self> {
        game.state().has_quarter_end().then_some(Self {
            game,
            _marker: PhantomData,
        })
    }

    pub fn advance_quarter(mut self) -> Result<GameMachine<QuarterStart>, GameMachine<Final>> {
        let next = match self.game.quarter() {
            Quarter::Q1 => Quarter::Q2,
            Quarter::Q2 => Quarter::Q3,
            Quarter::Q3 => Quarter::Q4,
            Quarter::Q4 | Quarter::OT => {
                if self.game.home_score() == self.game.away_score() {
                    Quarter::OT
                } else {
                    self.game.end();

                    return Err(GameMachine {
                        game: self.game,
                        _marker: PhantomData,
                    });
                }
            }
            Quarter::None => {
                self.game.end();

                return Err(GameMachine {
                    game: self.game,
                    _marker: PhantomData,
                });
            }
        };

        self.game.start_quarter();
        self.game.set_quarter(next);

        Ok(GameMachine {
            game: self.game,
            _marker: PhantomData,
        })
    }

    #[must_use]
    pub fn end(mut self) -> GameMachine<Final> {
        self.game.end();

        GameMachine {
            game: self.game,
            _marker: PhantomData,
        }
    }
}

impl GameMachine<Final> {
    #[must_use]
    pub fn from_final(game: Game) -> Option<Self> {
        game.state().is_final().then_some(Self {
            game,
            _marker: PhantomData,
        })
    }

    #[must_use]
    pub fn reset(mut self) -> GameMachine<Scheduled> {
        self.game.reset();

        GameMachine {
            game: self.game,
            _marker: PhantomData,
        }
    }
}

pub trait CanModifyScore {}
impl CanModifyScore for InProgress {}
impl CanModifyScore for Paused {}

impl<S> ScoreRead for GameMachine<S> {
    fn score(&self, team: Team) -> u16 {
        self.game.score(team)
    }

    fn scores(&self) -> (u16, u16) {
        self.game.scores()
    }
}

impl<S: CanModifyScore> ScoreWrite for GameMachine<S> {
    fn add_score(&mut self, team: Team, score: Score) -> &mut GameMachine<S> {
        let score = self.game.score(team).saturating_add(score.into());

        match team {
            Team::Home => self.game.set_home_score(score),
            Team::Away => self.game.set_away_score(score),
        };

        self
    }

    fn sub_score(&mut self, team: Team, score: Score) -> &mut GameMachine<S> {
        let score = self.game.score(team).saturating_sub(score.into());

        match team {
            Team::Home => self.game.set_home_score(score),
            Team::Away => self.game.set_away_score(score),
        };

        self
    }
}

impl<S: CanModifyScore> GameMachine<S> {
    pub fn reset_shot_clock(&mut self) -> &mut Self {
        let clock_millis = self.game.discipline().config().clock_millis;
        self.game.set_clock_millis(clock_millis);
        self
    }

    pub fn reset_shot_clock_14s(&mut self) -> &mut Self {
        let clock_millis = match self.game.discipline() {
            Discipline::Fiba5v5 => 14_000,
            _ => self.game.discipline().config().clock_millis,
        };
        self.game.set_clock_millis(clock_millis);
        self
    }
}

impl<S> GameMachine<S> {
    #[must_use]
    pub const fn game(&self) -> &Game {
        &self.game
    }

    #[must_use]
    pub fn into_game(self) -> Game {
        self.game
    }
}
