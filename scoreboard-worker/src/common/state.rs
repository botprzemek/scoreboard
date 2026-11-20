use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering, fence};
use scoreboard_basketball::Game;

pub static IS_CONNECTED_TO_MASTER: AtomicBool = AtomicBool::new(false);
pub static GAME_STATE: SharedGameState = SharedGameState::new();

/// Lock-free readable shared game state (seqlock).
///
/// Writers serialize via `critical_section`; readers (`get`) never acquire a
/// critical section so Core 1 HUB75 rendering cannot stall on Core 0 Wi-Fi/RTOS.
pub struct SharedGameState {
    seq: AtomicU32,
    state: UnsafeCell<Game>,
}

impl Default for SharedGameState {
    fn default() -> Self {
        Self::new()
    }
}

impl SharedGameState {
    pub const fn new() -> Self {
        Self {
            seq: AtomicU32::new(0),
            state: UnsafeCell::new(Game::empty()),
        }
    }

    #[inline]
    pub fn get(&self) -> Game {
        loop {
            let seq1 = self.seq.load(Ordering::Acquire);
            if seq1 & 1 != 0 {
                core::hint::spin_loop();
                continue;
            }
            // SAFETY: Torn reads are detected via `seq1 == seq2` with Acquire fences.
            let game = unsafe { core::ptr::read_volatile(self.state.get()) };
            fence(Ordering::Acquire);
            let seq2 = self.seq.load(Ordering::Relaxed);
            if seq1 == seq2 {
                return game;
            }
        }
    }

    #[inline]
    pub fn set(&self, game: Game) {
        critical_section::with(|_| {
            self.seq.fetch_add(1, Ordering::Release);
            fence(Ordering::Release);
            // SAFETY: Writers are serialized by `critical_section`.
            unsafe { core::ptr::write_volatile(self.state.get(), game) };
            self.seq.fetch_add(1, Ordering::Release);
        });
    }

    #[inline]
    pub fn update<F: FnOnce(&mut Game)>(&self, f: F) {
        critical_section::with(|_| {
            self.seq.fetch_add(1, Ordering::Release);
            fence(Ordering::Release);
            // SAFETY: Writers are serialized by `critical_section`.
            let mut game = unsafe { core::ptr::read_volatile(self.state.get()) };
            f(&mut game);
            unsafe { core::ptr::write_volatile(self.state.get(), game) };
            self.seq.fetch_add(1, Ordering::Release);
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
