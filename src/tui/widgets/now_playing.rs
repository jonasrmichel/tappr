use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::app::{LoopInfo, StationInfo};

/// Current playback status
pub enum PlayStatus {
    Idle,
    Loading,
    Playing,
    Error(String),
}

/// Unicode block characters for fine-grained progress bar (8 levels per cell)
const PROGRESS_BLOCKS: [&str; 9] = [
    " ",  // 0/8 - empty
    "▏",  // 1/8
    "▎",  // 2/8
    "▍",  // 3/8
    "▌",  // 4/8
    "▋",  // 5/8
    "▊",  // 6/8
    "▉",  // 7/8
    "█",  // 8/8 - full
];

/// Create a horizontal progress bar showing remaining time
/// Returns a string of block characters representing the countdown
fn progress_bar(progress: f32, width: usize) -> String {
    // Progress is how much has elapsed (0.0 = just started, 1.0 = finished)
    // We show remaining time, so remaining = 1.0 - progress
    let remaining = (1.0 - progress).clamp(0.0, 1.0);

    // Calculate how many "eighths" of cells to fill
    let total_eighths = (remaining * width as f32 * 8.0) as usize;
    let full_cells = total_eighths / 8;
    let partial_eighths = total_eighths % 8;

    let mut bar = String::with_capacity(width);

    // Add full cells
    for _ in 0..full_cells {
        bar.push_str(PROGRESS_BLOCKS[8]); // █
    }

    // Add partial cell if there's remaining space
    if full_cells < width && partial_eighths > 0 {
        bar.push_str(PROGRESS_BLOCKS[partial_eighths]);
    }

    // Pad with spaces to fill width (for consistent layout)
    while bar.chars().count() < width {
        bar.push(' ');
    }

    bar
}

/// Render the now playing panel
/// progress: 0.0 = just started, 1.0 = finished (used for countdown visual)
pub fn render(
    frame: &mut Frame,
    area: Rect,
    station: Option<&StationInfo>,
    loop_info: Option<&LoopInfo>,
    status: &PlayStatus,
    progress: Option<f32>,
) {
    let (title_color, status_text) = match status {
        PlayStatus::Idle => (Color::Gray, "IDLE"),
        PlayStatus::Loading => (Color::Yellow, "LOADING"),
        PlayStatus::Playing => (Color::Green, "PLAYING"),
        PlayStatus::Error(_) => (Color::Red, "ERROR"),
    };

    let block = Block::default()
        .title(format!(" Now Playing [{}] ", status_text))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(title_color));

    let lines = match (station, loop_info, status) {
        (_, _, PlayStatus::Idle) => {
            vec![
                Line::from(""),
                Line::from(Span::styled(
                    "Press 'n' to start",
                    Style::default().fg(Color::Gray),
                )),
            ]
        }
        (Some(station), None, PlayStatus::Loading) => {
            vec![
                Line::from(Span::styled(
                    &station.name,
                    Style::default().bold().fg(Color::White),
                )),
                Line::from(Span::styled(
                    format!("{}, {}", station.place_name, station.country),
                    Style::default().fg(Color::Gray),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "Processing audio...",
                    Style::default().fg(Color::Yellow),
                )),
            ]
        }
        (Some(station), Some(info), PlayStatus::Playing) => {
            let mut lines = vec![
                Line::from(Span::styled(
                    &station.name,
                    Style::default().bold().fg(Color::White),
                )),
                Line::from(Span::styled(
                    format!("{}, {}", station.place_name, station.country),
                    Style::default().fg(Color::Gray),
                )),
                Line::from(""),
            ];

            // Show BPM info with time-stretch indicator if applied
            if info.time_stretched {
                lines.push(Line::from(vec![
                    Span::styled("BPM: ", Style::default().fg(Color::Gray)),
                    Span::styled(
                        format!("{:.0}", info.source_bpm),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::styled(" -> ", Style::default().fg(Color::Yellow)),
                    Span::styled(
                        format!("{:.0}", info.bpm),
                        Style::default().fg(Color::Magenta).bold(),
                    ),
                    Span::styled(" [stretched]", Style::default().fg(Color::Yellow).italic()),
                ]));
            } else {
                lines.push(Line::from(vec![
                    Span::styled("BPM: ", Style::default().fg(Color::Gray)),
                    Span::styled(
                        format!("{:.1}", info.bpm),
                        Style::default().fg(Color::Magenta).bold(),
                    ),
                    Span::raw(" "),
                    Span::styled(
                        format!("({:.0}%)", info.bpm_confidence * 100.0),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]));
            }

            lines.push(Line::from(vec![
                Span::styled("Loop: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    format!("{} bars", info.bars),
                    Style::default().fg(Color::Cyan),
                ),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Coords: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    format!("{:.2}, {:.2}", station.latitude, station.longitude),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));

            // Add website link if available
            if let Some(ref url) = station.website {
                lines.push(Line::from(vec![
                    Span::styled("Web: ", Style::default().fg(Color::Gray)),
                    Span::styled(
                        url.clone(),
                        Style::default().fg(Color::Blue).underlined(),
                    ),
                ]));
            }

            // Add progress bar at bottom (narrow, smooth countdown)
            if let Some(p) = progress {
                let bar = progress_bar(p, 15);
                lines.push(Line::from(Span::styled(
                    bar,
                    Style::default().fg(Color::Green),
                )));
            }

            lines
        }
        (_, _, PlayStatus::Error(msg)) => {
            vec![
                Line::from(Span::styled(msg, Style::default().fg(Color::Red))),
                Line::from(""),
                Line::from(Span::styled(
                    "Trying next station...",
                    Style::default().fg(Color::Yellow),
                )),
            ]
        }
        _ => vec![Line::from("")],
    };

    let paragraph = Paragraph::new(lines).block(block).wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}
