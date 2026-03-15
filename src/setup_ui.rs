use std::fmt::Display;
use std::io::{self, BufRead, Write};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupWizardDefaults {
    pub detected_ram_gb: Option<u64>,
    pub enabled: bool,
    pub memory_cap_gb: u64,
    pub required_consecutive_samples: usize,
    pub startup_grace_secs: u64,
    pub cooldown_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupWizardAnswers {
    pub enabled: bool,
    pub memory_cap_gb: u64,
    pub required_consecutive_samples: usize,
    pub startup_grace_secs: u64,
    pub cooldown_secs: u64,
}

#[derive(Debug, Clone, Copy)]
struct U64Bounds {
    min: Option<u64>,
    max: Option<u64>,
}

pub fn run_setup_wizard<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    defaults: &SetupWizardDefaults,
) -> io::Result<SetupWizardAnswers> {
    writeln!(writer, "CancerBroker setup will:")?;
    writeln!(writer, "- register the local MCP server in OpenCode")?;
    writeln!(
        writer,
        "- configure the rust-analyzer memory guard for this machine"
    )?;
    match defaults.detected_ram_gb {
        Some(total_ram_gb) => writeln!(
            writer,
            "Detected system RAM: {total_ram_gb} GB. Press Enter to accept the default shown in brackets."
        )?,
        None => writeln!(
            writer,
            "Could not detect total system RAM. Press Enter to accept the conservative default shown in brackets."
        )?,
    }

    let enabled = prompt_bool(
        reader,
        writer,
        "Enable rust-analyzer memory protection",
        defaults.enabled,
        "When enabled, CancerBroker watches rust-analyzer memory and can clean it up after repeated over-limit samples.",
    )?;

    if !enabled {
        writeln!(
            writer,
            "rust-analyzer memory protection is disabled. Keeping the remaining defaults for future use."
        )?;
        return Ok(SetupWizardAnswers {
            enabled,
            memory_cap_gb: defaults.memory_cap_gb,
            required_consecutive_samples: defaults.required_consecutive_samples,
            startup_grace_secs: defaults.startup_grace_secs,
            cooldown_secs: defaults.cooldown_secs,
        });
    }

    let memory_cap_gb = prompt_u64(
        reader,
        writer,
        "Memory cap in GB",
        defaults.memory_cap_gb,
        U64Bounds {
            min: Some(1),
            max: defaults.detected_ram_gb,
        },
        "CancerBroker starts counting rust-analyzer as over the limit after it stays above this amount of RAM.",
    )?;
    let required_consecutive_samples = prompt_usize(
        reader,
        writer,
        "Consecutive over-limit samples before action",
        defaults.required_consecutive_samples,
        1,
        "This avoids reacting to a single short memory spike.",
    )?;
    let startup_grace_secs = prompt_u64(
        reader,
        writer,
        "Startup grace in seconds",
        defaults.startup_grace_secs,
        U64Bounds {
            min: Some(0),
            max: None,
        },
        "rust-analyzer often spikes during initial indexing, so counting starts after this delay.",
    )?;
    let cooldown_secs = prompt_u64_with_validator(
        reader,
        writer,
        "Cooldown after remediation in seconds",
        defaults.cooldown_secs,
        U64Bounds {
            min: Some(0),
            max: None,
        },
        "This prevents repeated remediation loops after rust-analyzer restarts.",
        |value| {
            if value < startup_grace_secs {
                Some(format!(
                    "Cooldown must be at least {startup_grace_secs} seconds so it is not shorter than the startup grace."
                ))
            } else {
                None
            }
        },
    )?;

    writeln!(writer, "Summary:")?;
    writeln!(writer, "- enabled: {enabled}")?;
    writeln!(writer, "- memory cap: {memory_cap_gb} GB")?;
    writeln!(
        writer,
        "- consecutive samples: {required_consecutive_samples}"
    )?;
    writeln!(writer, "- startup grace: {startup_grace_secs} seconds")?;
    writeln!(writer, "- cooldown: {cooldown_secs} seconds")?;
    writeln!(
        writer,
        "Note: the global guardian mode still controls whether this guard only observes or also remediates."
    )?;

    Ok(SetupWizardAnswers {
        enabled,
        memory_cap_gb,
        required_consecutive_samples,
        startup_grace_secs,
        cooldown_secs,
    })
}

fn prompt_bool<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    label: &str,
    default: bool,
    explanation: &str,
) -> io::Result<bool> {
    let default_hint = if default { "Y/n" } else { "y/N" };
    loop {
        let raw = prompt_line(
            reader,
            writer,
            &format!("{label}? [{default_hint}]"),
            explanation,
        )?;
        if raw.is_empty() {
            return Ok(default);
        }
        match raw.to_ascii_lowercase().as_str() {
            "y" | "yes" => return Ok(true),
            "n" | "no" => return Ok(false),
            _ => writeln!(
                writer,
                "Please answer yes or no, or press Enter to accept the default."
            )?,
        }
    }
}

fn prompt_usize<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    label: &str,
    default: usize,
    min: usize,
    explanation: &str,
) -> io::Result<usize> {
    prompt_parse(reader, writer, label, default, explanation, move |raw| {
        let value = raw
            .parse::<usize>()
            .map_err(|_| format!("Enter a whole number greater than or equal to {min}."))?;
        if value < min {
            return Err(format!(
                "Enter a whole number greater than or equal to {min}."
            ));
        }
        Ok(value)
    })
}

fn prompt_u64<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    label: &str,
    default: u64,
    bounds: U64Bounds,
    explanation: &str,
) -> io::Result<u64> {
    prompt_u64_with_validator(reader, writer, label, default, bounds, explanation, |_| {
        None
    })
}

fn prompt_u64_with_validator<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    label: &str,
    default: u64,
    bounds: U64Bounds,
    explanation: &str,
    validate: impl Fn(u64) -> Option<String>,
) -> io::Result<u64> {
    prompt_parse(reader, writer, label, default, explanation, move |raw| {
        let value = raw
            .parse::<u64>()
            .map_err(|_| build_range_message(bounds, "Enter a whole-number value"))?;
        if let Some(minimum) = bounds.min
            && value < minimum
        {
            return Err(build_range_message(bounds, "Value is too small"));
        }
        if let Some(maximum) = bounds.max
            && value > maximum
        {
            return Err(build_range_message(bounds, "Value is too large"));
        }
        if let Some(message) = validate(value) {
            return Err(message);
        }
        Ok(value)
    })
}

fn prompt_parse<T, R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    label: &str,
    default: T,
    explanation: &str,
    parse: impl Fn(&str) -> Result<T, String>,
) -> io::Result<T>
where
    T: Copy + Display,
{
    loop {
        let raw = prompt_line(reader, writer, &format!("{label} [{default}]"), explanation)?;
        if raw.is_empty() {
            return Ok(default);
        }
        match parse(&raw) {
            Ok(value) => return Ok(value),
            Err(message) => writeln!(writer, "{message}")?,
        }
    }
}

fn prompt_line<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    prompt: &str,
    explanation: &str,
) -> io::Result<String> {
    writeln!(writer)?;
    writeln!(writer, "{prompt}")?;
    writeln!(writer, "  {explanation}")?;
    write!(writer, "> ")?;
    writer.flush()?;

    let mut line = String::new();
    reader.read_line(&mut line)?;
    Ok(line.trim().to_string())
}

fn build_range_message(bounds: U64Bounds, prefix: &str) -> String {
    match (bounds.min, bounds.max) {
        (Some(minimum), Some(maximum)) => {
            format!("{prefix}. Enter a whole number between {minimum} and {maximum}.")
        }
        (Some(minimum), None) => {
            format!("{prefix}. Enter a whole number greater than or equal to {minimum}.")
        }
        (None, Some(maximum)) => {
            format!("{prefix}. Enter a whole number less than or equal to {maximum}.")
        }
        (None, None) => format!("{prefix}. Enter a whole number."),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::{SetupWizardAnswers, SetupWizardDefaults, run_setup_wizard};

    fn defaults() -> SetupWizardDefaults {
        SetupWizardDefaults {
            detected_ram_gb: Some(16),
            enabled: true,
            memory_cap_gb: 2,
            required_consecutive_samples: 2,
            startup_grace_secs: 180,
            cooldown_secs: 900,
        }
    }

    #[test]
    fn wizard_accepts_enter_for_all_defaults() {
        let mut input = Cursor::new("\n\n\n\n\n");
        let mut output = Vec::new();

        let answers =
            run_setup_wizard(&mut input, &mut output, &defaults()).expect("wizard should succeed");

        assert_eq!(
            answers,
            SetupWizardAnswers {
                enabled: true,
                memory_cap_gb: 2,
                required_consecutive_samples: 2,
                startup_grace_secs: 180,
                cooldown_secs: 900,
            }
        );
    }

    #[test]
    fn wizard_retries_invalid_integer_input() {
        let mut input = Cursor::new("\nabc\n4\n2\n180\n900\n");
        let mut output = Vec::new();

        let answers =
            run_setup_wizard(&mut input, &mut output, &defaults()).expect("wizard should succeed");

        assert_eq!(answers.memory_cap_gb, 4);
        assert!(
            String::from_utf8(output)
                .expect("utf8")
                .contains("Enter a whole-number value")
        );
    }

    #[test]
    fn wizard_parses_yes_no_answers() {
        let mut input = Cursor::new("no\n");
        let mut output = Vec::new();

        let answers =
            run_setup_wizard(&mut input, &mut output, &defaults()).expect("wizard should succeed");

        assert!(!answers.enabled);
        assert_eq!(answers.memory_cap_gb, 2);
    }

    #[test]
    fn wizard_rejects_cooldown_shorter_than_startup_grace() {
        let mut input = Cursor::new("\n\n\n300\n120\n600\n");
        let mut output = Vec::new();

        let answers =
            run_setup_wizard(&mut input, &mut output, &defaults()).expect("wizard should succeed");

        assert_eq!(answers.startup_grace_secs, 300);
        assert_eq!(answers.cooldown_secs, 600);
        assert!(
            String::from_utf8(output)
                .expect("utf8")
                .contains("Cooldown must be at least 300 seconds")
        );
    }
}
