use clap::Parser;
use color_eyre::eyre::Result;

use cancerbroker::cli::{Cli, run};

fn main() -> Result<()> {
    color_eyre::install()?;
    let cli = Cli::parse();
    run(cli)
}
