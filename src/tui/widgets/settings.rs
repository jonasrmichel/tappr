use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{BpmMode, Settings};

/// Render the settings panel
pub fn render(frame: &mut Frame, area: Rect, settings: &Settings) {
    let block = Block::default()
        .title(" Settings ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let bpm_text = match settings.bpm_mode {
        BpmMode::Auto { min, max } => format!("Auto ({:.0}-{:.0})", min, max),
        BpmMode::Fixed(bpm) => format!("Fixed ({:.0})", bpm),
    };

    // Truncate device name if too long
    let device_name = settings.current_audio_device_name();
    let device_display = if device_name.len() > 18 {
        format!("{}...", &device_name[..15])
    } else {
        device_name.to_string()
    };

    let lines = vec![
        Line::from(vec![
            Span::styled("BPM: ", Style::default().fg(Color::Gray)),
            Span::styled(bpm_text, Style::default().fg(Color::Yellow)),
        ]),
        Line::from(vec![
            Span::styled("Bars: ", Style::default().fg(Color::Gray)),
            Span::styled(
                settings.bars.to_string(),
                Style::default().fg(Color::Yellow),
            ),
        ]),
        Line::from(vec![
            Span::styled("Meter: ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{}/4", settings.beats_per_bar),
                Style::default().fg(Color::Yellow),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Device: ", Style::default().fg(Color::Gray)),
            Span::styled(device_display, Style::default().fg(Color::Magenta)),
        ]),
        Line::from(vec![
            Span::styled("Listen: ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{}s", settings.listen_seconds),
                Style::default().fg(Color::White),
            ),
        ]),
    ];

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}
