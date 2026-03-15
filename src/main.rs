use std::ffi::OsStr;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use clap::Parser;
use color_eyre::eyre::{Result, WrapErr};

use cancerbroker::cli::{Cli, run};
use cancerbroker::config::load_config;
use cancerbroker::daemon::run_rust_analyzer_memory_guard_once;

const RA_GUARD_HELPER_BINARY: &str = "cancerbroker-ra-guard";

fn render_ra_guard_output(
    output: &cancerbroker::daemon::MemoryGuardOutput,
    json: bool,
) -> Result<String> {
    if json {
        Ok(serde_json::to_string(output)?)
    } else {
        Ok(format!(
            "rust_analyzer_memory_candidates={} rust_analyzer_memory_remediations={}",
            output.rust_analyzer_memory_candidates, output.rust_analyzer_memory_remediations
        ))
    }
}

fn run_ra_guard_inline(config_path: &Path, json: bool) -> Result<()> {
    let config = load_config(config_path).wrap_err("config load failure")?;
    let output =
        run_rust_analyzer_memory_guard_once(&config).wrap_err("rust-analyzer guard run failure")?;
    println!("{}", render_ra_guard_output(&output, json)?);
    Ok(())
}

fn helper_binary_name() -> &'static str {
    #[cfg(windows)]
    {
        "cancerbroker-ra-guard.exe"
    }

    #[cfg(not(windows))]
    {
        RA_GUARD_HELPER_BINARY
    }
}

fn resolve_helper_binary_path() -> Result<PathBuf> {
    let current = std::env::current_exe().wrap_err("failed to resolve current executable")?;
    let parent = current
        .parent()
        .ok_or_else(|| color_eyre::eyre::eyre!("failed to resolve executable directory"))?;
    Ok(parent.join(helper_binary_name()))
}

fn delegate_ra_guard_to_helper(config_path: &Path, json: bool) -> Result<()> {
    let helper_path = resolve_helper_binary_path()?;

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;

        let mut command = Command::new(&helper_path);
        command.arg("--config").arg(config_path);
        if json {
            command.arg("--json");
        }

        let error = command.exec();
        return Err(error.into());
    }

    #[cfg(not(unix))]
    {
        let mut command = Command::new(&helper_path);
        command.arg("--config").arg(config_path);
        if json {
            command.arg("--json");
        }

        let status = command
            .status()
            .wrap_err("failed to execute ra-guard helper")?;

        if status.success() {
            Ok(())
        } else {
            Err(color_eyre::eyre::eyre!(
                "ra-guard helper exited with status {status}"
            ))
        }
    }
}

fn run_ra_guard_fast_path(config_path: PathBuf, json: bool) -> Result<()> {
    if delegate_ra_guard_to_helper(&config_path, json).is_ok() {
        return Ok(());
    }

    run_ra_guard_inline(&config_path, json)
}

fn try_ra_guard_fast_path() -> Option<Result<()>> {
    let mut args = std::env::args_os();
    args.next()?;

    let first = args.next()?;
    if first != OsStr::new("--config") {
        return None;
    }

    let config_path = PathBuf::from(args.next()?);
    let command = args.next()?;
    if command != OsStr::new("ra-guard") {
        return None;
    }

    let json = match args.next() {
        None => false,
        Some(flag) if flag == OsStr::new("--json") => true,
        Some(_) => return None,
    };

    if args.next().is_some() {
        return None;
    }

    Some(run_ra_guard_fast_path(config_path, json))
}

fn main() -> Result<()> {
    if let Some(result) = try_ra_guard_fast_path() {
        return result;
    }

    color_eyre::install()?;
    let cli = Cli::parse();
    run(cli)
}
