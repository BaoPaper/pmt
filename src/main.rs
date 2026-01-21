mod app;
mod models;
mod parser;
mod system;
mod ui;

use std::io;
use std::time::Duration;

use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind};
use crossterm::execute;
use ratatui::DefaultTerminal;

use crate::app::App;
use crate::ui::render_app;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let terminal = ratatui::init();
    execute!(io::stdout(), EnableMouseCapture)?;

    let app = App::load();
    let result = run_app(terminal, app);

    execute!(io::stdout(), DisableMouseCapture)?;
    ratatui::restore();
    result
}

fn run_app(mut terminal: DefaultTerminal, mut app: App) -> Result<(), Box<dyn std::error::Error>> {
    let tick_rate = Duration::from_millis(100);
    loop {
        if app.needs_redraw {
            terminal.clear()?;
            app.needs_redraw = false;
        }
        terminal.draw(|frame| render_app(frame, &mut app))?;

        if app.should_quit {
            break;
        }

        if event::poll(tick_rate)? {
            match event::read()? {
                Event::Key(key) => {
                    if key.kind == KeyEventKind::Press {
                        app.on_key(key);
                    }
                }
                Event::Mouse(mouse) => app.on_mouse(mouse),
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
    }
    Ok(())
}
