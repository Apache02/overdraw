//! overdraw: draw on top of the screen.
//!
//! The first `overdraw` starts in the tray. Running `overdraw` again (or
//! `overdraw --toggle`) shows or hides the canvas in the running instance, so a
//! desktop keyboard shortcut bound to that command works as a global hotkey.

mod render;
mod state;
mod ui;

use std::cell::RefCell;
use std::rc::Rc;

use gtk::gio::{ApplicationCommandLine, ApplicationFlags};
use gtk::glib::{self, ExitCode};
use gtk::prelude::*;

use ui::Ui;

const APP_ID: &str = "io.github.overdraw";
const USAGE: &str = "Usage: overdraw [--toggle | --show | --hide | --quit]

Without arguments: start in the tray, or toggle the canvas if already running.";

thread_local! {
    static UI: RefCell<Option<Rc<Ui>>> = const { RefCell::new(None) };
    static TRAY: RefCell<Option<glib::Object>> = const { RefCell::new(None) };
}

/// Runs in the primary instance, also for command lines forwarded from a second one.
fn on_command_line(app: &gtk::Application, cmd: &ApplicationCommandLine) -> i32 {
    let args: Vec<String> =
        cmd.arguments().iter().skip(1).map(|a| a.to_string_lossy().into_owned()).collect();
    let Some(ui) = UI.with(|u| u.borrow().clone()) else { return 1 };

    match args.first().map(String::as_str) {
        None if cmd.is_remote() => ui.toggle(),
        None => {}
        Some("--toggle") => ui.toggle(),
        Some("--show") => ui.show(),
        Some("--hide") => ui.hide(),
        Some("--quit") => app.quit(),
        Some("-h" | "--help") => println!("{USAGE}"),
        Some(other) => {
            eprintln!("overdraw: unknown option {other}\n{USAGE}");
            return 1;
        }
    }
    0
}

fn main() -> ExitCode {
    let app = gtk::Application::builder()
        .application_id(APP_ID)
        .flags(ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();

    app.connect_startup(|app| {
        // Stay alive in the tray while no window is shown.
        std::mem::forget(app.hold());
        let ui = Ui::new(app);
        TRAY.with(|t| *t.borrow_mut() = Some(Ui::install_tray(&ui)));
        UI.with(|u| *u.borrow_mut() = Some(ui));
    });
    app.connect_command_line(on_command_line);

    glib::set_prgname(Some("overdraw"));
    app.run()
}
