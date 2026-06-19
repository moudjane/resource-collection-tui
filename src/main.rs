use std::io;
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

use simulation::*;
use ui::draw_ui;

fn run_app() -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let simulations: Vec<SimulationInstance> = (0..SIMULATION_COUNT)
        .map(|_| SimulationInstance::spawn())
        .collect();

    let mut exit_requested = false;
    while !exit_requested {
        for simulation in &simulations {
            simulation.process_messages();
        }
        let snapshots: Vec<_> = simulations.iter().map(SimulationInstance::snapshot).collect();
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
