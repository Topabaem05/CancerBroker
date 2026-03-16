use std::env;
use std::process::ExitCode;

use notify_rust::Notification;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args();
    args.next();

    let mut summary = None;
    let mut body = None;

    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--summary" => {
                summary = Some(args.next().ok_or_else(|| usage().to_string())?);
            }
            "--body" => {
                body = Some(args.next().ok_or_else(|| usage().to_string())?);
            }
            _ => return Err(usage().to_string()),
        }
    }

    let summary = summary.ok_or_else(|| usage().to_string())?;
    let body = body.ok_or_else(|| usage().to_string())?;

    Notification::new()
        .summary(&summary)
        .body(&body)
        .show()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn usage() -> &'static str {
    "usage: cancerbroker-notify-helper --summary <text> --body <text>"
}

#[cfg(test)]
mod tests {
    use super::usage;

    #[test]
    fn usage_string_matches_expected_shape() {
        assert_eq!(
            usage(),
            "usage: cancerbroker-notify-helper --summary <text> --body <text>"
        );
    }
}
