use std::io::{self, Stdout};
use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};
use tokio::sync::mpsc;
use tracing::{debug, error, info};

use crate::app::{AppState, LoopInfo, Settings, StationInfo};
use crate::error::TuiError;
use crate::tasks::ProducerCommand;

use super::widgets::{now_playing, settings, world_map};
use now_playing::PlayStatus;

/// TUI application state
pub struct TuiApp {
    state: Arc<AppState>,
    cmd_tx: mpsc::Sender<ProducerCommand>,
    terminal: Terminal<CrosstermBackend<Stdout>>,

    // Display state
    current_station: Option<StationInfo>,
    current_loop: Option<LoopInfo>,
    station_history: Vec<StationInfo>,
    play_status: PlayStatus,
    last_error: Option<String>,
}

impl TuiApp {
    /// Create a new TUI application
    pub fn new(
        state: Arc<AppState>,
        cmd_tx: mpsc::Sender<ProducerCommand>,
    ) -> Result<Self, TuiError> {
        // Set up terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        Ok(Self {
            state,
            cmd_tx,
            terminal,
            current_station: None,
            current_loop: None,
            station_history: Vec::new(),
            play_status: PlayStatus::Idle,
            last_error: None,
        })
    }

    /// Restore terminal state
    fn restore_terminal(&mut self) -> Result<(), TuiError> {
        disable_raw_mode()?;
        execute!(self.terminal.backend_mut(), LeaveAlternateScreen)?;
        self.terminal.show_cursor()?;
        Ok(())
    }

    /// Update display with new station (loading state)
    pub fn set_loading(&mut self, station: StationInfo) {
        self.current_station = Some(station);
        self.current_loop = None;
        self.play_status = PlayStatus::Loading;
    }

    /// Update display with new loop (playing state)
    pub fn set_playing(&mut self, station: StationInfo, loop_info: LoopInfo) {
        // Add previous station to history
        if let Some(prev) = self.current_station.take() {
            self.station_history.push(prev);
            // Keep last 10 stations
            if self.station_history.len() > 10 {
                self.station_history.remove(0);
            }
        }

        self.current_station = Some(station);
        self.current_loop = Some(loop_info);
        self.play_status = PlayStatus::Playing;
        self.last_error = None;
    }

    /// Update display with error
    pub fn set_error(&mut self, message: String) {
        self.play_status = PlayStatus::Error(message.clone());
        self.last_error = Some(message);
    }

    /// Draw the TUI
    pub fn draw(&mut self, settings: &Settings) -> Result<(), TuiError> {
        let current_station = self.current_station.clone();
        let current_loop = self.current_loop.clone();
        let station_history = self.station_history.clone();
        let play_status = match &self.play_status {
            PlayStatus::Idle => PlayStatus::Idle,
            PlayStatus::Loading => PlayStatus::Loading,
            PlayStatus::Playing => PlayStatus::Playing,
            PlayStatus::Error(msg) => PlayStatus::Error(msg.clone()),
        };
        let last_error = self.last_error.clone();

        self.terminal.draw(|frame| {
            let area = frame.area();

            // Main layout: header, body, footer
            let main_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3), // Header
                    Constraint::Min(10),   // Body
                    Constraint::Length(3), // Footer
                ])
                .split(area);

            // Render header
            render_header(frame, main_chunks[0], &play_status);

            // Body layout: left panel (30%) + world map (70%)
            let body_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
                .split(main_chunks[1]);

            // Left panel: settings + now playing
            let left_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(9), Constraint::Min(5)])
                .split(body_chunks[0]);

            // Render panels
            settings::render(frame, left_chunks[0], settings);
            now_playing::render(
                frame,
                left_chunks[1],
                current_station.as_ref(),
                current_loop.as_ref(),
                &play_status,
            );
            world_map::render(
                frame,
                body_chunks[1],
                current_station.as_ref(),
                &station_history,
            );

            // Render footer with controls and any error
            render_footer(frame, main_chunks[2], last_error.as_deref());
        })?;

        Ok(())
    }

    /// Handle keyboard input (non-blocking)
    pub async fn handle_input(&mut self) -> Result<bool, TuiError> {
        // Poll for events with a short timeout
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            info!("Quit requested");
                            self.state.quit();
                            let _ = self.cmd_tx.send(ProducerCommand::Quit).await;
                            return Ok(true); // Signal quit
                        }
                        KeyCode::Char('n') => {
                            debug!("Next station requested");
                            let _ = self.cmd_tx.send(ProducerCommand::NextStation).await;
                        }
                        KeyCode::Char('b') => {
                            debug!("Toggle BPM mode");
                            let mut settings = self.state.settings.write().await;
                            settings.toggle_bpm_mode();
                        }
                        KeyCode::Char('+') | KeyCode::Char('=') => {
                            debug!("Increase bars");
                            let mut settings = self.state.settings.write().await;
                            settings.cycle_bars_up();
                        }
                        KeyCode::Char('-') => {
                            debug!("Decrease bars");
                            let mut settings = self.state.settings.write().await;
                            settings.cycle_bars_down();
                        }
                        _ => {}
                    }
                }
            }
        }

        Ok(false)
    }

    /// Run cleanup on drop
    pub fn cleanup(&mut self) {
        if let Err(e) = self.restore_terminal() {
            error!(error = %e, "Failed to restore terminal");
        }
    }
}

impl Drop for TuiApp {
    fn drop(&mut self) {
        self.cleanup();
    }
}

/// Render the header bar
fn render_header(frame: &mut Frame, area: Rect, status: &PlayStatus) {
    let status_text = match status {
        PlayStatus::Idle => ("IDLE", Color::Gray),
        PlayStatus::Loading => ("LOADING", Color::Yellow),
        PlayStatus::Playing => ("PLAYING", Color::Green),
        PlayStatus::Error(_) => ("ERROR", Color::Red),
    };

    let title = Line::from(vec![
        Span::styled(" tappr ", Style::default().bold().fg(Color::Cyan)),
        Span::raw("| "),
        Span::styled(status_text.0, Style::default().fg(status_text.1)),
        Span::raw(" | Ride the beat of the world's airwaves"),
    ]);

    let block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(Color::DarkGray));

    let paragraph = Paragraph::new(title).block(block).centered();
    frame.render_widget(paragraph, area);
}

/// Render the footer with controls
fn render_footer(frame: &mut Frame, area: Rect, error: Option<&str>) {
    let controls = if let Some(err) = error {
        Line::from(vec![
            Span::styled("Error: ", Style::default().fg(Color::Red)),
            Span::styled(err, Style::default().fg(Color::Red)),
        ])
    } else {
        Line::from(vec![
            Span::styled("q", Style::default().fg(Color::Yellow)),
            Span::raw(":quit  "),
            Span::styled("n", Style::default().fg(Color::Yellow)),
            Span::raw(":next  "),
            Span::styled("b", Style::default().fg(Color::Yellow)),
            Span::raw(":bpm  "),
            Span::styled("+/-", Style::default().fg(Color::Yellow)),
            Span::raw(":bars"),
        ])
    };

    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(Color::DarkGray));

    let paragraph = Paragraph::new(controls).block(block).centered();
    frame.render_widget(paragraph, area);
}
