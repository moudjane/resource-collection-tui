use std::collections::{HashMap, VecDeque};
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crossterm::event::{self, Event};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::{Frame, Terminal};

mod model;
mod simulation;

use model::*;
use simulation::*;

fn draw_ui(frame: &mut Frame, world: &WorldState) {
    let chunks = Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).split(frame.area());
    let stats = Paragraph::new(format!(
        "Energy: {}  Crystals: {}  Known resources: {}",
        world.total_energy,
        world.total_crystals,
        world.known_resources.len()
    ))
    .block(Block::default().borders(Borders::ALL).title("Base"));
    frame.render_widget(stats, chunks[0]);

    let robots_by_pos: HashMap<Position, RobotKind> =
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
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title("Simulation"));
    frame.render_widget(map_widget, chunks[1]);
}

fn run_app() -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut rng = rand::rng();
    let map = Map::generate(MAP_WIDTH, MAP_HEIGHT, &mut rng);
    let world = Arc::new(Mutex::new(WorldState::new(
        map,
        SCOUT_COUNT,
        COLLECTOR_COUNT,
    )));
    let (tx, rx) = mpsc::channel();
    let running = Arc::new(AtomicBool::new(true));

    let mut workers = Vec::new();
    for robot in {
        let world = world.lock().expect("world lock poisoned");
        world.robots.clone()
    } {
        let tx = tx.clone();
        let world = Arc::clone(&world);
        let running = Arc::clone(&running);
        workers.push(if robot.kind.can_collect() {
            spawn_collector(robot.id, world, tx, running)
        } else {
            spawn_scout(robot.id, world, tx, running)
        });
    }

    let mut exit_requested = false;
    while !exit_requested {
        process_messages(&world, &rx);
        let snapshot = { world.lock().expect("world lock poisoned").clone() };
        terminal.draw(|f| draw_ui(f, &snapshot))?;
        if event::poll(Duration::from_millis(20))? && matches!(event::read()?, Event::Key(_)) {
            exit_requested = true;
        }
        thread::sleep(Duration::from_millis(40));
    }

    running.store(false, Ordering::Relaxed);
    for worker in workers {
        let _ = worker.join();
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn main() {
    if let Err(err) = run_app() {
        eprintln!("Application error: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    #[test]
    fn generated_resources_have_expected_amount_range() {
        let mut rng = StdRng::seed_from_u64(42);
        let map = Map::generate(30, 15, &mut rng);
        let mut energy_count = 0;
        let mut crystal_count = 0;
        for tile in &map.tiles {
            if let Tile::Resource { kind, amount } = tile {
                assert!((50..=200).contains(amount));
                match kind {
                    ResourceKind::Energy => energy_count += 1,
                    ResourceKind::Crystal => crystal_count += 1,
                }
            }
        }
        assert!(energy_count > 0);
        assert!(crystal_count > 0);
    }

    #[test]
    fn scout_cannot_collect_resources() {
        assert!(!RobotKind::Scout.can_collect());
        assert!(RobotKind::Collector.can_collect());
    }

    #[test]
    fn prefers_fresh_positions_over_recent_ones() {
        let recent = VecDeque::from([Position { x: 1, y: 0 }, Position { x: 0, y: 1 }]);
        let candidates = [Position { x: 1, y: 0 }, Position { x: 2, y: 0 }, Position { x: 0, y: 1 }];
        assert_eq!(choose_non_repeating_position(&candidates, &recent), Some(Position { x: 2, y: 0 }));
    }
}
