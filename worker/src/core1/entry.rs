#[cfg(any(feature = "rp235x", feature = "esp32s3"))]
use crate::core1::ScoreboardLayout;
#[cfg(feature = "rp235x")]
use crate::peripherals::LedPeripherals;
#[cfg(feature = "rp235x")]
use crate::state::CORE1_READY_SIGNAL;
#[cfg(any(feature = "rp235x", feature = "esp32s3"))]
use crate::state::{GAME_STATE, IS_CONNECTED_TO_MASTER};

#[cfg(any(feature = "rp235x", feature = "esp32s3"))]
use basketball::{Quarter, ScoreRead, Team};
#[cfg(any(feature = "rp235x", feature = "esp32s3"))]
use core::sync::atomic::Ordering;
#[cfg(feature = "rp235x")]
use embassy_executor::{Executor, Spawner};
#[cfg(feature = "rp235x")]
use embassy_futures::yield_now;
#[cfg(any(feature = "rp235x", feature = "esp32s3"))]
use hub75::{CORAL_RED, DisplayBuffer, LedOutputs};
#[cfg(any(feature = "rp235x", feature = "esp32s3"))]
use static_cell::StaticCell;

#[cfg(feature = "linux")]
use crate::state::CORE1_READY_SIGNAL;
#[cfg(feature = "linux")]
use crate::state::GAME_STATE;
#[cfg(feature = "linux")]
use basketball::ScoreRead;

#[cfg(feature = "rp235x")]
pub static EXECUTOR: StaticCell<Executor> = StaticCell::new();
#[cfg(any(feature = "rp235x", feature = "esp32s3"))]
static DISPLAY: StaticCell<DisplayBuffer<64, 32, 16>> = StaticCell::new();

pub struct Core1;

#[cfg(feature = "rp235x")]
impl Core1 {
    pub fn entry(led: LedPeripherals) -> ! {
        let executor = Executor::new();

        EXECUTOR.init(executor).run(|spawner| {
            spawner.spawn(defmt::unwrap!(task(spawner, led)));

            CORE1_READY_SIGNAL.signal(());
        })
    }
}

#[cfg(any(feature = "rp235x", feature = "esp32s3"))]
#[derive(PartialEq, Eq)]
struct RenderKey {
    home_score: u16,
    away_score: u16,
    quarter: Quarter,
    minutes: u8,
    seconds: u8,
    centiseconds: u8,
    is_connected: bool,
}

#[cfg(feature = "rp235x")]
#[embassy_executor::task]
async fn task(_spawner: Spawner, peripherals: LedPeripherals) {
    let mut outputs = LedOutputs::from_peripherals(peripherals);
    let display = DISPLAY.init_with(DisplayBuffer::new);
    let mut layout = ScoreboardLayout::new();
    let mut last_render = embassy_time::Instant::now();
    let mut last_idle_color: Option<[u8; 3]> = None;
    let mut last_render_key: Option<RenderKey> = None;

    defmt::info!("🚀 [Core 1] Display runner started (Hardware SIO / DMA Engine)");

    loop {
        let now = embassy_time::Instant::now();
        let delta_micros = (now - last_render).as_micros();
        last_render = now;

        let is_connected = IS_CONNECTED_TO_MASTER.load(Ordering::Acquire);

        if !is_connected {
            last_render_key = None;

            const PALETTE: [[u8; 3]; 3] =
                [[0xEE, 0x66, 0x3B], [0xF1, 0x56, 0x56], [0xA3, 0x49, 0xCE]];

            let elapsed_ms = (now.as_micros() / 1000) as u32;
            const STEP_DURATION_MS: u32 = 1000;
            const TOTAL_DURATION_MS: u32 = STEP_DURATION_MS * 3;

            let cycle_time = elapsed_ms % TOTAL_DURATION_MS;
            let current_step = (cycle_time / STEP_DURATION_MS) as usize;
            let next_step = (current_step + 1) % PALETTE.len();

            let step_progress = (cycle_time % STEP_DURATION_MS) * 256 / STEP_DURATION_MS;
            let inv_progress = 256 - step_progress;

            let c1 = PALETTE[current_step];
            let c2 = PALETTE[next_step];

            let current_color: [u8; 3] = [
                ((c1[0] as u32 * inv_progress + c2[0] as u32 * step_progress) / 256) as u8,
                ((c1[1] as u32 * inv_progress + c2[1] as u32 * step_progress) / 256) as u8,
                ((c1[2] as u32 * inv_progress + c2[2] as u32 * step_progress) / 256) as u8,
            ];

            if last_idle_color != Some(current_color) {
                last_idle_color = Some(current_color);
                display.set_brightness(100);
                display.clear();
                for y in 0..32 {
                    for x in 0..64 {
                        display.set_pixel(x, y, current_color);
                    }
                }
                display.prepare_bitplanes();
            }

            outputs.render_bcm_frame_sio_fast(display.bitplanes(), 30);
        } else {
            last_idle_color = None;

            let mut game = GAME_STATE.get();
            if game.state() == basketball::State::InProgress {
                let _ = game.tick(core::time::Duration::from_micros(delta_micros));
            }

            let reg = game.regulation_time();
            let key = RenderKey {
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
                is_connected,
            };

            if last_render_key.as_ref() != Some(&key) {
                last_render_key = Some(key);
                display.set_brightness(100);
                layout.render(display, &game, CORAL_RED, is_connected);
            }

            outputs.render_bcm_frame_sio_fast(display.bitplanes(), 20);
        }

        yield_now().await;
    }
}

#[cfg(feature = "esp32s3")]
impl Core1 {
    pub fn entry(led: hub75::LedPeripherals) -> ! {
        let mut outputs = LedOutputs::from_peripherals(led);
        let display = DISPLAY.init_with(DisplayBuffer::new);
        let mut layout = ScoreboardLayout::new();
        let mut last_render = esp_hal::time::Instant::now();
        let mut last_render_key: Option<RenderKey> = None;

        loop {
            let now = esp_hal::time::Instant::now();
            let delta_micros = (now - last_render).as_micros();
            last_render = now;

            let is_connected = IS_CONNECTED_TO_MASTER.load(Ordering::Acquire);
            let mut game = GAME_STATE.get();
            if game.state() == basketball::State::InProgress {
                let _ = game.tick(core::time::Duration::from_micros(delta_micros));
            }

            let reg = game.regulation_time();
            let key = RenderKey {
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
                is_connected,
            };

            if last_render_key.as_ref() != Some(&key) {
                last_render_key = Some(key);
                display.set_brightness(100);
                layout.render(display, &game, CORAL_RED, is_connected);
            }

            outputs.render_bcm_frame_esp32s3_fast(display.bitplanes(), 25);
        }
    }
}

#[cfg(feature = "linux")]
impl Core1 {
    pub fn entry() {
        println!("🚀 [Core 1 Thread] Text Scoreboard Runner started");
        CORE1_READY_SIGNAL.signal();

        let mut last_render = std::time::Instant::now();
        let mut last_draw = std::time::Instant::now() - std::time::Duration::from_secs(1);
        let mut last_snapshot: Option<(basketball::Game, bool)> = None;

        loop {
            let now = std::time::Instant::now();
            let delta = now.duration_since(last_render);
            last_render = now;

            let mut game = GAME_STATE.get();

            if game.state() == basketball::State::Running {
                GAME_STATE.update(|g| {
                    if g.state() == basketball::State::Running {
                        g.tick(delta);
                    }
                });
                game = GAME_STATE.get();
            }

            let is_connected =
                crate::state::IS_CONNECTED_TO_MASTER.load(core::sync::atomic::Ordering::Relaxed);

            let snapshot = (game, is_connected);
            let changed = last_snapshot.as_ref() != Some(&snapshot);

            if changed && now.duration_since(last_draw) >= std::time::Duration::from_millis(100) {
                last_draw = now;
                last_snapshot = Some(snapshot);
                print_terminal_scoreboard(&game, is_connected);
            }

            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    }
}

#[cfg(feature = "linux")]
fn print_terminal_scoreboard(game: &basketball::Game, is_connected: bool) {
    use std::fmt::Write as _;
    use std::io::Write as _;

    let reg = game.regulation_time();
    let time_str = if reg.minutes == 0 {
        format!("{:02}:{:02}", reg.seconds, reg.centiseconds)
    } else {
        format!("{:02}:{:02}", reg.minutes, reg.seconds)
    };
    let clk = game.clock_time();
    let shot_str = format!("{:02}:{:02}", clk.seconds, clk.centiseconds);

    let scores = game.scores();
    let discipline_str = match game.discipline() {
        basketball::Discipline::Fiba5v5 => "FIBA 5v5",
        basketball::Discipline::Fiba3v3 => "FIBA 3v3",
        _ => "FIBA",
    };
    let quarter_str = match game.quarter() {
        basketball::Quarter::None => "3x3",
        basketball::Quarter::Q1 => "Q1",
        basketball::Quarter::Q2 => "Q2",
        basketball::Quarter::Q3 => "Q3",
        basketball::Quarter::Q4 => "Q4",
        _ => "Q?",
    };
    let state_str = match game.state() {
        basketball::State::Running => "RUNNING",
        basketball::State::Paused => "PAUSED",
        basketball::State::QuarterEnd => "QUARTER END",
        basketball::State::GameEnd => "GAME END",
        _ => "IDLE",
    };
    let master_str = if is_connected {
        "CONNECTED"
    } else {
        "DISCONNECTED"
    };

    let mut out = String::with_capacity(256);
    out.push_str("\x1B[2J\x1B[H");
    let _ = writeln!(out, "=== SCOREBOARD WORKER (LINUX) ===");
    let _ = writeln!(out, "Master : {master_str}");
    let _ = writeln!(
        out,
        "Game   : #{} | {} | {} | {}",
        game.id(),
        discipline_str,
        quarter_str,
        state_str
    );
    let _ = writeln!(
        out,
        "Score  : HOME {:>3} : {:<3} AWAY",
        scores.home, scores.away
    );
    let _ = writeln!(out, "Clock  : {} | Shot: {}", time_str, shot_str);

    let mut stdout = std::io::stdout().lock();
    let _ = stdout.write_all(out.as_bytes());
    let _ = stdout.flush();
}
