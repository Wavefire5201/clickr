# clickr

A fast, lightweight autoclicker for Linux Wayland (Hyprland, Sway, etc.) with a TUI interface.

Uses kernel-level uinput to create a virtual mouse device — works on any Wayland compositor without special protocol support.

## Requirements

- Linux with uinput support
- User must be in the `input` group (see Security below)
- A notification daemon (dunst, mako, swaync) for toggle notifications

## Build

```bash
cargo build --release
```

Binary at `target/release/clickr`.

## Usage

```bash
./target/release/clickr
```

## Controls

| Key | Action |
|-----|--------|
| **F6** | Toggle on/off (global hotkey — works outside TUI) |
| **Space** | Toggle on/off (in TUI) |
| **S** | Set CPS manually (type a number, Enter to confirm) |
| **Up/Down** | Adjust clicks per second (1–1000) |
| **B** | Cycle mouse button (left/right/middle) |
| **M** | Cycle click mode (click/double/hold) |
| **J** | Toggle jitter (randomized interval) |
| **Left/Right** | Adjust jitter amount (±1–100ms) |
| **H** | Cycle hotkey (F6–F12) |
| **R** | Reset click counter |
| **Q / Esc** | Quit |

## Features

- **Global hotkey** — toggle from any window via F6–F12, auto-detects hotplugged keyboards
- **1–1000 CPS** — adaptive speed stepping with log-scale gauge
- **Click modes** — single click, double click, hold
- **Jitter** — random ±ms variance for human-like clicking
- **Desktop notifications** — popup on toggle
- **Wayland native** — uinput bypasses display server entirely
- **Clean shutdown** — all buttons released on exit, threads joined

## How it works

Three threads:
1. **TUI** (main) — renders interface, handles keyboard input
2. **Clicker** — creates a virtual mouse via `/dev/uinput`, emits click events
3. **Hotkey** — reads keyboard devices via evdev, detects toggle key, rescans for new devices

The virtual mouse appears as real hardware to the kernel, so every compositor processes it natively.

## Security

**clickr requires membership in the `input` group.** This is a sensitive privilege — it grants:

- **Read access** to `/dev/input/event*` — the hotkey listener reads raw keyboard events from all keyboards. This is the same level of access a keylogger would need.
- **Write access** to `/dev/uinput` — the clicker creates a virtual mouse that can inject clicks into any application.

This is inherent to how any Wayland autoclicker must work (Wayland blocks client-to-client input injection by design, so kernel-level access is the only option).

**Mitigations built in:**
- Refuses to run as root or setuid
- Only reads F-key events for hotkey detection, does not log other keys
- Virtual device is named `clickr virtual mouse` for easy identification
- All buttons are force-released on shutdown

**To set up:**
```bash
sudo usermod -aG input $USER
```
Log out and back in for the group change to take effect. Only grant this to users you trust with full input access.
