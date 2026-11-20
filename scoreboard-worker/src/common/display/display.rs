#[cfg(any(feature = "rp235x", feature = "esp32s3"))]
use crate::common::display::layout::ScoreboardLayout;
use crate::common::state::{GAME_STATE, IS_CONNECTED_TO_MASTER};
use crate::common::traits::{Clock, DisplayTarget};

use core::sync::atomic::Ordering;
use core::time::Duration;
use scoreboard_basketball::{Game, GameMachine, Quarter, ScoreRead, State, Team};
use scoreboard_hub75::Color;
#[cfg(any(feature = "rp235x", feature = "esp32s3"))]
use scoreboard_hub75::{CORAL_RED, DisplayBuffer, HEIGHT, WIDTH};

#[derive(PartialEq, Eq)]
struct RenderKey {
    home_score: u16,
    away_score: u16,
    quarter: Quarter,
    minutes: u8,
    seconds: u8,
    centiseconds: u8,
    state: State,
    is_connected: bool,
}

impl RenderKey {
    fn from_game(game: &Game, is_connected: bool) -> Self {
        let reg = game.regulation_time();
        Self {
            home_score: game.score(Team::Home),
            away_score: game.score(Team::Away),
            quarter: game.quarter(),
            minutes: reg.minutes,
            seconds: reg.seconds,
            centiseconds: if reg.minutes == 0 {
                reg.centiseconds
            } else {
                0
            },
            state: game.state(),
            is_connected,
        }
    }
}

#[cfg(any(feature = "rp235x", feature = "esp32s3"))]
pub struct Hub75Display<'a, F> {
    buffer: &'a mut DisplayBuffer,
    layout: ScoreboardLayout,
    idle_animation: bool,
    present_fn: F,
}

#[cfg(any(feature = "rp235x", feature = "esp32s3"))]
impl<'a, F> Hub75Display<'a, F>
where
    F: FnMut(&DisplayBuffer, bool),
{
    pub fn new(buffer: &'a mut DisplayBuffer, idle_animation: bool, present_fn: F) -> Self {
        Self {
            buffer,
            layout: ScoreboardLayout::new(),
            idle_animation,
            present_fn,
        }
    }
}

#[cfg(any(feature = "rp235x", feature = "esp32s3"))]
impl<F> DisplayTarget for Hub75Display<'_, F>
where
    F: FnMut(&DisplayBuffer, bool),
{
    fn supports_idle_animation(&self) -> bool {
        self.idle_animation
    }

    fn update(&mut self, game: &Game, is_connected: bool) -> bool {
        self.buffer.set_brightness(100);
        self.layout
            .render(self.buffer, game, CORAL_RED, is_connected);
        true
    }

    fn update_idle(&mut self, color: Color) {
        self.buffer.set_brightness(100);
        self.buffer.clear();
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                self.buffer.set_pixel(x, y, color);
            }
        }
        self.buffer.prepare_bitplanes();
    }

    fn refresh(&mut self, is_connected: bool) {
        (self.present_fn)(self.buffer, is_connected);
    }
}

/// Platform-agnostic Core 1 display loop driver.
pub struct DisplayRunner<D, C> {
    target: D,
    clock: C,
    last_render_micros: u64,
    last_idle_color: Option<Color>,
    last_render_key: Option<RenderKey>,
    persist_ticked_game: bool,
}

impl<D: DisplayTarget, C: Clock> DisplayRunner<D, C> {
    pub fn new(target: D, clock: C) -> Self {
        let now = clock.now_micros();
        Self {
            target,
            clock,
            last_render_micros: now,
            last_idle_color: None,
            last_render_key: None,
            persist_ticked_game: false,
        }
    }

    /// Persist locally ticked game clock back to [`GAME_STATE`] (useful when no high-frequency updates arrive).
    #[cfg(feature = "linux")]
    pub const fn with_persisted_tick(mut self, persist: bool) -> Self {
        self.persist_ticked_game = persist;
        self
    }

    /// Execute one iteration of the display loop.
    pub fn step(&mut self) {
        let now = self.clock.now_micros();
        let delta_micros = now.saturating_sub(self.last_render_micros);
        self.last_render_micros = now;

        let is_connected = IS_CONNECTED_TO_MASTER.load(Ordering::Acquire);

        if !is_connected && self.target.supports_idle_animation() {
            self.last_render_key = None;
            let color = idle_palette_color((now / 1000) as u32);
            if self.last_idle_color != Some(color) {
                self.last_idle_color = Some(color);
                self.target.update_idle(color);
            }
            self.target.refresh(false);
            return;
        }

        self.last_idle_color = None;

        let delta = Duration::from_micros(delta_micros);
        let mut game = GAME_STATE.get();
        if game.state() == State::InProgress {
            if self.persist_ticked_game {
                GAME_STATE.update(|g| {
                    *g = tick_game_in_progress(*g, delta);
                });
                game = GAME_STATE.get();
            } else {
                game = tick_game_in_progress(game, delta);
            }
        }

        let key = RenderKey::from_game(&game, is_connected);
        if self.last_render_key.as_ref() != Some(&key) && self.target.update(&game, is_connected) {
            self.last_render_key = Some(key);
        }

        self.target.refresh(is_connected);
    }
}

pub fn tick_game_in_progress(game: Game, delta: Duration) -> Game {
    match GameMachine::from_in_progress(game) {
        Some(machine) => match machine.tick(delta) {
            Ok(running) => running.into_game(),
            Err(ended) => ended.into_game(),
        },
        None => game,
    }
}

pub fn idle_palette_color(elapsed_ms: u32) -> Color {
    const PALETTE: [Color; 3] = [[0xEE, 0x66, 0x3B], [0xF1, 0x56, 0x56], [0xA3, 0x49, 0xCE]];
    const STEP_DURATION_MS: u32 = 1000;
    const TOTAL_DURATION_MS: u32 = STEP_DURATION_MS * 3;

    let cycle_time = elapsed_ms % TOTAL_DURATION_MS;
    let current_step = (cycle_time / STEP_DURATION_MS) as usize;
    let next_step = (current_step + 1) % PALETTE.len();

    let step_progress = (cycle_time % STEP_DURATION_MS) * 256 / STEP_DURATION_MS;
    let inv_progress = 256 - step_progress;

    let c1 = PALETTE[current_step];
    let c2 = PALETTE[next_step];

    [
        ((c1[0] as u32 * inv_progress + c2[0] as u32 * step_progress) / 256) as u8,
        ((c1[1] as u32 * inv_progress + c2[1] as u32 * step_progress) / 256) as u8,
        ((c1[2] as u32 * inv_progress + c2[2] as u32 * step_progress) / 256) as u8,
    ]
}
