use std::collections::{HashMap, HashSet, VecDeque};
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crossterm::event::{self, Event};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use noise::{NoiseFn, Perlin};
use rand::Rng;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::{Frame, Terminal};

const MAP_WIDTH: usize = 60;
const MAP_HEIGHT: usize = 24;
const SCOUT_COUNT: usize = 3;
const COLLECTOR_COUNT: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Position {
    x: i32,
    y: i32,
}

impl Position {
    fn manhattan_distance(self, other: Position) -> i32 {
        (self.x - other.x).abs() + (self.y - other.y).abs()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResourceKind {
    Energy,
    Crystal,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Tile {
    Empty,
    Obstacle,
    Resource { kind: ResourceKind, amount: u16 },
    Base,
}

#[derive(Clone, Debug)]
struct Map {
    width: usize,
    height: usize,
    tiles: Vec<Tile>,
    base: Position,
}

impl Map {
    fn generate(width: usize, height: usize, rng: &mut impl Rng) -> Self {
        let base = Position {
            x: (width as i32) / 2,
            y: (height as i32) / 2,
        };
        let mut map = Self {
            width,
            height,
            tiles: vec![Tile::Empty; width * height],
            base,
        };
        let perlin = Perlin::new(rng.random());

        for y in 0..height {
            for x in 0..width {
                let pos = Position {
                    x: x as i32,
                    y: y as i32,
                };
                if pos.manhattan_distance(base) <= 1 {
                    continue;
                }
                let value = perlin.get([x as f64 / 9.0, y as f64 / 9.0]);
                if value > 0.25 {
                    map.set_tile(pos, Tile::Obstacle);
                }
            }
        }
        map.set_tile(base, Tile::Base);
        map.place_resources(ResourceKind::Energy, 16, rng);
        map.place_resources(ResourceKind::Crystal, 16, rng);
        map
    }

    fn place_resources(&mut self, kind: ResourceKind, count: usize, rng: &mut impl Rng) {
        let mut placed = 0;
        let mut attempts = 0;
        while placed < count && attempts < count * 200 {
            attempts += 1;
            let pos = Position {
                x: rng.random_range(0..self.width as i32),
                y: rng.random_range(0..self.height as i32),
            };
            if pos == self.base {
                continue;
            }
            if matches!(self.tile_at(pos), Some(Tile::Empty)) {
                let amount = rng.random_range(50..=200);
                self.set_tile(pos, Tile::Resource { kind, amount });
                placed += 1;
            }
        }
    }

    fn in_bounds(&self, pos: Position) -> bool {
        pos.x >= 0 && pos.y >= 0 && (pos.x as usize) < self.width && (pos.y as usize) < self.height
    }

    fn idx(&self, pos: Position) -> usize {
        pos.y as usize * self.width + pos.x as usize
    }

    fn tile_at(&self, pos: Position) -> Option<&Tile> {
        if self.in_bounds(pos) {
            Some(&self.tiles[self.idx(pos)])
        } else {
            None
        }
    }

    fn set_tile(&mut self, pos: Position, tile: Tile) {
        if self.in_bounds(pos) {
            let idx = self.idx(pos);
            self.tiles[idx] = tile;
        }
    }

    fn is_passable(&self, pos: Position) -> bool {
        !matches!(self.tile_at(pos), Some(Tile::Obstacle) | None)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RobotKind {
    Scout,
    Collector,
}

impl RobotKind {
    fn can_collect(self) -> bool {
        matches!(self, Self::Collector)
    }
}

#[derive(Clone, Debug)]
struct RobotState {
    id: usize,
    kind: RobotKind,
    pos: Position,
}

#[derive(Clone, Debug)]
struct WorldState {
    map: Map,
    robots: Vec<RobotState>,
    known_obstacles: HashSet<Position>,
    known_resources: HashMap<Position, ResourceKind>,
    total_energy: u32,
    total_crystals: u32,
}

impl WorldState {
    fn new(map: Map, scouts: usize, collectors: usize) -> Self {
        let mut robots = Vec::new();
        for i in 0..scouts {
            robots.push(RobotState {
                id: i,
                kind: RobotKind::Scout,
                pos: map.base,
            });
        }
        for i in 0..collectors {
            robots.push(RobotState {
                id: i + scouts,
                kind: RobotKind::Collector,
                pos: map.base,
            });
        }
        Self {
            map,
            robots,
            known_obstacles: HashSet::new(),
            known_resources: HashMap::new(),
            total_energy: 0,
            total_crystals: 0,
        }
    }
}

#[derive(Debug)]
enum Message {
    RobotMoved { id: usize, pos: Position },
    ObstacleFound(Position),
    ResourceFound { pos: Position, kind: ResourceKind },
    ResourceDepleted(Position),
    Deposited(ResourceKind),
}

fn neighbors(pos: Position) -> [Position; 4] {
    [
        Position {
            x: pos.x + 1,
            y: pos.y,
        },
        Position {
            x: pos.x - 1,
            y: pos.y,
        },
        Position {
            x: pos.x,
            y: pos.y + 1,
        },
        Position {
            x: pos.x,
            y: pos.y - 1,
        },
    ]
}

fn discover_surroundings(
    world: &Arc<Mutex<WorldState>>,
    tx: &Sender<Message>,
    center: Position,
    known_obstacles: &mut HashSet<Position>,
    known_resources: &mut HashSet<Position>,
) {
    let mut found = Vec::new();
    {
        let world = world.lock().expect("world lock poisoned");
        for dy in -1..=1 {
            for dx in -1..=1 {
                let pos = Position {
                    x: center.x + dx,
                    y: center.y + dy,
                };
                if let Some(tile) = world.map.tile_at(pos) {
                    found.push((pos, tile.clone()));
                }
            }
        }
    }
    for (pos, tile) in found {
        match tile {
            Tile::Obstacle if known_obstacles.insert(pos) => {
                let _ = tx.send(Message::ObstacleFound(pos));
            }
            Tile::Resource { kind, .. } if known_resources.insert(pos) => {
                let _ = tx.send(Message::ResourceFound { pos, kind });
            }
            _ => {}
        }
    }
}

fn is_frontier_tile(world: &WorldState, known_tiles: &HashSet<Position>, pos: Position) -> bool {
    matches!(world.map.tile_at(pos), Some(tile) if !matches!(tile, Tile::Obstacle))
        && neighbors(pos)
            .into_iter()
            .any(|neighbor| world.map.in_bounds(neighbor) && !known_tiles.contains(&neighbor))
}

fn bfs_next_step_towards<F>(world: &WorldState, start: Position, is_goal: F) -> Option<Position>
where
    F: Fn(Position) -> bool,
{
    let mut queue = VecDeque::from([start]);
    let mut came_from = HashMap::new();
    came_from.insert(start, start);

    while let Some(current) = queue.pop_front() {
        if current != start && is_goal(current) {
            let mut step = current;
            while came_from[&step] != start {
                step = came_from[&step];
            }
            return Some(step);
        }

        for neighbor in neighbors(current) {
            if !world.map.is_passable(neighbor) || came_from.contains_key(&neighbor) {
                continue;
            }
            came_from.insert(neighbor, current);
            queue.push_back(neighbor);
        }
    }

    None
}

fn push_recent_position(recent: &mut VecDeque<Position>, pos: Position, capacity: usize) {
    if recent.back().copied() != Some(pos) {
        recent.push_back(pos);
    }
    while recent.len() > capacity {
        recent.pop_front();
    }
}

fn choose_non_repeating_position(
    candidates: &[Position],
    recent: &VecDeque<Position>,
) -> Option<Position> {
    candidates
        .iter()
        .copied()
        .find(|candidate| !recent.contains(candidate))
        .or_else(|| candidates.first().copied())
}

fn step_towards_avoiding_recent(
    world: &WorldState,
    from: Position,
    target: Position,
    recent: &VecDeque<Position>,
) -> Position {
    let dx = (target.x - from.x).signum();
    let dy = (target.y - from.y).signum();
    let preferred = [
        Position {
            x: from.x + dx,
            y: from.y,
        },
        Position {
            x: from.x,
            y: from.y + dy,
        },
        Position {
            x: from.x + dx,
            y: from.y + dy,
        },
    ];

    if let Some(candidate) = preferred
        .into_iter()
        .find(|candidate| *candidate != from && world.map.is_passable(*candidate) && !recent.contains(candidate))
    {
        return candidate;
    }

    if let Some(candidate) = preferred
        .into_iter()
        .find(|candidate| *candidate != from && world.map.is_passable(*candidate))
    {
        return candidate;
    }

    let passable_neighbors: Vec<_> = neighbors(from)
        .into_iter()
        .filter(|p| world.map.is_passable(*p))
        .collect();
    choose_non_repeating_position(&passable_neighbors, recent).unwrap_or(from)
}

fn spawn_scout(
    id: usize,
    world: Arc<Mutex<WorldState>>,
    tx: Sender<Message>,
    running: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut known_obstacles = HashSet::new();
        let mut known_resources = HashSet::new();
        let mut known_tiles = HashSet::new();
        let mut rng = rand::rng();
        let mut recent_positions = VecDeque::with_capacity(4);
        while running.load(Ordering::Relaxed) {
            let current = {
                let world = world.lock().expect("world lock poisoned");
                world.robots[id].pos
            };
            discover_surroundings(
                &world,
                &tx,
                current,
                &mut known_obstacles,
                &mut known_resources,
            );
            {
                let world = world.lock().expect("world lock poisoned");
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let pos = Position {
                            x: current.x + dx,
                            y: current.y + dy,
                        };
                        if world.map.in_bounds(pos) {
                            known_tiles.insert(pos);
                        }
                    }
                }
            }

            let next = {
                let world = world.lock().expect("world lock poisoned");
                let frontier_step = bfs_next_step_towards(&world, current, |pos| {
                    is_frontier_tile(&world, &known_tiles, pos)
                });

                frontier_step
                    .and_then(|step| {
                        if recent_positions.contains(&step) {
                            None
                        } else {
                            Some(step)
                        }
                    })
                    .or(frontier_step)
                    .unwrap_or_else(|| {
                        let options: Vec<_> = neighbors(current)
                            .into_iter()
                            .filter(|p| world.map.is_passable(*p) && !known_obstacles.contains(p))
                            .collect();
                        choose_non_repeating_position(&options, &recent_positions)
                            .unwrap_or_else(|| {
                                if options.is_empty() {
                                    current
                                } else {
                                    options[rng.random_range(0..options.len())]
                                }
                            })
                    })
            };
            let _ = tx.send(Message::RobotMoved { id, pos: next });
            push_recent_position(&mut recent_positions, current, 4);
            push_recent_position(&mut recent_positions, next, 4);
            thread::sleep(Duration::from_millis(75));
        }
    })
}

fn spawn_collector(
    id: usize,
    world: Arc<Mutex<WorldState>>,
    tx: Sender<Message>,
    running: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut carrying: Option<ResourceKind> = None;
        let mut rng = rand::rng();
        let mut recent_positions = VecDeque::with_capacity(6);
        while running.load(Ordering::Relaxed) {
            let (current, base, known_resources) = {
                let world = world.lock().expect("world lock poisoned");
                (
                    world.robots[id].pos,
                    world.map.base,
                    world.known_resources.keys().copied().collect::<Vec<_>>(),
                )
            };
            if let Some(kind) = carrying {
                let next = {
                    let world = world.lock().expect("world lock poisoned");
                    step_towards_avoiding_recent(&world, current, base, &recent_positions)
                };
                let _ = tx.send(Message::RobotMoved { id, pos: next });
                if next == base {
                    let _ = tx.send(Message::Deposited(kind));
                    carrying = None;
                }
                push_recent_position(&mut recent_positions, current, 6);
                push_recent_position(&mut recent_positions, next, 6);
                thread::sleep(Duration::from_millis(80));
                continue;
            }

            let target = {
                let world = world.lock().expect("world lock poisoned");
                known_resources
                    .into_iter()
                    .filter(|pos| {
                        matches!(
                            world.map.tile_at(*pos),
                            Some(Tile::Resource {
                                amount,
                                ..
                            }) if *amount > 0
                        )
                    })
                    .min_by_key(|pos| pos.manhattan_distance(current))
            };

            if let Some(target) = target {
                if target == current {
                    let mut world = world.lock().expect("world lock poisoned");
                    if let Some(Tile::Resource { kind, amount }) =
                        world.map.tile_at(current).cloned()
                    {
                        if amount > 0 {
                            carrying = Some(kind);
                            let remaining = amount - 1;
                            if remaining == 0 {
                                world.map.set_tile(current, Tile::Empty);
                                let _ = tx.send(Message::ResourceDepleted(current));
                            } else {
                                world.map.set_tile(
                                    current,
                                    Tile::Resource {
                                        kind,
                                        amount: remaining,
                                    },
                                );
                            }
                        }
                    }
                } else {
                    let next = {
                        let world = world.lock().expect("world lock poisoned");
                        step_towards_avoiding_recent(&world, current, target, &recent_positions)
                    };
                    let _ = tx.send(Message::RobotMoved { id, pos: next });
                    push_recent_position(&mut recent_positions, current, 6);
                    push_recent_position(&mut recent_positions, next, 6);
                }
            } else {
                let options: Vec<_> = {
                    let world = world.lock().expect("world lock poisoned");
                    neighbors(current)
                        .into_iter()
                        .filter(|p| world.map.is_passable(*p))
                        .collect()
                };
                if !options.is_empty() {
                    let next = choose_non_repeating_position(&options, &recent_positions)
                        .unwrap_or_else(|| options[rng.random_range(0..options.len())]);
                    let _ = tx.send(Message::RobotMoved { id, pos: next });
                    push_recent_position(&mut recent_positions, current, 6);
                    push_recent_position(&mut recent_positions, next, 6);
                }
            }
            thread::sleep(Duration::from_millis(80));
        }
    })
}

fn process_messages(world: &Arc<Mutex<WorldState>>, rx: &Receiver<Message>) {
    while let Ok(message) = rx.try_recv() {
        let mut world = world.lock().expect("world lock poisoned");
        match message {
            Message::RobotMoved { id, pos } => world.robots[id].pos = pos,
            Message::ObstacleFound(pos) => {
                world.known_obstacles.insert(pos);
            }
            Message::ResourceFound { pos, kind } => {
                world.known_resources.insert(pos, kind);
            }
            Message::ResourceDepleted(pos) => {
                world.known_resources.remove(&pos);
            }
            Message::Deposited(ResourceKind::Energy) => world.total_energy += 1,
            Message::Deposited(ResourceKind::Crystal) => world.total_crystals += 1,
        }
    }
}

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
