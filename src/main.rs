use clap::Parser;
use color_eyre::eyre::Result;

use opencode_guardian::cli::{run, Cli};

fn main() -> Result<()> {
    color_eyre::install()?;
    let cli = Cli::parse();
    run(cli)
}
