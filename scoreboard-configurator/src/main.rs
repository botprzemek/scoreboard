use scoreboard_configurator::Cli;

use clap::Parser;

fn main() {
    Cli::parse().run();
}
