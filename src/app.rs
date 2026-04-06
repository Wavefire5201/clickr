use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
}

impl MouseButton {
    pub fn next(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Middle,
            Self::Middle => Self::Left,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Left => "Left",
            Self::Right => "Right",
            Self::Middle => "Middle",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickMode {
    Click,
    DoubleClick,
    Hold,
}

impl ClickMode {
    pub fn next(self) -> Self {
        match self {
            Self::Click => Self::DoubleClick,
            Self::DoubleClick => Self::Hold,
            Self::Hold => Self::Click,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Click => "Click",
            Self::DoubleClick => "Double",
            Self::Hold => "Hold",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct HotkeyBinding {
    pub code: u16,
    pub label: &'static str,
}

// linux/input-event-codes.h: F6=64, F7=65, F8=66, F9=67, F10=68, F11=87, F12=88
pub const HOTKEY_OPTIONS: &[HotkeyBinding] = &[
    HotkeyBinding { code: 64, label: "F6" },
    HotkeyBinding { code: 65, label: "F7" },
    HotkeyBinding { code: 66, label: "F8" },
    HotkeyBinding { code: 67, label: "F9" },
    HotkeyBinding { code: 68, label: "F10" },
    HotkeyBinding { code: 87, label: "F11" },
    HotkeyBinding { code: 88, label: "F12" },
];

pub struct AppState {
    pub active: AtomicBool,
    pub click_count: AtomicU64,
    pub should_quit: AtomicBool,
    pub settings: Mutex<Settings>,
    last_toggle: Mutex<Option<Instant>>,
}

pub struct Settings {
    pub cps: u32,
    pub button: MouseButton,
    pub mode: ClickMode,
    pub jitter_enabled: bool,
    pub jitter_ms: u32,
    pub hotkey_index: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            cps: 20,
            button: MouseButton::Left,
            mode: ClickMode::Click,
            jitter_enabled: false,
            jitter_ms: 5,
            hotkey_index: 0, // F6
        }
    }
}

impl Settings {
    pub fn hotkey(&self) -> &HotkeyBinding {
        &HOTKEY_OPTIONS[self.hotkey_index]
    }

    pub fn interval_ms(&self) -> f64 {
        1000.0 / self.cps as f64
    }
}

const TOGGLE_DEBOUNCE_MS: u128 = 100;

impl AppState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            active: AtomicBool::new(false),
            click_count: AtomicU64::new(0),
            should_quit: AtomicBool::new(false),
            settings: Mutex::new(Settings::default()),
            // None = no previous toggle, so first toggle always succeeds
            last_toggle: Mutex::new(None),
        })
    }

    pub fn toggle(&self) {
        // Debounce: ignore toggles within 100ms (multiple keyboard devices
        // can report the same keypress nearly simultaneously)
        {
            let mut last = self.last_toggle.lock().expect("last_toggle lock");
            let now = Instant::now();
            if let Some(prev) = *last
                && now.duration_since(prev).as_millis() < TOGGLE_DEBOUNCE_MS
            {
                return;
            }
            *last = Some(now);
        }

        let was_active = self.active.fetch_xor(true, Ordering::AcqRel);
        self.notify(!was_active);
    }

    /// Force-deactivate without debounce. Used for programmatic stops (e.g. emit failure).
    pub fn force_deactivate(&self) {
        self.active.store(false, Ordering::Release);
        self.notify(false);
    }

    fn notify(&self, active: bool) {
        let (summary, body, icon) = if active {
            ("clickr — Active", "Autoclicker started", "input-mouse")
        } else {
            ("clickr — Stopped", "Autoclicker stopped", "process-stop")
        };

        std::thread::spawn(move || {
            let _ = notify_rust::Notification::new()
                .summary(summary)
                .body(body)
                .icon(icon)
                .timeout(1500)
                .show();
        });
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }

    pub fn increment_clicks(&self) {
        self.click_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn click_count(&self) -> u64 {
        self.click_count.load(Ordering::Relaxed)
    }

    pub fn reset_clicks(&self) {
        self.click_count.store(0, Ordering::Relaxed);
    }

    pub fn quit(&self) {
        self.should_quit.store(true, Ordering::Release);
    }

    pub fn should_quit(&self) -> bool {
        self.should_quit.load(Ordering::Acquire)
    }
}
