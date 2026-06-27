use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use rand::Rng;

use crate::model::{
    COLLECTOR_COUNT, MAP_HEIGHT, MAP_WIDTH, Map, Message, Position, ResourceKind, SCOUT_COUNT,
    Tile, WorldState,
};

pub(crate) const SIMULATION_COUNT: usize = 4;

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

/// Scans the 3x3 area around a robot and records newly discovered tiles.
pub(crate) fn discover_surroundings(
    world: &Arc<Mutex<WorldState>>,
    tx: &Sender<Message>,
    center: Position,
    known_obstacles: &mut HashSet<Position>,
    known_resources: &mut HashSet<Position>,
) {
    let world = world.lock().expect("world lock poisoned");
    for dy in -1..=1 {
        for dx in -1..=1 {
            let pos = Position {
                x: center.x + dx,
                y: center.y + dy,
            };
            if let Some(tile) = world.map.tile_at(pos) {
                match tile {
                    Tile::Obstacle if known_obstacles.insert(pos) => {
                        let _ = tx.send(Message::ObstacleFound(pos));
                    }
                    Tile::Resource { kind, .. } if known_resources.insert(pos) => {
                        let _ = tx.send(Message::ResourceFound {
                            pos,
                            kind: *kind,
                        });
                    }
                    _ => {}
                }
            }
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

/// Keeps a short trail so a robot can avoid immediate backtracking.
pub(crate) fn push_recent_position(
    recent: &mut VecDeque<Position>,
    pos: Position,
    capacity: usize,
) {
    if recent.back().copied() != Some(pos) {
        recent.push_back(pos);
    }
    while recent.len() > capacity {
        recent.pop_front();
    }
}

pub(crate) fn choose_non_repeating_position(
    candidates: &[Position],
    recent: &VecDeque<Position>,
) -> Option<Position> {
    candidates
        .iter()
        .copied()
        .find(|candidate| !recent.contains(candidate))
        .or_else(|| candidates.first().copied())
}

pub(crate) fn step_towards_avoiding_recent(
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

    if let Some(candidate) = preferred.into_iter().find(|candidate| {
        *candidate != from && world.map.is_passable(*candidate) && !recent.contains(candidate)
    }) {
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

/// Scouts explore toward the edge of known terrain instead of wandering randomly.
pub(crate) fn spawn_scout(
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
                        choose_non_repeating_position(&options, &recent_positions).unwrap_or_else(
                            || {
                                if options.is_empty() {
                                    current
                                } else {
                                    options[rng.random_range(0..options.len())]
                                }
                            },
                        )
                    })
            };
            let _ = tx.send(Message::RobotMoved { id, pos: next });
            push_recent_position(&mut recent_positions, current, 4);
            push_recent_position(&mut recent_positions, next, 4);
            thread::sleep(Duration::from_millis(40));
        }
    })
}

fn dbg_log(dbg_tx: &Option<mpsc::Sender<String>>, msg: String) {
    if let Some(tx) = dbg_tx {
        tx.send(msg).unwrap();
    }
}

/// Collectors chase known resources, harvest them, and bring goods back to base.
pub(crate) fn spawn_collector(
    dbg_tx: Option<mpsc::Sender<String>>,
    id: usize,
    world: Arc<Mutex<WorldState>>,
    tx: Sender<Message>,
    running: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut carrying: Option<ResourceKind> = None;
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
                thread::sleep(Duration::from_millis(40));
                continue;
            }

            let fmt_known_resources = format!("{known_resources:?}");

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

            dbg_log(&dbg_tx, format!("sim4: {fmt_known_resources} {target:?}"));

            if let Some(target) = target {
                if target == current {
                    let mut world = world.lock().expect("world lock poisoned");
                    if let Some(Tile::Resource { kind, amount }) =
                        world.map.tile_at(current).cloned()
                        && amount > 0 {
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
                } else {
                    let next = {
                        let world = world.lock().expect("world lock poisoned");
                        step_towards_avoiding_recent(&world, current, target, &recent_positions)
                    };
                    let _ = tx.send(Message::RobotMoved { id, pos: next });
                    push_recent_position(&mut recent_positions, current, 6);
                    push_recent_position(&mut recent_positions, next, 6);
                }
            }
            thread::sleep(Duration::from_millis(40));
        }
    })
}

/// Applies worker messages to the shared world snapshot.
pub(crate) fn process_messages(world: &Arc<Mutex<WorldState>>, rx: &Receiver<Message>) {
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

/// Owns one independent simulation world and the worker threads that animate it.
pub(crate) struct SimulationInstance {
    world: Arc<Mutex<WorldState>>,
    rx: Receiver<Message>,
    running: Arc<AtomicBool>,
    workers: Vec<thread::JoinHandle<()>>,
}

impl SimulationInstance {
    /// Builds a fresh world and starts its scouts and collectors.
    pub(crate) fn spawn(debug_tx: Option<mpsc::Sender<String>>) -> Self {
        let mut rng = rand::rng();
        let map = Map::generate(MAP_WIDTH, MAP_HEIGHT, &mut rng);
        let world = Arc::new(Mutex::new(WorldState::new(
            map,
            SCOUT_COUNT,
            COLLECTOR_COUNT,
        )));
        let (tx, rx) = std::sync::mpsc::channel();
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
                spawn_collector(debug_tx.clone(), robot.id, world, tx, running)
            } else {
                spawn_scout(robot.id, world, tx, running)
            });
        }

        Self {
            world,
            rx,
            running,
            workers,
        }
    }

    /// Drains pending worker messages into the world snapshot.
    pub(crate) fn process_messages(&self) {
        process_messages(&self.world, &self.rx);
    }

    /// Returns a cloned snapshot for rendering.
    pub(crate) fn snapshot(&self) -> WorldState {
        self.world.lock().expect("world lock poisoned").clone()
    }

    /// Stops the workers and waits for them to finish.
    pub(crate) fn stop(mut self) {
        self.running.store(false, Ordering::Relaxed);
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
#[path = "simulation_tests.rs"]
mod simulation_tests;
