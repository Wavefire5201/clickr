use crate::app::AppState;
use evdev::{Device, Key};
use nix::poll::{PollFd, PollFlags, PollTimeout};
use std::collections::{HashMap, HashSet};
use std::os::unix::io::{AsRawFd, BorrowedFd};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const RESCAN_INTERVAL: Duration = Duration::from_secs(10);
const MAX_RETRY_BACKOFF: Duration = Duration::from_secs(600);

/// Device names are hardware-supplied strings headed for the terminal; strip control characters.
fn sanitize_name(raw: Option<&str>) -> String {
    raw.unwrap_or("unknown")
        .chars()
        .filter(|c| !c.is_control())
        .take(64)
        .collect()
}

enum ListenOutcome {
    Unopenable,
    Ended,
}

/// Enumerate keyboards and print results. Call BEFORE starting the TUI
/// so log output doesn't bleed into the alternate screen.
pub fn enumerate_keyboards() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    for (path, device) in evdev::enumerate() {
        if let Some(supported) = device.supported_keys()
            && supported.contains(Key::KEY_F6)
        {
            let name = sanitize_name(device.name());
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
    registry: Arc<Registry>,
    rescan_handle: Option<JoinHandle<()>>,
}

impl HotkeyHandles {
    pub fn join_all(self) {
        // Join rescan thread first (it stops spawning new listeners)
        if let Some(h) = self.rescan_handle {
            let _ = h.join();
        }
        // Then join all listener threads
        let handles = self
            .registry
            .handles
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .drain(..)
            .collect::<Vec<_>>();
        for h in handles {
            let _ = h.join();
        }
    }
}

struct Registry {
    handles: Mutex<Vec<JoinHandle<()>>>,
    active: Mutex<HashSet<PathBuf>>,
    /// (consecutive failures, earliest retry)
    failed: Mutex<HashMap<PathBuf, (u32, Instant)>>,
}

impl Registry {
    fn spawn_listener(self: &Arc<Self>, path: PathBuf, state: Arc<AppState>) {
        self.active
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(path.clone());
        let reg = Arc::clone(self);
        let handle = thread::spawn(move || {
            let outcome = listen_device(&path, state);
            match outcome {
                ListenOutcome::Unopenable => {
                    let mut failed = reg.failed.lock().unwrap_or_else(|e| e.into_inner());
                    let attempts = failed.get(&path).map_or(0, |(n, _)| *n) + 1;
                    let backoff = (RESCAN_INTERVAL * 2u32.saturating_pow(attempts.min(10)))
                        .min(MAX_RETRY_BACKOFF);
                    failed.insert(path.clone(), (attempts, Instant::now() + backoff));
                }
                ListenOutcome::Ended => {
                    reg.failed
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .remove(&path);
                }
            }
            // Free the path so a rescan can pick the device up again.
            reg.active
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&path);
        });
        let mut handles = self.handles.lock().unwrap_or_else(|e| e.into_inner());
        // Dropping finished handles detaches them; otherwise the list grows on every hotplug.
        handles.retain(|h| !h.is_finished());
        handles.push(handle);
    }

    fn should_spawn(&self, path: &PathBuf) -> bool {
        if self
            .active
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(path)
        {
            return false;
        }
        match self
            .failed
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(path)
        {
            Some((_, retry_at)) => Instant::now() >= *retry_at,
            None => true,
        }
    }
}

/// Spawn listener threads for the given keyboard paths. Call AFTER TUI init.
pub fn run(initial_paths: Vec<PathBuf>, state: Arc<AppState>) -> HotkeyHandles {
    let registry = Arc::new(Registry {
        handles: Mutex::new(Vec::new()),
        active: Mutex::new(HashSet::new()),
        failed: Mutex::new(HashMap::new()),
    });

    for path in initial_paths {
        registry.spawn_listener(path, Arc::clone(&state));
    }

    // Periodic rescan for hotplugged keyboards
    let rescan_state = Arc::clone(&state);
    let rescan_registry = Arc::clone(&registry);
    let rescan_handle = thread::spawn(move || {
        let mut last_scan = Instant::now();

        while !rescan_state.should_quit() {
            thread::sleep(Duration::from_millis(500));

            if last_scan.elapsed() < RESCAN_INTERVAL {
                continue;
            }
            last_scan = Instant::now();

            for (path, _) in find_keyboards() {
                if rescan_registry.should_spawn(&path) {
                    rescan_registry.spawn_listener(path, Arc::clone(&rescan_state));
                }
            }
        }
    });

    HotkeyHandles {
        registry,
        rescan_handle: Some(rescan_handle),
    }
}

fn find_keyboards() -> Vec<(PathBuf, String)> {
    let mut keyboards = Vec::new();

    for (path, device) in evdev::enumerate() {
        if let Some(supported) = device.supported_keys()
            && supported.contains(Key::KEY_F6)
        {
            keyboards.push((path, sanitize_name(device.name())));
        }
    }

    keyboards
}

fn listen_device(path: &PathBuf, state: Arc<AppState>) -> ListenOutcome {
    let Ok(mut device) = Device::open(path) else {
        return ListenOutcome::Unopenable;
    };

    let mut consecutive_errors = 0u32;

    loop {
        if state.should_quit() {
            return ListenOutcome::Ended;
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
                    return ListenOutcome::Ended;
                }
                continue;
            }
        }

        if let Some(revents) = poll_fds[0].revents() {
            // Device disconnected or errored — exit this listener
            if revents.intersects(PollFlags::POLLERR | PollFlags::POLLHUP | PollFlags::POLLNVAL) {
                return ListenOutcome::Ended;
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
                    let settings = state.settings.lock().unwrap_or_else(|e| e.into_inner());
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
                    return ListenOutcome::Ended;
                }
            }
        }
    }
}
