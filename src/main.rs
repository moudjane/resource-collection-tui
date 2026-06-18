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
use ratatui::Terminal;

mod model;
mod simulation;
mod ui;

use model::*;
use simulation::*;
use ui::draw_ui;

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
