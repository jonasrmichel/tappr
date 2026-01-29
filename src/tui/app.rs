use std::collections::VecDeque;
use std::io::{self, Stdout};
use std::sync::Arc;
use std::time::{Duration, Instant};

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

use super::widgets::{now_playing, settings, up_next, world_map};
use now_playing::PlayStatus;
use up_next::QueuedStation;

/// TUI application state
pub struct TuiApp {
    state: Arc<AppState>,
    cmd_tx: mpsc::Sender<ProducerCommand>,
    terminal: Terminal<CrosstermBackend<Stdout>>,

    // Now playing state (what's actually playing right now)
    now_playing_station: Option<StationInfo>,
    now_playing_loop: Option<LoopInfo>,
    now_playing_started: Option<Instant>,

    // Queue of upcoming stations
    up_next: VecDeque<QueuedStation>,

    // Display state
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
            now_playing_station: None,
            now_playing_loop: None,
            now_playing_started: None,
            up_next: VecDeque::new(),
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
    /// This is called when a worker starts processing a station
    pub fn set_loading(&mut self, _station: StationInfo) {
        // Loading state is now shown implicitly when we have items in queue
        // but nothing playing yet, or when queue is empty
        if self.now_playing_station.is_none() && self.up_next.is_empty() {
            self.play_status = PlayStatus::Loading;
        }
    }

    /// Set a station as now playing immediately (first clip)
    pub fn set_now_playing(&mut self, station: StationInfo, loop_info: LoopInfo) {
        // Add previous station to history
        if let Some(prev) = self.now_playing_station.take() {
            self.station_history.push(prev);
            // Keep last 10 stations
            if self.station_history.len() > 10 {
                self.station_history.remove(0);
            }
        }

        self.now_playing_station = Some(station);
        self.now_playing_loop = Some(loop_info);
        self.now_playing_started = Some(Instant::now());
        self.play_status = PlayStatus::Playing;
        self.last_error = None;
    }

    /// Add a station to the up next queue (subsequent clips)
    pub fn add_to_queue(&mut self, station: StationInfo, loop_info: LoopInfo) {
        self.up_next.push_back(QueuedStation { station, loop_info });
        self.play_status = PlayStatus::Playing;
        self.last_error = None;
    }

    /// Advance to the next station in queue (called when playback actually advances)
    /// Returns (duration_secs, bpm) of the new now-playing clip, if any
    pub fn advance_queue(&mut self) -> Option<(f32, f32)> {
        if let Some(next) = self.up_next.pop_front() {
            // Add current to history
            if let Some(prev) = self.now_playing_station.take() {
                self.station_history.push(prev);
                if self.station_history.len() > 10 {
                    self.station_history.remove(0);
                }
            }

            let duration_secs = next.loop_info.duration_samples as f32 / next.loop_info.sample_rate as f32;
            let bpm = next.loop_info.bpm;
            self.now_playing_station = Some(next.station);
            self.now_playing_loop = Some(next.loop_info);
            self.now_playing_started = Some(Instant::now());
            Some((duration_secs, bpm))
        } else {
            None
        }
    }


    /// Get the number of items in the up next queue
    pub fn queue_len(&self) -> usize {
        self.up_next.len()
    }

    /// Legacy method for compatibility - routes to appropriate method
    #[allow(dead_code)]
    pub fn set_playing(&mut self, station: StationInfo, loop_info: LoopInfo) {
        // This is kept for backwards compatibility but main.rs should use
        // set_now_playing() or add_to_queue() directly
        if self.now_playing_station.is_none() {
            self.set_now_playing(station, loop_info);
        } else {
            self.add_to_queue(station, loop_info);
        }
    }

    /// Update display with error
    pub fn set_error(&mut self, message: String) {
        self.play_status = PlayStatus::Error(message.clone());
        self.last_error = Some(message);
    }

    /// Draw the TUI
    pub fn draw(&mut self, settings: &Settings) -> Result<(), TuiError> {
        // Queue advancement is now handled by main.rs based on actual sink state

        let now_playing_station = self.now_playing_station.clone();
        let now_playing_loop = self.now_playing_loop.clone();
        let up_next_queue: Vec<QueuedStation> = self.up_next.iter().map(|q| QueuedStation {
            station: q.station.clone(),
            loop_info: q.loop_info.clone(),
        }).collect();
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

            // Left panel: settings + now playing + up next
            let left_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(9),  // Settings
                    Constraint::Length(14), // Now Playing
                    Constraint::Min(5),     // Up Next
                ])
                .split(body_chunks[0]);

            // Calculate playback progress for countdown visual
            let playback_progress = if let (Some(started), Some(loop_info)) = (self.now_playing_started, &now_playing_loop) {
                let elapsed = started.elapsed().as_secs_f32();
                let duration = loop_info.duration_samples as f32 / loop_info.sample_rate as f32;
                if duration > 0.0 {
                    Some((elapsed / duration).min(1.0))
                } else {
                    None
                }
            } else {
                None
            };

            // Render panels
            settings::render(frame, left_chunks[0], settings);
            now_playing::render(
                frame,
                left_chunks[1],
                now_playing_station.as_ref(),
                now_playing_loop.as_ref(),
                &play_status,
                playback_progress,
            );
            up_next::render(frame, left_chunks[2], &up_next_queue);
            world_map::render(
                frame,
                body_chunks[1],
                now_playing_station.as_ref(),
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
                        KeyCode::Char('d') => {
                            debug!("Cycle audio device");
                            let mut settings = self.state.settings.write().await;
                            if settings.next_audio_device() {
                                let device_index = settings.audio_device_index;
                                drop(settings); // Release lock before async send
                                let _ = self.cmd_tx.send(ProducerCommand::AudioDeviceChanged(device_index)).await;
                            }
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
            Span::raw(":bars  "),
            Span::styled("d", Style::default().fg(Color::Yellow)),
            Span::raw(":device"),
        ])
    };

    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(Color::DarkGray));

    let paragraph = Paragraph::new(controls).block(block).centered();
    frame.render_widget(paragraph, area);
}
