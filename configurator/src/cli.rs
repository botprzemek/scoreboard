use crate::commands::{
    AttachArgs, AttachCommand, CreateArgs, CreateCommand, FlashArgs, FlashCommand, PeekArgs,
    PeekCommand, RebootArgs, RebootCommand, VerifyArgs, VerifyCommand, ViewArgs, ViewCommand,
    WriteArgs, WriteCommand,
};
use config::ConfigError;

use clap::{Parser, Subcommand};

#[derive(Subcommand)]
pub enum Commands {
    Create(CreateArgs),
    Verify(VerifyArgs),
    Peek(PeekArgs),
    View(ViewArgs),
    Write(WriteArgs),
    Flash(FlashArgs),
    Reboot(RebootArgs),
    Attach(AttachArgs),
}

impl Commands {
    pub fn run(self) -> Result<(), ConfigError> {
        use Commands::*;

        match self {
            Create(args) => CreateCommand::handle(&args),
            Verify(args) => VerifyCommand::handle(&args),
            Peek(args) => PeekCommand::handle(&args),
            View(args) => ViewCommand::handle(&args),
            Write(args) => WriteCommand::handle(&args),
            Flash(args) => FlashCommand::handle(&args),
            Reboot(args) => RebootCommand::handle(&args),
            Attach(args) => AttachCommand::handle(&args),
        }
    }
}

#[derive(Parser)]
#[command(name = "configurator")]
#[command(about = "RP235x device configuration tool")]
#[command(
    long_about = "Creates, verifies, views, and flashes encrypted RP235x device configurations."
)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

impl Cli {
    pub fn run(self) {
        let error = match self.command.run() {
            Ok(()) => return,
            Err(error) => error,
        };

        println!("Error: {error}");
    }
}
