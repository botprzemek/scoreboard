use crate::common::{DisplayRunner, DisplayTarget};
use crate::linux::platform::{CORE1_READY_SIGNAL, LinuxClock};

use scoreboard_basketball::{Discipline, Game, Quarter, ScoreRead, State};
use std::fmt::Write as _;
use std::io::Write as _;
use std::time::{Duration, Instant};

pub struct TerminalDisplay {
    last_draw: Instant,
}

impl Default for TerminalDisplay {
    fn default() -> Self {
        Self::new()
    }
}

impl TerminalDisplay {
    pub fn new() -> Self {
        Self {
            last_draw: Instant::now() - Duration::from_secs(1),
        }
    }
}

impl DisplayTarget for TerminalDisplay {
    fn update(&mut self, game: &Game, is_connected: bool) -> bool {
        let now = Instant::now();
        if game.state() == State::InProgress
            && now.duration_since(self.last_draw) < Duration::from_millis(100)
        {
            return false;
        }
        self.last_draw = now;
        print_terminal_scoreboard(game, is_connected);
        true
    }

    fn refresh(&mut self, _is_connected: bool) {
        std::thread::sleep(Duration::from_millis(25));
    }
}

pub struct Core1;

impl Core1 {
    pub fn entry() {
        println!("🚀 [Core 1 Thread] Text Scoreboard Runner started");
        CORE1_READY_SIGNAL.signal();

        let mut runner =
            DisplayRunner::new(TerminalDisplay::new(), LinuxClock::new()).with_persisted_tick(true);

        loop {
            runner.step();
        }
    }
}

fn print_terminal_scoreboard(game: &Game, is_connected: bool) {
    let reg = game.regulation_time();
    let time_str = if reg.minutes == 0 {
        format!("{:02}:{:02}", reg.seconds, reg.centiseconds)
    } else {
        format!("{:02}:{:02}", reg.minutes, reg.seconds)
    };
    let clk = game.clock_time();
    let shot_str = format!("{:02}:{:02}", clk.seconds, clk.centiseconds);

    let (home_score, away_score) = game.scores();
    let discipline_str = match game.discipline() {
        Discipline::Fiba5v5 => "FIBA 5v5",
        Discipline::Fiba3v3 => "FIBA 3v3",
        _ => "FIBA",
    };
    let quarter_str = match game.quarter() {
        Quarter::None => "3x3",
        Quarter::Q1 => "Q1",
        Quarter::Q2 => "Q2",
        Quarter::Q3 => "Q3",
        Quarter::Q4 => "Q4",
        _ => "Q?",
    };
    let state_str = match game.state() {
        State::Scheduled => "SCHEDULED",
        State::QuarterStart => "QUARTER START",
        State::InProgress => "IN PROGRESS",
        State::Paused => "PAUSED",
        State::QuarterEnd => "QUARTER END",
        State::Final => "FINAL",
        _ => "UNKNOWN",
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
        home_score, away_score
    );
    let _ = writeln!(out, "Clock  : {} | Shot: {}", time_str, shot_str);

    let mut stdout = std::io::stdout().lock();
    let _ = stdout.write_all(out.as_bytes());
    let _ = stdout.flush();
}
