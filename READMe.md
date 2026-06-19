# resource-collection-tui

A terminal-based, multi-threaded simulation of autonomous robots exploring a procedurally generated world and gathering resources, all rendered live in your terminal with [`ratatui`](https://ratatui.rs/).

Four independent worlds run side by side in a 2×2 grid. In each one, scout robots map the unknown terrain while collector robots hunt down discovered resources, harvest them, and haul them back to base.

## Preview

Each panel shows one live world. The header tracks the running totals of resources deposited and the number of resources currently known.

```
┌ Sim 1 | Energy: 12  Crystals: 9  Known resources: 21 ───────┐
│ ..........0..........E......................#............... │
│ ....0000....x.........................o..................... │
│ ...........C..........0000.................................. │
│ ...........................#............E................... │
└─────────────────────────────────────────────────────────────┘
```

### Legend

| Symbol | Meaning            | Color          |
| :----: | ------------------ | -------------- |
|  `#`   | Base               | Light green    |
|  `x`   | Scout robot        | Red            |
|  `o`   | Collector robot    | Magenta        |
|  `E`   | Energy resource    | Green          |
|  `C`   | Crystal resource   | Light magenta  |
|  `0`   | Obstacle           | Light cyan     |
|  `.`   | Empty tile         | Dark gray      |

## How it works

The simulation is built around shared state mutated by independent worker threads and read back by a render loop.

**The world.** Each map is a 60×24 grid. A base is placed at the center, obstacles are carved out using Perlin noise, and 16 energy and 16 crystal deposits are scattered across the passable empty tiles, each holding a random amount between 50 and 200 units.

**The robots.** Every world is staffed by 3 scouts and 4 collectors, and each robot runs on its own thread.

- **Scouts** explore toward the frontier of known terrain rather than wandering at random. They scan the 3×3 area around themselves, record newly discovered obstacles and resources, and use a breadth-first search to step toward the nearest tile bordering unexplored ground. A short trail of recent positions discourages immediate backtracking.
- **Collectors** wait for scouts to reveal resources, then path toward the closest known deposit, harvest one unit, and carry it back to base. Depleted deposits are cleared from the map. When no resource is known yet, they drift across passable tiles until one appears.

**Communication.** Workers never mutate the shared world directly during their decision-making. Instead they send `Message` events (robot moved, obstacle found, resource found, resource depleted, resource deposited) over an `mpsc` channel. The main loop drains these messages into a single `Arc<Mutex<WorldState>>` per world, then takes a snapshot to render. This keeps rendering decoupled from the workers and avoids contention on the lock.

**The render loop.** The main thread processes pending messages, snapshots all four worlds, draws them, and polls for keyboard input roughly every 40 ms. Pressing **any key** stops the workers, restores the terminal, and exits cleanly.

## Requirements

- [Rust](https://www.rust-lang.org/tools/install) toolchain with **edition 2024** support (Rust 1.85 or newer).
- A terminal that supports raw mode and an alternate screen (most modern terminals do).

## Build and run

```bash
git clone https://github.com/moudjane/resource-collection-tui.git
cd resource-collection-tui
cargo run --release
```

Press any key to quit.

## Running the tests

```bash
cargo test
```

The suite covers world generation (resource amounts and presence), robot capabilities, and the movement heuristic that prefers fresh tiles over recently visited ones.

## Project structure

```
src/
├── main.rs              # Terminal setup, render loop, and input handling
├── model.rs             # World, map, tiles, robots, and message types
├── model_tests.rs       # Tests for map generation and robot roles
├── simulation.rs        # Worker threads, pathfinding, and per-world orchestration
├── simulation_tests.rs  # Tests for movement heuristics
└── ui.rs                # 2×2 grid layout and per-world rendering
```

## Configuration

A few constants let you reshape the simulation without touching the logic:

| Constant           | File            | Default | Description                       |
| ------------------ | --------------- | :-----: | --------------------------------- |
| `SIMULATION_COUNT` | `simulation.rs` |   `4`   | Number of worlds shown at once    |
| `MAP_WIDTH`        | `model.rs`      |  `60`   | World width in tiles              |
| `MAP_HEIGHT`       | `model.rs`      |  `24`   | World height in tiles             |
| `SCOUT_COUNT`      | `model.rs`      |   `3`   | Scouts per world                  |
| `COLLECTOR_COUNT`  | `model.rs`      |   `4`   | Collectors per world              |

> Note: the UI lays the worlds out in a fixed 2×2 grid, so values of `SIMULATION_COUNT` above 4 will not all be visible.

## Dependencies

- [`ratatui`](https://crates.io/crates/ratatui) — terminal UI rendering
- [`crossterm`](https://crates.io/crates/crossterm) — terminal backend and input
- [`noise`](https://crates.io/crates/noise) — Perlin noise for obstacle generation
- [`rand`](https://crates.io/crates/rand) — randomized maps and movement
