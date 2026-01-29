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

/// Render the now playing panel
pub fn render(
    frame: &mut Frame,
    area: Rect,
    station: Option<&StationInfo>,
    loop_info: Option<&LoopInfo>,
    status: &PlayStatus,
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
                lines.push(Line::from(""));
                lines.push(Line::from(vec![
                    Span::styled("Web: ", Style::default().fg(Color::Gray)),
                    Span::styled(
                        url.clone(),
                        Style::default().fg(Color::Blue).underlined(),
                    ),
                ]));
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
