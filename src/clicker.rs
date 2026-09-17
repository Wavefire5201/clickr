use crate::app::{AppState, ClickMode, MouseButton};
use evdev::uinput::VirtualDeviceBuilder;
use evdev::{AttributeSet, InputEvent, Key, RelativeAxisType};
use rand::Rng;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn mouse_key(button: MouseButton) -> Key {
    match button {
        MouseButton::Left => Key::BTN_LEFT,
        MouseButton::Right => Key::BTN_RIGHT,
        MouseButton::Middle => Key::BTN_MIDDLE,
    }
}

pub fn run(state: Arc<AppState>) {
    let mut device = match create_virtual_mouse() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Failed to create virtual mouse: {e}");
            eprintln!("Make sure you're in the 'input' group: sudo usermod -aG input $USER");
            state.quit();
            return;
        }
    };

    while !state.should_quit() {
        if !state.is_active() {
            thread::sleep(Duration::from_millis(10));
            continue;
        }

        let (interval_ms, button, mode, jitter_enabled, jitter_ms) = {
            let settings = state.settings.lock().unwrap_or_else(|e| e.into_inner());
            (
                settings.interval_ms(),
                settings.button,
                settings.mode,
                settings.jitter_enabled,
                settings.jitter_ms,
            )
        };

        match mode {
            ClickMode::Click => {
                if !click(&mut device, button) {
                    // uinput emit failed — user gets notified via force_deactivate toast
                    release_all(&mut device);
                    state.force_deactivate();
                    continue;
                }
                state.increment_clicks();
            }
            ClickMode::DoubleClick => {
                if !click(&mut device, button) {
                    // uinput emit failed — user gets notified via force_deactivate toast
                    release_all(&mut device);
                    state.force_deactivate();
                    continue;
                }
                // Re-check active/quit before and after the inter-click delay
                if !state.is_active() || state.should_quit() {
                    state.increment_clicks();
                    continue;
                }
                thread::sleep(Duration::from_millis(30));
                if !state.is_active() || state.should_quit() {
                    state.increment_clicks();
                    continue;
                }
                if !click(&mut device, button) {
                    // uinput emit failed — user gets notified via force_deactivate toast
                    release_all(&mut device);
                    state.force_deactivate();
                    continue;
                }
                state.increment_clicks();
            }
            ClickMode::Hold => {
                if !press(&mut device, button) {
                    // uinput emit failed — user gets notified via force_deactivate toast
                    release_all(&mut device);
                    state.force_deactivate();
                    continue;
                }
                state.increment_clicks();
                // Stay held until deactivated, quit, or settings change
                while state.is_active() && !state.should_quit() {
                    let settings = state.settings.lock().unwrap_or_else(|e| e.into_inner());
                    let changed = settings.button != button || settings.mode != mode;
                    drop(settings);
                    if changed {
                        break;
                    }
                    thread::sleep(Duration::from_millis(10));
                }
                release_all(&mut device);
                continue;
            }
        }

        let sleep_ms = if jitter_enabled {
            let jitter = rand::thread_rng().gen_range(-(jitter_ms as f64)..=jitter_ms as f64);
            (interval_ms + jitter).max(1.0)
        } else {
            interval_ms
        };

        thread::sleep(Duration::from_micros((sleep_ms * 1000.0) as u64));
    }

    // Clean up: release all buttons on shutdown
    release_all(&mut device);
}

fn create_virtual_mouse() -> std::io::Result<evdev::uinput::VirtualDevice> {
    let mut keys = AttributeSet::<Key>::new();
    keys.insert(Key::BTN_LEFT);
    keys.insert(Key::BTN_RIGHT);
    keys.insert(Key::BTN_MIDDLE);

    let mut rel_axes = AttributeSet::<RelativeAxisType>::new();
    rel_axes.insert(RelativeAxisType::REL_X);
    rel_axes.insert(RelativeAxisType::REL_Y);

    VirtualDeviceBuilder::new()?
        .name("clickr virtual mouse")
        .with_keys(&keys)?
        .with_relative_axes(&rel_axes)?
        .build()
}

fn emit(device: &mut evdev::uinput::VirtualDevice, events: &[InputEvent]) -> bool {
    device.emit(events).is_ok()
}

fn click(device: &mut evdev::uinput::VirtualDevice, button: MouseButton) -> bool {
    let key = mouse_key(button);
    let press = InputEvent::new(evdev::EventType::KEY, key.code(), 1);
    let release = InputEvent::new(evdev::EventType::KEY, key.code(), 0);
    let syn = InputEvent::new(evdev::EventType::SYNCHRONIZATION, 0, 0);
    // If press succeeds but release fails, force release_all at the call site
    emit(device, &[press, syn]) && emit(device, &[release, syn])
}

fn press(device: &mut evdev::uinput::VirtualDevice, button: MouseButton) -> bool {
    let key = mouse_key(button);
    let press = InputEvent::new(evdev::EventType::KEY, key.code(), 1);
    let syn = InputEvent::new(evdev::EventType::SYNCHRONIZATION, 0, 0);
    emit(device, &[press, syn])
}

fn release(device: &mut evdev::uinput::VirtualDevice, button: MouseButton) {
    let key = mouse_key(button);
    let release = InputEvent::new(evdev::EventType::KEY, key.code(), 0);
    let syn = InputEvent::new(evdev::EventType::SYNCHRONIZATION, 0, 0);
    emit(device, &[release, syn]);
}

fn release_all(device: &mut evdev::uinput::VirtualDevice) {
    release(device, MouseButton::Left);
    release(device, MouseButton::Right);
    release(device, MouseButton::Middle);
}
