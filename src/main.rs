use std::io;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crossterm::event::{self, Event};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

mod model;
mod simulation;
mod ui;

use simulation::*;
use ui::draw_ui;

fn run_app() -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (tx, _rx) = mpsc::channel();

    let simulations: Vec<SimulationInstance> = (0..SIMULATION_COUNT - 1)
        .map(|_| SimulationInstance::spawn(None))
        .chain(std::iter::once(SimulationInstance::spawn(Some(tx))))
        .collect();

    let mut exit_requested = false;
    while !exit_requested {
        for simulation in &simulations {
            simulation.process_messages();
        }
        let snapshots: Vec<_> = simulations
            .iter()
            .map(SimulationInstance::snapshot)
            .collect();

        terminal.draw(|f| draw_ui(f, &snapshots))?;
        if event::poll(Duration::from_millis(20))? && matches!(event::read()?, Event::Key(_)) {
            exit_requested = true;
        }
        thread::sleep(Duration::from_millis(40));
    }

    for simulation in simulations {
        simulation.stop();
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
