use core::cell::RefCell;
use core::sync::atomic::AtomicBool;
use critical_section::Mutex as CsMutex;
use scoreboard_basketball::Game;

pub static IS_CONNECTED_TO_MASTER: AtomicBool = AtomicBool::new(false);
pub static GAME_STATE: SharedGameState = SharedGameState::new();

pub struct SharedGameState {
    state: CsMutex<RefCell<Game>>,
}

impl Default for SharedGameState {
    fn default() -> Self {
        Self::new()
    }
}

impl SharedGameState {
    pub const fn new() -> Self {
        Self {
            state: CsMutex::new(RefCell::new(Game::empty())),
        }
    }

    #[inline]
    pub fn get(&self) -> Game {
        critical_section::with(|cs| *self.state.borrow(cs).borrow())
    }

    #[inline]
    pub fn set(&self, game: Game) {
        critical_section::with(|cs| {
            *self.state.borrow(cs).borrow_mut() = game;
        });
    }

    #[inline]
    pub fn update<F: FnOnce(&mut Game)>(&self, f: F) {
        critical_section::with(|cs| {
            f(&mut self.state.borrow(cs).borrow_mut());
        });
    }
}

unsafe impl Sync for SharedGameState {}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Input {
    #[default]
    None = 0x00,
    BootSuccess = 0x10,
    #[cfg(feature = "rp235x")]
    BootFailed = 0x11,
    CoreSynced = 0x20,
    WifiConnected = 0x30,
    #[cfg(feature = "rp235x")]
    WifiFailed = 0x31,
    #[cfg(feature = "linux")]
    ErrorOccurred = 0x90,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum State {
    #[default]
    Boot = 0x01,
    CoreSync = 0x02,
    Networking = 0x03,
    Running = 0x04,
    ErrorRecovery = 0x05,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Machine {
    state: State,
}

impl Machine {
    #[must_use]
    pub const fn new() -> Self {
        Self { state: State::Boot }
    }

    /// Apply an input and return `Some(new_state)` if the state changed.
    pub fn transition(&mut self, input: Input) -> Option<State> {
        let old_state = self.state;
        self.state = match (self.state, input) {
            (State::Boot, Input::BootSuccess) => State::CoreSync,
            #[cfg(feature = "rp235x")]
            (State::Boot, Input::BootFailed) => State::ErrorRecovery,
            (State::CoreSync, Input::CoreSynced) => State::Networking,
            (State::Networking, Input::WifiConnected) => State::Running,
            #[cfg(feature = "rp235x")]
            (State::Networking, Input::WifiFailed) => State::ErrorRecovery,
            #[cfg(feature = "linux")]
            (_, Input::ErrorOccurred) => State::ErrorRecovery,
            _ => self.state,
        };

        (self.state != old_state).then_some(self.state)
    }
}
