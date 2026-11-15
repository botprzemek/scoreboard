use std::time::{Duration, Instant};

use basketball::{Game, State};

#[derive(Clone, Copy)]
pub(super) struct ClockAdvanceOutcome {
    pub ticked: bool,
    pub state_changed: bool,
}

#[derive(Clone, Copy)]
pub(super) struct ActiveGame {
    pub game: Game,
    pub last_tick: Option<Instant>,
}

impl ActiveGame {
    pub(super) fn new(game: Game, now: Instant) -> Self {
        let last_tick = if game.state() == State::Running {
            Some(now)
        } else {
            None
        };
        Self { game, last_tick }
    }

    pub(super) fn advance_clock(&mut self, now: Instant) -> ClockAdvanceOutcome {
        if self.game.state() != State::Running {
            self.last_tick = None;
            return ClockAdvanceOutcome {
                ticked: false,
                state_changed: false,
            };
        }

        let last = self.last_tick.get_or_insert(now);
        let elapsed_millis = now.duration_since(*last).as_millis() as u64;
        if elapsed_millis == 0 {
            return ClockAdvanceOutcome {
                ticked: false,
                state_changed: false,
            };
        }

        let previous_state = self.game.state();
        let previous_quarter = self.game.quarter();

        *last += Duration::from_millis(elapsed_millis);
        self.game.tick(Duration::from_millis(elapsed_millis));

        if self.game.state() != State::Running {
            self.last_tick = None;
        }

        let state_changed =
            previous_state != self.game.state() || previous_quarter != self.game.quarter();
        ClockAdvanceOutcome {
            ticked: true,
            state_changed,
        }
    }
}
