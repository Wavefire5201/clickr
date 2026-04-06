use crate::app::AppState;
use evdev::{Device, Key};
use nix::poll::{PollFd, PollFlags, PollTimeout};
use std::collections::HashSet;
use std::os::unix::io::{AsRawFd, BorrowedFd};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const RESCAN_INTERVAL: Duration = Duration::from_secs(10);

/// Enumerate keyboards and print results. Call BEFORE starting the TUI
/// so log output doesn't bleed into the alternate screen.
pub fn enumerate_keyboards() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    for (path, device) in evdev::enumerate() {
        if let Some(supported) = device.supported_keys()
            && supported.contains(Key::KEY_F6)
        {
            let name = device.name().unwrap_or("unknown").to_string();
            eprintln!("Hotkey: listening on {name} ({path:?})");
            paths.push(path);
        }
    }

    if paths.is_empty() {
        eprintln!("Warning: No keyboard devices found for hotkey listening.");
        eprintln!("Make sure you're in the 'input' group.");
    }

    paths
}

/// Tracks all spawned listener threads so they can be joined on shutdown.
pub struct HotkeyHandles {
    handles: Arc<Mutex<Vec<JoinHandle<()>>>>,
    rescan_handle: Option<JoinHandle<()>>,
}

impl HotkeyHandles {
    pub fn join_all(self) {
        // Join rescan thread first (it stops spawning new listeners)
        if let Some(h) = self.rescan_handle {
            let _ = h.join();
        }
        // Then join all listener threads
        let handles = self.handles.lock().expect("handles lock").drain(..).collect::<Vec<_>>();
        for h in handles {
            let _ = h.join();
        }
    }
}

/// Spawn listener threads for the given keyboard paths. Call AFTER TUI init.
pub fn run(initial_paths: Vec<PathBuf>, state: Arc<AppState>) -> HotkeyHandles {
    let handles: Arc<Mutex<Vec<JoinHandle<()>>>> = Arc::new(Mutex::new(Vec::new()));
    // Track active listener paths — listeners remove themselves on exit so rescan can retry
    let active_paths: Arc<Mutex<HashSet<PathBuf>>> = Arc::new(Mutex::new(HashSet::new()));

    for path in &initial_paths {
        active_paths.lock().expect("active_paths lock").insert(path.clone());
        let s = Arc::clone(&state);
        let p = path.clone();
        let ap = Arc::clone(&active_paths);
        handles.lock().expect("handles lock").push(thread::spawn(move || {
            listen_device(&p, s);
            // Remove path so rescan can retry if device reappears
            ap.lock().expect("active_paths lock").remove(&p);
        }));
    }

    // Periodic rescan for hotplugged keyboards
    let rescan_state = Arc::clone(&state);
    let rescan_handles = Arc::clone(&handles);
    let rescan_active = Arc::clone(&active_paths);
    let rescan_handle = thread::spawn(move || {
        let mut last_scan = Instant::now();

        while !rescan_state.should_quit() {
            thread::sleep(Duration::from_millis(500));

            if last_scan.elapsed() < RESCAN_INTERVAL {
                continue;
            }
            last_scan = Instant::now();

            for (path, _) in find_keyboards() {
                let already_active = rescan_active.lock().expect("active_paths lock").contains(&path);
                if !already_active {
                    rescan_active.lock().expect("active_paths lock").insert(path.clone());
                    let s = Arc::clone(&rescan_state);
                    let ap = Arc::clone(&rescan_active);
                    let p = path.clone();
                    let h = thread::spawn(move || {
                        listen_device(&p, s);
                        ap.lock().expect("active_paths lock").remove(&p);
                    });
                    rescan_handles.lock().expect("handles lock").push(h);
                }
            }
        }
    });

    HotkeyHandles {
        handles,
        rescan_handle: Some(rescan_handle),
    }
}

fn find_keyboards() -> Vec<(PathBuf, String)> {
    let mut keyboards = Vec::new();

    for (path, device) in evdev::enumerate() {
        if let Some(supported) = device.supported_keys()
            && supported.contains(Key::KEY_F6)
        {
            let name = device.name().unwrap_or("unknown").to_string();
            keyboards.push((path, name));
        }
    }

    keyboards
}

fn listen_device(path: &PathBuf, state: Arc<AppState>) {
    let Ok(mut device) = Device::open(path) else {
        return;
    };

    let mut consecutive_errors = 0u32;

    loop {
        if state.should_quit() {
            return;
        }

        // SAFETY: device owns the fd and outlives the BorrowedFd (scoped to this loop iteration)
        let borrowed = unsafe { BorrowedFd::borrow_raw(device.as_raw_fd()) };
        let mut poll_fds = [PollFd::new(borrowed, PollFlags::POLLIN)];

        match nix::poll::poll(&mut poll_fds, PollTimeout::from(100u16)) {
            Ok(n) if n > 0 => {}
            Ok(_) => continue,
            Err(_) => {
                consecutive_errors += 1;
                if consecutive_errors >= 50 {
                    return;
                }
                continue;
            }
        }

        if let Some(revents) = poll_fds[0].revents() {
            // Device disconnected or errored — exit this listener
            if revents.intersects(PollFlags::POLLERR | PollFlags::POLLHUP | PollFlags::POLLNVAL) {
                return;
            }
            if !revents.contains(PollFlags::POLLIN) {
                continue;
            }
        } else {
            continue;
        }

        match device.fetch_events() {
            Ok(events) => {
                consecutive_errors = 0;

                let hotkey_code = {
                    let settings = state.settings.lock().expect("settings lock");
                    settings.hotkey().code
                };

                for event in events {
                    if event.event_type() == evdev::EventType::KEY
                        && event.value() == 1
                        && event.code() == hotkey_code
                    {
                        state.toggle();
                    }
                }
            }
            Err(_) => {
                consecutive_errors += 1;
                if consecutive_errors >= 50 {
                    return;
                }
            }
        }
    }
}
