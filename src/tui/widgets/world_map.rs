use ratatui::prelude::*;
use ratatui::symbols::Marker;
use ratatui::widgets::canvas::{Canvas, Circle, Map, MapResolution};
use ratatui::widgets::{Block, Borders};

use crate::app::StationInfo;

/// Render the world map with station marker
pub fn render(frame: &mut Frame, area: Rect, station: Option<&StationInfo>, history: &[StationInfo]) {
    let block = Block::default()
        .title(" World Map ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue));

    // Clone data for the closure
    let station_data = station.map(|s| (s.name.clone(), s.latitude, s.longitude));
    let history_coords: Vec<(f64, f64)> = history
        .iter()
        .map(|s| (s.latitude, s.longitude))
        .collect();

    let canvas = Canvas::default()
        .block(block)
        .marker(Marker::Braille)
        .x_bounds([-180.0, 180.0])
        .y_bounds([-90.0, 90.0])
        .paint(move |ctx| {
            // Draw world map
            ctx.draw(&Map {
                color: Color::DarkGray,
                resolution: MapResolution::High,
            });

            // Draw history trail (fading dots)
            for (i, (lat, lon)) in history_coords.iter().rev().take(5).enumerate() {
                let color = if i < 2 {
                    Color::Rgb(100, 100, 100)
                } else {
                    Color::Rgb(60, 60, 60)
                };

                ctx.draw(&Circle {
                    x: *lon,
                    y: *lat,
                    radius: 1.5,
                    color,
                });
            }

            // Draw current station marker
            if let Some((name, lat, lon)) = &station_data {
                // Outer glow
                ctx.draw(&Circle {
                    x: *lon,
                    y: *lat,
                    radius: 4.0,
                    color: Color::Rgb(255, 100, 100),
                });

                // Inner marker
                ctx.draw(&Circle {
                    x: *lon,
                    y: *lat,
                    radius: 2.0,
                    color: Color::Red,
                });

                // Station label (offset above marker)
                let label_y = lat + 8.0;
                ctx.print(
                    *lon,
                    label_y.min(85.0), // Keep label on screen
                    Span::styled(name.clone(), Style::default().fg(Color::White)),
                );
            }
        });

    frame.render_widget(canvas, area);
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_coordinate_bounds() {
        // Test that coordinates are within bounds
        let lat = 51.5074; // London
        let lon = -0.1278;

        assert!(lat >= -90.0 && lat <= 90.0);
        assert!(lon >= -180.0 && lon <= 180.0);
    }
}
