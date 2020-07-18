mod event;
mod tcpdiag;
mod cli;
mod table;

use cli::CLI;
use event::{Event, Events};
use std::{error::Error, io};
use termion::{event::Key, input::MouseTerminal, raw::IntoRawMode, screen::AlternateScreen};
use std::panic::{self, PanicInfo};
use backtrace::Backtrace;
use tui::{
    backend::TermionBackend,
    Terminal,
};

fn panic_hook(info: &PanicInfo<'_>) {
    if cfg!(debug_assertions) {
        let location = info.location().unwrap();

        let msg = match info.payload().downcast_ref::<&'static str>() {
            Some(s) => *s,
            None => match info.payload().downcast_ref::<String>() {
                Some(s) => &s[..],
                None => "Box<Any>",
            },
        };

        let stacktrace: String = format!("{:?}", Backtrace::new()).replace('\n', "\n\r");

        println!(
            "{}thread '<unnamed>' panicked at '{}', {}\n\r{}",
            termion::screen::ToMainScreen,
            msg,
            location,
            stacktrace
        );
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    panic::set_hook(Box::new(|info| {
        panic_hook(info);
    }));

    // Terminal initialization
    let stdout = io::stdout().into_raw_mode()?;
    let stdout = MouseTerminal::from(stdout);
    let stdout = AlternateScreen::from(stdout);
    let backend = TermionBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.hide_cursor()?;

    let events = Events::new();
    let mut app = CLI::new();

    // Input
    loop {
        terminal.draw(|mut f| app.render(&mut f))?;

        match events.next()? {
            Event::Input(key) => match key {
                Key::Char('q') => {
                    break;
                }
                Key::Down | Key::Char('j') => {
                    app.overview.next();
                }
                Key::Up | Key::Char('k') => {
                    app.overview.previous();
                }
                Key::Char('\n') => {
                    app.enter_detail_view(); 
                }
                Key::Char('b') => {
                    app.exit_detail_view(); 
                }
                _ => {}
            },
            Event::Tick => {
                app.on_tick();
            }
        };
    }

    Ok(())
}
