use std::io::{self, Stdout};
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Terminal,
};

use crate::probe::{format_report, run_probe_suite, ProbeOptions, ProbeReport};
use crate::storage::{
    compare_reports, format_comparison, format_history, latest_run, list_runs, save_run,
    RunComparison, StoredRun,
};
use crate::version;

pub async fn run_tui(options: ProbeOptions, interval_seconds: u64) -> Result<()> {
    let mut terminal = setup_terminal()?;
    let mut state = TuiState::new(options.target.clone(), interval_seconds);

    loop {
        state.status = "Collecting measurements...".to_string();
        draw(&mut terminal, &state)?;

        match run_probe_suite(&options).await {
            Ok(report) => {
                let comparison = latest_run(&options.target)?
                    .map(|previous| compare_reports(&previous.report, &report));
                save_run(&report)?;
                state.report = Some(report);
                state.comparison = comparison;
                state.history = list_runs(Some(&options.target), 5)?;
                state.error = None;
                state.refresh_count += 1;
                state.status = "Press q to quit, r to refresh now.".to_string();
            }
            Err(error) => {
                state.error = Some(error.to_string());
                state.status = "Probe run failed. Press r to retry or q to quit.".to_string();
            }
        }

        let wait_until = Instant::now() + Duration::from_secs(interval_seconds);
        loop {
            draw(&mut terminal, &state)?;

            if event::poll(Duration::from_millis(200))? {
                let input = event::read()?;
                if let Event::Key(key) = input {
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Char('q') => {
                                restore_terminal(&mut terminal)?;
                                return Ok(());
                            }
                            KeyCode::Char('r') => break,
                            _ => {}
                        }
                    }
                }
            }

            if Instant::now() >= wait_until {
                break;
            }
        }
    }
}

struct TuiState {
    target: String,
    interval_seconds: u64,
    refresh_count: u64,
    status: String,
    report: Option<ProbeReport>,
    comparison: Option<RunComparison>,
    history: Vec<StoredRun>,
    error: Option<String>,
}

impl TuiState {
    fn new(target: String, interval_seconds: u64) -> Self {
        Self {
            target,
            interval_seconds,
            refresh_count: 0,
            status: "Starting PantheonProbe TUI...".to_string(),
            report: None,
            comparison: None,
            history: Vec::new(),
            error: None,
        }
    }
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn draw(terminal: &mut Terminal<CrosstermBackend<Stdout>>, state: &TuiState) -> Result<()> {
    terminal.draw(|frame| {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(10),
                Constraint::Length(3),
            ])
            .split(frame.size());

        let header = Paragraph::new(vec![
            Line::from(Span::styled(
                version::short_banner(&state.target),
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Line::from(format!(
                "Refresh every {}s | completed runs: {}",
                state.interval_seconds, state.refresh_count
            )),
        ])
        .block(Block::default().borders(Borders::ALL).title("Overview"));
        frame.render_widget(header, layout[0]);

        let body_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(68), Constraint::Percentage(32)])
            .split(layout[1]);

        let body_text = match (&state.report, &state.error) {
            (_, Some(error)) => format!("Probe error\n\n{error}"),
            (Some(report), None) => format_report(report),
            (None, None) => "Waiting for first probe run...".to_string(),
        };

        let body = Paragraph::new(body_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Latest report"),
            )
            .wrap(Wrap { trim: false });
        frame.render_widget(body, body_layout[0]);

        let side_text = match (&state.comparison, state.history.is_empty()) {
            (Some(comparison), false) => {
                format!(
                    "{}\n\nRecent runs\n{}",
                    format_comparison(comparison),
                    format_history(&state.history)
                )
            }
            (Some(comparison), true) => format_comparison(comparison),
            (None, false) => format!("Recent runs\n{}", format_history(&state.history)),
            (None, true) => "No saved history yet.".to_string(),
        };

        let side_panel = Paragraph::new(side_text)
            .block(Block::default().borders(Borders::ALL).title("History"))
            .wrap(Wrap { trim: false });
        frame.render_widget(side_panel, body_layout[1]);

        let footer = Paragraph::new(Line::from(state.status.clone()))
            .block(Block::default().borders(Borders::ALL).title("Status"));
        frame.render_widget(footer, layout[2]);
    })?;

    Ok(())
}
