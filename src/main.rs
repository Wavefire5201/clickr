mod app;
mod clicker;
mod hotkey;
mod ui;

use std::sync::Arc;
use std::thread;

fn main() {
    if let Some(arg) = std::env::args().nth(1) {
        match arg.as_str() {
            "--version" | "-V" => {
                println!("clickr {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            "--help" | "-h" => {
                println!("clickr {}", env!("CARGO_PKG_VERSION"));
                println!("{}", env!("CARGO_PKG_DESCRIPTION"));
                println!();
                println!("Usage: clickr [--version | --help]");
                println!();
                println!("Runs the TUI. Requires membership in the 'input' group.");
                return;
            }
            other => {
                eprintln!("clickr: unknown argument '{other}'");
                eprintln!("Usage: clickr [--version | --help]");
                std::process::exit(2);
            }
        }
    }

    // Refuse to run as root or setuid — use input group instead
    if nix::unistd::geteuid().is_root() {
        eprintln!("Error: clickr should not be run as root or setuid.");
        eprintln!("Add your user to the 'input' group instead: sudo usermod -aG input $USER");
        std::process::exit(1);
    }

    // Enumerate keyboards BEFORE TUI starts so logs go to normal stderr
    let keyboard_paths = hotkey::enumerate_keyboards();

    let state = app::AppState::new();

    // Spawn clicker thread (uinput virtual mouse)
    let clicker_state = Arc::clone(&state);
    let clicker_handle = thread::spawn(move || clicker::run(clicker_state));

    // Small delay to let uinput device register
    thread::sleep(std::time::Duration::from_millis(100));

    // Spawn hotkey listener threads (evdev keyboard readers)
    let hotkey_state = Arc::clone(&state);
    let hotkey_handles = hotkey::run(keyboard_paths, hotkey_state);

    // Run TUI on main thread (blocks until quit)
    if let Err(e) = ui::run(Arc::clone(&state)) {
        ratatui::restore();
        eprintln!("TUI error: {e}");
    }

    // Signal all threads to stop and wait for clean shutdown
    state.quit();
    let _ = clicker_handle.join();
    hotkey_handles.join_all();
}
