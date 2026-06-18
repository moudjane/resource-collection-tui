use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::model::{Position, ResourceKind, RobotKind, WorldState};

/// Draws the current world snapshot and the high-level base stats.
pub(crate) fn draw_ui(frame: &mut Frame, world: &WorldState) {
    let chunks = Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).split(frame.area());
    let stats = Paragraph::new(format!(
        "Energy: {}  Crystals: {}  Known resources: {}",
        world.total_energy,
        world.total_crystals,
        world.known_resources.len()
    ))
    .block(Block::default().borders(Borders::ALL).title("Base"));
    frame.render_widget(stats, chunks[0]);

    let robots_by_pos: std::collections::HashMap<Position, RobotKind> =
        world.robots.iter().map(|r| (r.pos, r.kind)).collect();
    let mut lines = Vec::with_capacity(world.map.height);
    for y in 0..world.map.height {
        let mut spans = Vec::with_capacity(world.map.width);
        for x in 0..world.map.width {
            let pos = Position {
                x: x as i32,
                y: y as i32,
            };
            let (ch, color) = if let Some(kind) = robots_by_pos.get(&pos) {
                match kind {
                    RobotKind::Scout => ('x', Color::Red),
                    RobotKind::Collector => ('o', Color::Magenta),
                }
            } else {
                match world.map.tile_at(pos).unwrap_or(&crate::model::Tile::Empty) {
                    crate::model::Tile::Obstacle => ('0', Color::LightCyan),
                    crate::model::Tile::Resource {
                        kind: ResourceKind::Energy,
                        ..
                    } => ('E', Color::Green),
                    crate::model::Tile::Resource {
                        kind: ResourceKind::Crystal,
                        ..
                    } => ('C', Color::LightMagenta),
                    crate::model::Tile::Base => ('#', Color::LightGreen),
                    crate::model::Tile::Empty => ('.', Color::DarkGray),
                }
            };
            spans.push(Span::styled(ch.to_string(), Style::default().fg(color)));
        }
        lines.push(Line::from(spans));
    }
    let map_widget =
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title("Simulation"));
    frame.render_widget(map_widget, chunks[1]);
}
