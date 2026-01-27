use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::app::{LoopInfo, StationInfo};

/// Queued station info
pub struct QueuedStation {
    pub station: StationInfo,
    pub loop_info: LoopInfo,
}

/// Render the up next panel showing queued stations
pub fn render(frame: &mut Frame, area: Rect, queue: &[QueuedStation]) {
    let block = Block::default()
        .title(format!(" Up Next ({}) ", queue.len()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue));

    let lines: Vec<Line> = if queue.is_empty() {
        vec![
            Line::from(""),
            Line::from(Span::styled(
                "Loading stations...",
                Style::default().fg(Color::DarkGray).italic(),
            )),
        ]
    } else {
        queue
            .iter()
            .enumerate()
            .flat_map(|(i, q)| {
                let num_style = Style::default().fg(Color::DarkGray);
                let name_style = if i == 0 {
                    Style::default().fg(Color::White).bold()
                } else {
                    Style::default().fg(Color::Gray)
                };
                let location_style = Style::default().fg(Color::DarkGray);
                let bpm_style = Style::default().fg(Color::Magenta);

                let bpm_info = if q.loop_info.time_stretched {
                    vec![
                        Span::styled(format!("{:.0}", q.loop_info.source_bpm), Style::default().fg(Color::DarkGray)),
                        Span::styled("->", Style::default().fg(Color::Yellow)),
                        Span::styled(format!("{:.0}", q.loop_info.bpm), bpm_style),
                    ]
                } else {
                    vec![Span::styled(format!("{:.0} BPM", q.loop_info.bpm), bpm_style)]
                };

                vec![
                    Line::from(vec![
                        Span::styled(format!("{}. ", i + 1), num_style),
                        Span::styled(&q.station.name, name_style),
                    ]),
                    Line::from(
                        vec![
                            Span::raw("   "),
                            Span::styled(
                                format!("{}, {}", q.station.place_name, q.station.country),
                                location_style,
                            ),
                            Span::raw(" | "),
                        ]
                        .into_iter()
                        .chain(bpm_info)
                        .collect::<Vec<_>>()
                    ),
                ]
            })
            .collect()
    };

    let paragraph = Paragraph::new(lines).block(block).wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}
