use std::io;
use std::time::Duration;

use crossterm::cursor::{Hide, Show};
use crossterm::event::{self, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use tui::Terminal;
use tui::backend::{Backend, CrosstermBackend};
use tui::layout::{Constraint, Direction, Layout, Rect};
use tui::style::{Color, Modifier, Style};
use tui::text::{Span, Spans};
use tui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::setup_ui::{SetupWizardAnswers, SetupWizardDefaults};

#[derive(Debug)]
pub enum SetupTuiError {
    Init,
    Runtime(io::Error),
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SetupStep {
    Enable,
    MemoryCap,
    Samples,
    StartupGrace,
    Cooldown,
    Review,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LayoutMode {
    Wide,
    Stacked,
    TooSmall,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InputBuffer {
    text: String,
    replace_on_digit: bool,
}

impl InputBuffer {
    fn new(value: impl ToString) -> Self {
        Self {
            text: value.to_string(),
            replace_on_digit: true,
        }
    }

    fn set_value(&mut self, value: impl ToString) {
        self.text = value.to_string();
        self.replace_on_digit = true;
    }

    fn push_digit(&mut self, digit: char) {
        if self.replace_on_digit {
            self.text.clear();
            self.replace_on_digit = false;
        }
        self.text.push(digit);
    }

    fn backspace(&mut self) {
        self.replace_on_digit = false;
        self.text.pop();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SetupWizardState {
    defaults: SetupWizardDefaults,
    answers: SetupWizardAnswers,
    memory_cap: InputBuffer,
    required_samples: InputBuffer,
    startup_grace: InputBuffer,
    cooldown: InputBuffer,
    step_index: usize,
    error: Option<String>,
}

impl SetupWizardState {
    fn new(defaults: &SetupWizardDefaults) -> Self {
        let answers = SetupWizardAnswers {
            enabled: defaults.enabled,
            memory_cap_gb: defaults.memory_cap_gb,
            required_consecutive_samples: defaults.required_consecutive_samples,
            startup_grace_secs: defaults.startup_grace_secs,
            cooldown_secs: defaults.cooldown_secs,
        };
        let mut state = Self {
            defaults: defaults.clone(),
            answers: answers.clone(),
            memory_cap: InputBuffer::new(defaults.memory_cap_gb),
            required_samples: InputBuffer::new(defaults.required_consecutive_samples),
            startup_grace: InputBuffer::new(defaults.startup_grace_secs),
            cooldown: InputBuffer::new(defaults.cooldown_secs),
            step_index: 0,
            error: None,
        };
        state.prepare_current_step();
        state
    }

    fn visible_steps(&self) -> Vec<SetupStep> {
        let mut steps = vec![SetupStep::Enable];
        if self.answers.enabled {
            steps.extend([
                SetupStep::MemoryCap,
                SetupStep::Samples,
                SetupStep::StartupGrace,
                SetupStep::Cooldown,
            ]);
        }
        steps.push(SetupStep::Review);
        steps
    }

    fn current_step(&self) -> SetupStep {
        self.visible_steps()[self.step_index]
    }

    fn progress_label(&self) -> String {
        let steps = self.visible_steps();
        format!("Step {}/{}", self.step_index + 1, steps.len())
    }

    fn toggle_enabled(&mut self) {
        self.answers.enabled = !self.answers.enabled;
        let max_index = self.visible_steps().len().saturating_sub(1);
        self.step_index = self.step_index.min(max_index);
        self.error = None;
        self.prepare_current_step();
    }

    fn move_previous(&mut self) {
        if self.step_index > 0 {
            self.step_index -= 1;
            self.error = None;
            self.prepare_current_step();
        }
    }

    fn move_next(&mut self) {
        let max_index = self.visible_steps().len().saturating_sub(1);
        self.step_index = (self.step_index + 1).min(max_index);
        self.error = None;
        self.prepare_current_step();
    }

    fn prepare_current_step(&mut self) {
        match self.current_step() {
            SetupStep::MemoryCap => self.memory_cap.replace_on_digit = true,
            SetupStep::Samples => self.required_samples.replace_on_digit = true,
            SetupStep::StartupGrace => self.startup_grace.replace_on_digit = true,
            SetupStep::Cooldown => self.cooldown.replace_on_digit = true,
            SetupStep::Enable | SetupStep::Review => {}
        }
    }

    fn current_question(&self) -> &'static str {
        match self.current_step() {
            SetupStep::Enable => "Enable rust-analyzer memory protection?",
            SetupStep::MemoryCap => "Memory cap in GB",
            SetupStep::Samples => "Consecutive over-limit samples before action",
            SetupStep::StartupGrace => "Startup grace in seconds",
            SetupStep::Cooldown => "Cooldown after remediation in seconds",
            SetupStep::Review => "Review setup changes",
        }
    }

    fn current_description(&self) -> String {
        match self.current_step() {
            SetupStep::Enable => {
                "When enabled, CancerBroker watches rust-analyzer memory and can clean it up after repeated over-limit samples.".to_string()
            }
            SetupStep::MemoryCap => {
                if let Some(detected_ram_gb) = self.defaults.detected_ram_gb {
                    format!(
                        "CancerBroker starts counting rust-analyzer as over the limit after it stays above this amount of RAM. Detected system RAM: {detected_ram_gb} GB."
                    )
                } else {
                    "CancerBroker starts counting rust-analyzer as over the limit after it stays above this amount of RAM.".to_string()
                }
            }
            SetupStep::Samples => {
                "This avoids reacting to a single short memory spike.".to_string()
            }
            SetupStep::StartupGrace => {
                "rust-analyzer often spikes during initial indexing, so counting starts after this delay.".to_string()
            }
            SetupStep::Cooldown => {
                "This prevents repeated remediation loops after rust-analyzer restarts.".to_string()
            }
            SetupStep::Review => "Press Enter to write the Opencode MCP config and rust-analyzer guard settings. The global guardian mode is not changed by this wizard.".to_string(),
        }
    }

    fn current_input(&self) -> Option<&str> {
        match self.current_step() {
            SetupStep::Enable | SetupStep::Review => None,
            SetupStep::MemoryCap => Some(&self.memory_cap.text),
            SetupStep::Samples => Some(&self.required_samples.text),
            SetupStep::StartupGrace => Some(&self.startup_grace.text),
            SetupStep::Cooldown => Some(&self.cooldown.text),
        }
    }

    fn step_titles(&self) -> Vec<&'static str> {
        self.visible_steps()
            .into_iter()
            .map(|step| match step {
                SetupStep::Enable => "Protection",
                SetupStep::MemoryCap => "Memory Cap",
                SetupStep::Samples => "Samples",
                SetupStep::StartupGrace => "Startup Grace",
                SetupStep::Cooldown => "Cooldown",
                SetupStep::Review => "Review",
            })
            .collect()
    }

    fn summary_lines(&self) -> Vec<String> {
        let mut lines = vec![
            format!("enabled: {}", self.answers.enabled),
            format!("memory cap: {} GB", self.answers.memory_cap_gb),
            format!(
                "consecutive samples: {}",
                self.answers.required_consecutive_samples
            ),
            format!("startup grace: {} seconds", self.answers.startup_grace_secs),
            format!("cooldown: {} seconds", self.answers.cooldown_secs),
        ];
        if !self.answers.enabled {
            lines.push("guard disabled: numeric defaults are preserved for future use".to_string());
        }
        lines
    }

    fn handle_digit(&mut self, digit: char) {
        match self.current_step() {
            SetupStep::MemoryCap => self.memory_cap.push_digit(digit),
            SetupStep::Samples => self.required_samples.push_digit(digit),
            SetupStep::StartupGrace => self.startup_grace.push_digit(digit),
            SetupStep::Cooldown => self.cooldown.push_digit(digit),
            SetupStep::Enable | SetupStep::Review => {}
        }
        self.error = None;
    }

    fn handle_backspace(&mut self) {
        match self.current_step() {
            SetupStep::MemoryCap => self.memory_cap.backspace(),
            SetupStep::Samples => self.required_samples.backspace(),
            SetupStep::StartupGrace => self.startup_grace.backspace(),
            SetupStep::Cooldown => self.cooldown.backspace(),
            SetupStep::Enable | SetupStep::Review => self.move_previous(),
        }
        self.error = None;
    }

    fn confirm(&mut self) -> Result<Option<SetupWizardAnswers>, String> {
        let current_step = self.current_step();
        match current_step {
            SetupStep::Enable => {
                self.move_next();
                Ok(None)
            }
            SetupStep::MemoryCap => {
                let value = parse_u64(
                    &self.memory_cap.text,
                    Some(1),
                    self.defaults.detected_ram_gb,
                )?;
                self.answers.memory_cap_gb = value;
                self.memory_cap.set_value(value);
                self.move_next();
                Ok(None)
            }
            SetupStep::Samples => {
                let value = parse_usize(&self.required_samples.text, 1)?;
                self.answers.required_consecutive_samples = value;
                self.required_samples.set_value(value);
                self.move_next();
                Ok(None)
            }
            SetupStep::StartupGrace => {
                let value = parse_u64(&self.startup_grace.text, Some(0), None)?;
                self.answers.startup_grace_secs = value;
                self.startup_grace.set_value(value);
                self.move_next();
                Ok(None)
            }
            SetupStep::Cooldown => {
                let value = parse_u64(&self.cooldown.text, Some(0), None)?;
                if value < self.answers.startup_grace_secs {
                    return Err(format!(
                        "Cooldown must be at least {} seconds so it is not shorter than the startup grace.",
                        self.answers.startup_grace_secs
                    ));
                }
                self.answers.cooldown_secs = value;
                self.cooldown.set_value(value);
                self.move_next();
                Ok(None)
            }
            SetupStep::Review => Ok(Some(self.answers.clone())),
        }
    }
}

struct SetupTerminalSession {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
}

impl SetupTerminalSession {
    fn new() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, Hide)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;
        Ok(Self { terminal })
    }
}

impl Drop for SetupTerminalSession {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), Show, LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

pub fn run_setup_wizard_tui(
    defaults: &SetupWizardDefaults,
) -> Result<SetupWizardAnswers, SetupTuiError> {
    let mut session = SetupTerminalSession::new().map_err(|_| SetupTuiError::Init)?;
    run_event_loop(&mut session.terminal, defaults).map_err(|error| {
        if error.kind() == io::ErrorKind::Interrupted {
            SetupTuiError::Cancelled
        } else {
            SetupTuiError::Runtime(error)
        }
    })
}

fn run_event_loop<B: Backend>(
    terminal: &mut Terminal<B>,
    defaults: &SetupWizardDefaults,
) -> io::Result<SetupWizardAnswers> {
    let mut state = SetupWizardState::new(defaults);
    loop {
        terminal.draw(|frame| draw_setup_wizard(frame, &state))?;

        if !event::poll(Duration::from_millis(250))? {
            continue;
        }

        let Event::Key(key) = event::read()? else {
            continue;
        };

        match key.code {
            KeyCode::Esc => {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "setup wizard cancelled",
                ));
            }
            KeyCode::Up => state.move_previous(),
            KeyCode::Left if state.current_step() == SetupStep::Enable => state.toggle_enabled(),
            KeyCode::Right if state.current_step() == SetupStep::Enable => state.toggle_enabled(),
            KeyCode::Enter => match state.confirm() {
                Ok(Some(answers)) => return Ok(answers),
                Ok(None) => state.error = None,
                Err(message) => state.error = Some(message),
            },
            KeyCode::Backspace => state.handle_backspace(),
            KeyCode::Char(' ') if state.current_step() == SetupStep::Enable => {
                state.toggle_enabled()
            }
            KeyCode::Char('y') | KeyCode::Char('Y')
                if state.current_step() == SetupStep::Enable =>
            {
                state.answers.enabled = true;
                state.error = None;
            }
            KeyCode::Char('n') | KeyCode::Char('N')
                if state.current_step() == SetupStep::Enable =>
            {
                state.answers.enabled = false;
                state.error = None;
            }
            KeyCode::Char(digit) if digit.is_ascii_digit() => state.handle_digit(digit),
            _ => {}
        }
    }
}

fn draw_setup_wizard<B: Backend>(frame: &mut tui::Frame<B>, state: &SetupWizardState) {
    let area = frame.size();
    frame.render_widget(Clear, area);

    let shell = Block::default()
        .borders(Borders::ALL)
        .title("CancerBroker Setup Wizard");
    let inner = shell.inner(area);
    frame.render_widget(shell, area);

    match layout_mode(inner) {
        LayoutMode::TooSmall => {
            render_too_small_panel(frame, inner);
        }
        LayoutMode::Wide | LayoutMode::Stacked => {
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(5),
                    Constraint::Min(14),
                    Constraint::Length(4),
                ])
                .split(inner);

            render_header_panel(frame, layout[0], state);

            let body_layout = match layout_mode(inner) {
                LayoutMode::Wide => Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(62), Constraint::Percentage(38)])
                    .split(layout[1]),
                LayoutMode::Stacked => Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
                    .split(layout[1]),
                LayoutMode::TooSmall => unreachable!(),
            };

            render_current_step(frame, body_layout[0], state);
            render_summary_panel(frame, body_layout[1], state);
            render_controls_panel(frame, layout[2], state);
        }
    }
}

fn layout_mode(area: Rect) -> LayoutMode {
    if area.width < 72 || area.height < 18 {
        LayoutMode::TooSmall
    } else if area.width >= 110 {
        LayoutMode::Wide
    } else {
        LayoutMode::Stacked
    }
}

fn render_header_panel<B: Backend>(
    frame: &mut tui::Frame<B>,
    area: Rect,
    state: &SetupWizardState,
) {
    let details = match state.defaults.detected_ram_gb {
        Some(detected_ram_gb) => format!(
            "Detected RAM: {detected_ram_gb} GB | {} | writes Opencode MCP + rust-analyzer guard config",
            state.progress_label()
        ),
        None => format!(
            "Detected RAM unavailable | {} | writes Opencode MCP + rust-analyzer guard config",
            state.progress_label()
        ),
    };
    let panel = Paragraph::new(vec![
        Spans::from(Span::styled(
            state.current_question().to_string(),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Spans::from(details),
    ])
    .block(Block::default().borders(Borders::ALL).title("Header"))
    .wrap(Wrap { trim: true });
    frame.render_widget(panel, area);
}

fn render_current_step<B: Backend>(
    frame: &mut tui::Frame<B>,
    area: Rect,
    state: &SetupWizardState,
) {
    let outer = Block::default().borders(Borders::ALL).title("Step");
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(4),
            Constraint::Length(4),
            Constraint::Length(3),
        ])
        .split(inner);

    let title_panel = Paragraph::new(vec![Spans::from(vec![
        Span::styled(
            state.current_question().to_string(),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  |  "),
        Span::raw(state.progress_label()),
    ])])
    .block(Block::default().borders(Borders::ALL).title("Title"))
    .wrap(Wrap { trim: true });
    frame.render_widget(title_panel, layout[0]);

    let description_panel = Paragraph::new(state.current_description())
        .block(Block::default().borders(Borders::ALL).title("Description"))
        .wrap(Wrap { trim: true });
    frame.render_widget(description_panel, layout[1]);

    let input_panel = render_input_panel(state);
    frame.render_widget(input_panel, layout[2]);

    let footer_lines = match &state.error {
        Some(error) => vec![Spans::from(Span::styled(
            error.clone(),
            Style::default().fg(Color::Red),
        ))],
        None => vec![Spans::from(help_text_for_step(state.current_step()))],
    };
    let footer_title = if state.error.is_some() {
        "Validation"
    } else {
        "Help"
    };
    let footer_panel = Paragraph::new(footer_lines)
        .block(Block::default().borders(Borders::ALL).title(footer_title))
        .wrap(Wrap { trim: true });
    frame.render_widget(footer_panel, layout[3]);
}

fn render_input_panel(state: &SetupWizardState) -> Paragraph<'static> {
    let mut lines = vec![Spans::from(Span::styled(
        state.step_titles()[state.step_index].to_string(),
        Style::default().add_modifier(Modifier::BOLD),
    ))];
    lines.push(Spans::from(""));

    match state.current_step() {
        SetupStep::Enable => {
            let enabled_style = if state.answers.enabled {
                Style::default().fg(Color::Black).bg(Color::Green)
            } else {
                Style::default().fg(Color::Green)
            };
            let disabled_style = if state.answers.enabled {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().fg(Color::Black).bg(Color::Yellow)
            };
            lines.push(Spans::from(vec![
                Span::styled(" Enabled ", enabled_style),
                Span::raw("   "),
                Span::styled(" Disabled ", disabled_style),
            ]));
            lines.push(Spans::from("Use Left/Right/Space to toggle."));
        }
        SetupStep::Review => {
            for line in state.summary_lines() {
                lines.push(Spans::from(line));
            }
        }
        SetupStep::MemoryCap
        | SetupStep::Samples
        | SetupStep::StartupGrace
        | SetupStep::Cooldown => {
            lines.push(Spans::from(vec![
                Span::styled("Value: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(
                    state.current_input().unwrap_or_default().to_string(),
                    Style::default().fg(Color::Yellow),
                ),
            ]));
            lines.push(Spans::from("Type digits to replace the current value."));
        }
    }

    Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Input"))
        .wrap(Wrap { trim: true })
}

fn render_summary_panel<B: Backend>(
    frame: &mut tui::Frame<B>,
    area: Rect,
    state: &SetupWizardState,
) {
    let outer = Block::default().borders(Borders::ALL).title("Summary");
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(6),
            Constraint::Length(5),
        ])
        .split(inner);

    let path_panel = Paragraph::new(state.step_titles().join(" -> "))
        .block(Block::default().borders(Borders::ALL).title("Path"))
        .wrap(Wrap { trim: true });
    frame.render_widget(path_panel, layout[0]);

    let answers_panel = Paragraph::new(
        state
            .summary_lines()
            .into_iter()
            .map(Spans::from)
            .collect::<Vec<_>>(),
    )
    .block(Block::default().borders(Borders::ALL).title("Answers"))
    .wrap(Wrap { trim: true });
    frame.render_widget(answers_panel, layout[1]);

    let note_panel = Paragraph::new(vec![
        Spans::from("The wizard updates the Opencode MCP config and rust-analyzer memory-guard config only."),
        Spans::from("Global guardian mode is preserved from your existing configuration."),
    ])
    .block(Block::default().borders(Borders::ALL).title("Notes"))
    .wrap(Wrap { trim: true });
    frame.render_widget(note_panel, layout[2]);
}

fn render_controls_panel<B: Backend>(
    frame: &mut tui::Frame<B>,
    area: Rect,
    state: &SetupWizardState,
) {
    let mut lines = vec![Spans::from(vec![
        Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" confirm  "),
        Span::styled("Up", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" previous  "),
        Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" cancel"),
    ])];

    if state.current_step() == SetupStep::Enable {
        lines.push(Spans::from(vec![
            Span::styled(
                "Left/Right/Space",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(" toggle enabled"),
        ]));
    } else if state.current_step() != SetupStep::Review {
        lines.push(Spans::from(vec![
            Span::styled("Digits", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" replace value  "),
            Span::styled("Backspace", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" erase/go back"),
        ]));
    }

    let controls = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Controls"))
        .wrap(Wrap { trim: true });
    frame.render_widget(controls, area);
}

fn render_too_small_panel<B: Backend>(frame: &mut tui::Frame<B>, area: Rect) {
    let warning = Paragraph::new(vec![
        Spans::from(Span::styled(
            "Terminal too small for the setup wizard.",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Spans::from("Resize the terminal and run `cancerbroker setup` again."),
        Spans::from(
            "Non-interactive setup remains available with `cancerbroker setup --non-interactive`.",
        ),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Resize Required"),
    )
    .wrap(Wrap { trim: true });
    frame.render_widget(warning, area);
}

fn help_text_for_step(step: SetupStep) -> &'static str {
    match step {
        SetupStep::Enable => {
            "Choose whether CancerBroker should actively guard rust-analyzer memory."
        }
        SetupStep::MemoryCap => "Enter the memory cap in whole gigabytes.",
        SetupStep::Samples => "Choose how many over-limit samples are required before action.",
        SetupStep::StartupGrace => "Choose how long startup spikes should be ignored.",
        SetupStep::Cooldown => "Choose the cooldown after remediation before counting again.",
        SetupStep::Review => "Press Enter to write both config files with the shown values.",
    }
}

fn parse_u64(raw: &str, min: Option<u64>, max: Option<u64>) -> Result<u64, String> {
    let value = raw
        .parse::<u64>()
        .map_err(|_| build_range_message(min, max, "Enter a whole-number value"))?;
    if let Some(minimum) = min
        && value < minimum
    {
        return Err(build_range_message(min, max, "Value is too small"));
    }
    if let Some(maximum) = max
        && value > maximum
    {
        return Err(build_range_message(min, max, "Value is too large"));
    }
    Ok(value)
}

fn parse_usize(raw: &str, min: usize) -> Result<usize, String> {
    let value = raw
        .parse::<usize>()
        .map_err(|_| format!("Enter a whole number greater than or equal to {min}."))?;
    if value < min {
        return Err(format!(
            "Enter a whole number greater than or equal to {min}."
        ));
    }
    Ok(value)
}

fn build_range_message(min: Option<u64>, max: Option<u64>, prefix: &str) -> String {
    match (min, max) {
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
    use super::{SetupStep, SetupWizardState};
    use crate::setup_ui::SetupWizardDefaults;

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
    fn state_skips_numeric_steps_when_guard_is_disabled() {
        let mut state = SetupWizardState::new(&defaults());
        state.toggle_enabled();
        assert_eq!(
            state.visible_steps(),
            vec![SetupStep::Enable, SetupStep::Review]
        );
        assert!(state.confirm().expect("enable step").is_none());
        assert_eq!(state.current_step(), SetupStep::Review);
    }

    #[test]
    fn state_validates_cooldown_against_startup_grace() {
        let mut state = SetupWizardState::new(&defaults());
        state.step_index = 4;
        state.prepare_current_step();
        state.answers.startup_grace_secs = 300;
        state.cooldown.set_value(120);

        let error = state.confirm().expect_err("cooldown should fail");
        assert!(error.contains("Cooldown must be at least 300 seconds"));
    }

    #[test]
    fn review_returns_answers() {
        let mut state = SetupWizardState::new(&defaults());
        state.step_index = state.visible_steps().len() - 1;
        let answers = state
            .confirm()
            .expect("review should succeed")
            .expect("review should return answers");
        assert_eq!(answers.memory_cap_gb, 2);
        assert!(answers.enabled);
    }
}
