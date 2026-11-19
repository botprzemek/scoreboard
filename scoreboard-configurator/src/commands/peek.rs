use clap::Args;
use scoreboard_config::flash::FlashReader;
use scoreboard_config::print::Print;
use scoreboard_config::{Config, ConfigError};

#[derive(Args)]
#[command(about = "Read configuration from target flash memory", long_about = None)]
pub struct PeekArgs {
    #[arg(short, long, default_value = "true")]
    pub verbose: bool,
}

pub struct PeekCommand;

impl PeekCommand {
    pub fn handle(args: &PeekArgs) -> Result<(), ConfigError> {
        let config = Config::read()?;

        if args.verbose {
            config.print()?;
        }

        Ok(())
    }
}
