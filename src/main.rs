mod app;
mod clicker;
mod hotkey;
mod ui;

use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread;

/// An elevated binary would hand keyboard and uinput access to every user on the machine.
fn refuse_if_privileged() {
    use nix::unistd::{getegid, geteuid, getgid, getuid};
    // AT_SECURE covers setuid, setgid, file capabilities, and LSM transitions.
    // SAFETY: getauxval takes no pointers.
    let at_secure = unsafe { nix::libc::getauxval(nix::libc::AT_SECURE) } != 0;
    if geteuid().is_root() || getuid() != geteuid() || getgid() != getegid() || at_secure {
        eprintln!("Error: clickr must not run as root, setuid, setgid, or with file capabilities.");
        eprintln!("Add your user to the 'input' group instead: sudo usermod -aG input $USER");
        std::process::exit(1);
    }
}

/// Buffered key events live in this process's memory. Non-dumpable means no
/// core dumps and no same-user reads of /proc/self/mem or ptrace attach.
fn protect_process_memory() {
    // SAFETY: prctl(PR_SET_DUMPABLE, 0) takes no pointers.
    unsafe { nix::libc::prctl(nix::libc::PR_SET_DUMPABLE, 0, 0, 0, 0) };
}

extern "C" fn on_signal(_: nix::libc::c_int) {
    app::SIGNAL_QUIT.store(true, Ordering::Release);
}

fn install_signal_handlers() {
    use nix::sys::signal::{SigHandler, Signal, signal};
    for sig in [Signal::SIGINT, Signal::SIGTERM, Signal::SIGHUP] {
        // SAFETY: the handler only stores to an atomic.
        let _ = unsafe { signal(sig, SigHandler::Handler(on_signal)) };
    }
}

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

    refuse_if_privileged();
    protect_process_memory();
    install_signal_handlers();

    // Enumerate keyboards BEFORE TUI starts so logs go to normal stderr
    let keyboard_paths = hotkey::enumerate_keyboards();

    let state = app::AppState::new();

    // Spawn clicker thread (uinput virtual mouse)
    let clicker_state = Arc::clone(&state);
    let clicker_handle = thread::spawn(move || {
        // A clicker panic must stop the app, not leave a live TUI over a dead clicker.
        let result = catch_unwind(AssertUnwindSafe(|| {
            clicker::run(Arc::clone(&clicker_state))
        }));
        clicker_state.quit();
        if let Err(payload) = result {
            resume_unwind(payload);
        }
    });

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
    if clicker_handle.join().is_err() {
        eprintln!("Error: clicker thread panicked; see message above.");
    }
    hotkey_handles.join_all();
}
