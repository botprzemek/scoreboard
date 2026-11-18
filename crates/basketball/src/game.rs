use crate::BasketballError;
use crate::{Discipline, Quarter, RegulationTime, ScoreRead, ShotClockTime, State, Team, layout};

use net::{ClientPacket, ServerPacket};

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Game {
    id: u64,
    state: State,
    discipline: Discipline,
    quarter: Quarter,

    home_score: u16,
    away_score: u16,

    regulation_millis: u32,
    clock_millis: u32,
}

impl Default for Game {
    fn default() -> Self {
        Self::new(1, Discipline::Fiba5v5)
    }
}

impl TryFrom<ClientPacket> for Game {
    type Error = BasketballError;

    fn try_from(packet: ClientPacket) -> Result<Self, Self::Error> {
        Self::from_bytes(&packet.data())
    }
}

impl TryFrom<ServerPacket> for Game {
    type Error = BasketballError;

    fn try_from(packet: ServerPacket) -> Result<Self, Self::Error> {
        Self::from_bytes(&packet.data())
    }
}

impl ScoreRead for Game {
    fn score(&self, team: Team) -> u16 {
        match team {
            Team::Home => self.home_score,
            Team::Away => self.away_score,
        }
    }

    fn scores(&self) -> (u16, u16) {
        (self.home_score, self.away_score)
    }
}

impl Game {
    #[must_use]
    pub fn new(id: u64, discipline: Discipline) -> Self {
        let config = discipline.config();

        Self {
            id,
            state: State::Scheduled,
            discipline,
            quarter: config.initial_quarter,
            home_score: 0,
            away_score: 0,
            regulation_millis: config.regulation_millis,
            clock_millis: config.clock_millis,
        }
    }

    #[must_use]
    pub const fn empty() -> Self {
        Self {
            id: 1,
            state: State::Scheduled,
            discipline: Discipline::Fiba5v5,
            quarter: Quarter::Q1,
            home_score: 0,
            away_score: 0,
            regulation_millis: 0,
            clock_millis: 0,
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, BasketballError> {
        if bytes.len() < layout::DATA_SIZE {
            return Err(BasketballError::InvalidPayloadLength);
        }
        let mut home_score = [0u8; 2];
        let mut away_score = [0u8; 2];
        let mut regulation_millis = [0u8; 4];
        let mut clock_millis = [0u8; 4];
        let mut id_bytes = [0u8; 8];

        home_score.copy_from_slice(&bytes[layout::HOME_SCORE_START..layout::HOME_SCORE_END]);
        away_score.copy_from_slice(&bytes[layout::AWAY_SCORE_START..layout::AWAY_SCORE_END]);

        regulation_millis.copy_from_slice(
            &bytes[layout::REGULATION_MILLIS_START..layout::REGULATION_MILLIS_END],
        );
        clock_millis.copy_from_slice(&bytes[layout::CLOCK_MILLIS_START..layout::CLOCK_MILLIS_END]);
        id_bytes.copy_from_slice(&bytes[layout::ID_START..layout::ID_END]);

        let raw_id = u64::from_be_bytes(id_bytes);
        let id = if raw_id == 0 { 1 } else { raw_id };

        Ok(Self {
            id,
            state: State::try_from(bytes[layout::STATE])?,
            discipline: Discipline::try_from(bytes[layout::DISCIPLINE])?,
            quarter: Quarter::try_from(bytes[layout::QUARTER])?,

            home_score: u16::from_be_bytes(home_score),
            away_score: u16::from_be_bytes(away_score),

            regulation_millis: u32::from_be_bytes(regulation_millis),
            clock_millis: u32::from_be_bytes(clock_millis),
        })
    }

    #[must_use]
    pub fn to_bytes(&self) -> [u8; layout::DATA_SIZE] {
        let mut bytes = [0u8; layout::DATA_SIZE];

        bytes[layout::STATE] = self.state as u8;
        bytes[layout::DISCIPLINE] = self.discipline as u8;
        bytes[layout::QUARTER] = self.quarter as u8;

        let home_score_bytes = self.home_score.to_be_bytes();
        let away_score_bytes = self.away_score.to_be_bytes();
        let regulation_millis_bytes = self.regulation_millis.to_be_bytes();
        let clock_millis_bytes = self.clock_millis.to_be_bytes();
        let id_bytes = self.id.to_be_bytes();

        bytes[layout::HOME_SCORE_START..layout::HOME_SCORE_END].copy_from_slice(&home_score_bytes);
        bytes[layout::AWAY_SCORE_START..layout::AWAY_SCORE_END].copy_from_slice(&away_score_bytes);
        bytes[layout::REGULATION_MILLIS_START..layout::REGULATION_MILLIS_END]
            .copy_from_slice(&regulation_millis_bytes);
        bytes[layout::CLOCK_MILLIS_START..layout::CLOCK_MILLIS_END]
            .copy_from_slice(&clock_millis_bytes);
        bytes[layout::ID_START..layout::ID_END].copy_from_slice(&id_bytes);

        bytes
    }
}

impl Game {
    #[must_use]
    pub fn id(&self) -> u64 {
        self.id
    }

    #[must_use]
    pub fn state(&self) -> State {
        self.state
    }

    #[must_use]
    pub fn discipline(&self) -> Discipline {
        self.discipline
    }

    #[must_use]
    pub fn quarter(&self) -> Quarter {
        self.quarter
    }

    #[must_use]
    pub fn home_score(&self) -> u16 {
        self.home_score
    }

    #[must_use]
    pub fn away_score(&self) -> u16 {
        self.away_score
    }

    #[must_use]
    pub fn regulation_millis(&self) -> u32 {
        self.regulation_millis
    }

    #[must_use]
    pub fn clock_millis(&self) -> u32 {
        self.clock_millis
    }
}

impl Game {
    pub(crate) fn set_state(&mut self, state: State) -> &mut Self {
        self.state = state;

        self
    }

    pub(crate) fn set_home_score(&mut self, score: u16) -> &mut Self {
        self.home_score = score;

        self
    }

    pub(crate) fn set_away_score(&mut self, score: u16) -> &mut Self {
        self.away_score = score;

        self
    }

    pub(crate) fn set_quarter(&mut self, quarter: Quarter) -> &mut Self {
        self.quarter = quarter;

        self
    }

    pub(crate) fn set_regulation_millis(&mut self, millis: u32) -> &mut Self {
        self.regulation_millis = millis;

        self
    }

    pub(crate) fn set_clock_millis(&mut self, millis: u32) -> &mut Self {
        self.clock_millis = millis;

        self
    }
}

impl Game {
    pub(crate) fn start_quarter(&mut self) {
        self.set_state(State::QuarterStart);
    }

    pub(crate) fn resume(&mut self) {
        self.set_state(State::InProgress);
    }

    pub(crate) fn pause(&mut self) {
        self.set_state(State::Paused);
    }

    pub(crate) fn end_quarter(&mut self) {
        self.set_state(State::QuarterEnd);
    }

    pub(crate) fn end(&mut self) {
        self.set_state(State::Final);
    }

    pub(crate) fn reset(&mut self) {
        *self = Self::new(self.id, self.discipline);
    }
}

impl Game {
    #[must_use]
    pub fn regulation_time(&self) -> RegulationTime {
        RegulationTime::from_millis(self.regulation_millis)
    }

    #[must_use]
    pub fn clock_time(&self) -> ShotClockTime {
        ShotClockTime::from_millis(self.clock_millis)
    }
}
