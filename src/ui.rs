use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::model::{Position, ResourceKind, RobotKind, Tile, WorldState};

/// Draws multiple independent world snapshots in a 2x2 grid.
pub(crate) fn draw_ui(frame: &mut Frame, worlds: &[WorldState], msg: &str) {
    let big_rows = Layout::vertical([Constraint::Percentage(90), Constraint::Percentage(10)])
        .split(frame.area());
    let rows = Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(big_rows[0]);
    let top =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(rows[0]);
    let bottom =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(rows[1]);
    let panels = [top[0], top[1], bottom[0], bottom[1]];

    for (index, area) in panels.into_iter().enumerate() {
        if let Some(world) = worlds.get(index) {
            render_world_panel(frame, area, world, index + 1);
        } else {
            let empty = Paragraph::new("No simulation").block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!("Simulation {}", index + 1)),
            );
            frame.render_widget(empty, area);
        }
    }

    frame.render_widget(
        Paragraph::new(msg).block(Block::default().borders(Borders::ALL)),
        big_rows[1],
    );
}

fn render_world_panel(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    world: &WorldState,
    index: usize,
) {
    let title = format!(
        "Sim {} | Energy: {}  Crystals: {}  Known resources: {}",
        index,
        world.total_energy,
        world.total_crystals,
        world.known_resources.len()
    );
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
                match world.map.tile_at(pos).unwrap_or(&Tile::Empty) {
                    Tile::Obstacle => ('0', Color::LightCyan),
                    Tile::Resource {
                        kind: ResourceKind::Energy,
                        ..
                    } => ('E', Color::Green),
                    Tile::Resource {
                        kind: ResourceKind::Crystal,
                        ..
                    } => ('C', Color::LightMagenta),
                    Tile::Base => ('#', Color::LightGreen),
                    Tile::Empty => ('.', Color::DarkGray),
                }
            };
            spans.push(Span::styled(ch.to_string(), Style::default().fg(color)));
        }
        lines.push(Line::from(spans));
    }
    let map_widget =
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(title));
    frame.render_widget(map_widget, area);
}
