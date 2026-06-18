use std::collections::{HashMap, HashSet};

use noise::{NoiseFn, Perlin};
use rand::Rng;

pub(crate) const MAP_WIDTH: usize = 60;
pub(crate) const MAP_HEIGHT: usize = 24;
pub(crate) const SCOUT_COUNT: usize = 3;
pub(crate) const COLLECTOR_COUNT: usize = 4;

/// Position on the grid world.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Position {
    pub(crate) x: i32,
    pub(crate) y: i32,
}

impl Position {
    pub(crate) fn manhattan_distance(self, other: Position) -> i32 {
        (self.x - other.x).abs() + (self.y - other.y).abs()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResourceKind {
    Energy,
    Crystal,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Tile {
    Empty,
    Obstacle,
    Resource { kind: ResourceKind, amount: u16 },
    Base,
}

#[derive(Clone, Debug)]
pub(crate) struct Map {
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) tiles: Vec<Tile>,
    pub(crate) base: Position,
}

impl Map {
    /// Builds the world layout and seeds resources using Perlin noise.
    pub(crate) fn generate(width: usize, height: usize, rng: &mut impl Rng) -> Self {
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

    /// Adds a fixed number of resources of one kind on passable empty tiles.
    pub(crate) fn place_resources(&mut self, kind: ResourceKind, count: usize, rng: &mut impl Rng) {
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

    pub(crate) fn in_bounds(&self, pos: Position) -> bool {
        pos.x >= 0 && pos.y >= 0 && (pos.x as usize) < self.width && (pos.y as usize) < self.height
    }

    pub(crate) fn idx(&self, pos: Position) -> usize {
        pos.y as usize * self.width + pos.x as usize
    }

    pub(crate) fn tile_at(&self, pos: Position) -> Option<&Tile> {
        if self.in_bounds(pos) {
            Some(&self.tiles[self.idx(pos)])
        } else {
            None
        }
    }

    pub(crate) fn set_tile(&mut self, pos: Position, tile: Tile) {
        if self.in_bounds(pos) {
            let idx = self.idx(pos);
            self.tiles[idx] = tile;
        }
    }

    pub(crate) fn is_passable(&self, pos: Position) -> bool {
        !matches!(self.tile_at(pos), Some(Tile::Obstacle) | None)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RobotKind {
    Scout,
    Collector,
}

impl RobotKind {
    pub(crate) fn can_collect(self) -> bool {
        matches!(self, Self::Collector)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct RobotState {
    pub(crate) id: usize,
    pub(crate) kind: RobotKind,
    pub(crate) pos: Position,
}

#[derive(Clone, Debug)]
pub(crate) struct WorldState {
    pub(crate) map: Map,
    pub(crate) robots: Vec<RobotState>,
    pub(crate) known_obstacles: HashSet<Position>,
    pub(crate) known_resources: HashMap<Position, ResourceKind>,
    pub(crate) total_energy: u32,
    pub(crate) total_crystals: u32,
}

impl WorldState {
    /// Creates a new world snapshot with every robot starting at base.
    pub(crate) fn new(map: Map, scouts: usize, collectors: usize) -> Self {
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
pub(crate) enum Message {
    RobotMoved { id: usize, pos: Position },
    ObstacleFound(Position),
    ResourceFound { pos: Position, kind: ResourceKind },
    ResourceDepleted(Position),
    Deposited(ResourceKind),
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod model_tests;
