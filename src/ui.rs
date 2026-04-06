use crate::app::{AppState, HOTKEY_OPTIONS};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Direction, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Gauge, Paragraph};
use ratatui::Frame;
use std::sync::Arc;
use std::time::Duration;

const TUI_WIDTH: u16 = 60;
const TUI_HEIGHT: u16 = 18;

struct UiState {
    cps_input: Option<String>, // Some = editing CPS, None = normal mode
}

pub fn run(state: Arc<AppState>) -> std::io::Result<()> {
    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, &state);
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut ratatui::DefaultTerminal, state: &Arc<AppState>) -> std::io::Result<()> {
    let mut ui = UiState { cps_input: None };

    while !state.should_quit() {
        terminal.draw(|frame| render(frame, state, &ui))?;

        if event::poll(Duration::from_millis(50))?
            && let Event::Key(key) = event::read()?
        {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            handle_key(key.code, state, &mut ui);
        }
    }
    Ok(())
}

fn handle_key(code: KeyCode, state: &AppState, ui: &mut UiState) {
    // CPS input mode
    if let Some(ref mut buf) = ui.cps_input {
        match code {
            KeyCode::Enter => {
                if let Ok(val) = buf.parse::<u32>() {
                    let clamped = val.clamp(1, 1000);
                    state.settings.lock().expect("settings lock").cps = clamped;
                }
                ui.cps_input = None;
            }
            KeyCode::Esc => {
                ui.cps_input = None;
            }
            KeyCode::Backspace => {
                buf.pop();
            }
            KeyCode::Char(c) if c.is_ascii_digit() && buf.len() < 4 => {
                buf.push(c);
            }
            _ => {}
        }
        return;
    }

    // Normal mode
    match code {
        KeyCode::Char('q') | KeyCode::Esc => state.quit(),
        KeyCode::Char(' ') => state.toggle(),
        KeyCode::Char('r') | KeyCode::Char('R') => state.reset_clicks(),
        KeyCode::Char('s') | KeyCode::Char('S') => {
            ui.cps_input = Some(String::new());
        }
        KeyCode::Up => {
            let mut s = state.settings.lock().expect("settings lock");
            if s.cps < 1000 {
                s.cps = (s.cps + speed_step(s.cps)).min(1000);
            }
        }
        KeyCode::Down => {
            let mut s = state.settings.lock().expect("settings lock");
            if s.cps > 1 {
                s.cps = s.cps.saturating_sub(speed_step(s.cps.saturating_sub(1))).max(1);
            }
        }
        KeyCode::Char('b') | KeyCode::Char('B') => {
            let mut s = state.settings.lock().expect("settings lock");
            s.button = s.button.next();
        }
        KeyCode::Char('m') | KeyCode::Char('M') => {
            let mut s = state.settings.lock().expect("settings lock");
            s.mode = s.mode.next();
        }
        KeyCode::Char('j') | KeyCode::Char('J') => {
            let mut s = state.settings.lock().expect("settings lock");
            s.jitter_enabled = !s.jitter_enabled;
        }
        KeyCode::Char('h') | KeyCode::Char('H') => {
            let mut s = state.settings.lock().expect("settings lock");
            s.hotkey_index = (s.hotkey_index + 1) % HOTKEY_OPTIONS.len();
        }
        KeyCode::Right => {
            let mut s = state.settings.lock().expect("settings lock");
            if s.jitter_ms < 100 {
                s.jitter_ms += 1;
            }
        }
        KeyCode::Left => {
            let mut s = state.settings.lock().expect("settings lock");
            if s.jitter_ms > 1 {
                s.jitter_ms -= 1;
            }
        }
        _ => {}
    }
}

/// Adaptive speed step: bigger jumps at higher CPS values
fn speed_step(cps: u32) -> u32 {
    match cps {
        0..=10 => 1,
        11..=50 => 5,
        51..=200 => 10,
        _ => 50,
    }
}

/// Center a fixed-size rect within the terminal
fn centered_rect(area: Rect) -> Rect {
    let w = TUI_WIDTH.min(area.width);
    let h = TUI_HEIGHT.min(area.height);

    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(w)])
        .flex(Flex::Center)
        .split(area);

    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(h)])
        .flex(Flex::Center)
        .split(horizontal[0]);

    vertical[0]
}

fn render(frame: &mut Frame, state: &AppState, ui: &UiState) {
    // Clear full screen background
    frame.render_widget(Clear, frame.area());
    frame.render_widget(
        Block::default().style(Style::default().bg(Color::Black)),
        frame.area(),
    );

    let area = centered_rect(frame.area());

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),  // status
            Constraint::Length(3),  // gauge
            Constraint::Length(6),  // settings
            Constraint::Length(3),  // footer
        ])
        .split(area);

    render_status(frame, chunks[0], state);
    render_gauge(frame, chunks[1], state, ui);
    render_settings(frame, chunks[2], state);
    render_footer(frame, chunks[3], state, ui);
}

fn render_status(frame: &mut Frame, area: Rect, state: &AppState) {
    let active = state.is_active();
    let count = state.click_count();

    let (indicator, color) = if active {
        ("  ▶ CLICKING", Color::Green)
    } else {
        ("  ■ IDLE", Color::DarkGray)
    };

    let lines = vec![
        Line::from(Span::styled(
            indicator,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            Span::raw("  Clicks: "),
            Span::styled(
                format_count(count),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            ),
            if count > 0 {
                Span::styled("  [R]reset", Style::default().fg(Color::DarkGray))
            } else {
                Span::raw("")
            },
        ]),
    ];

    let border_color = if active { Color::Green } else { Color::DarkGray };
    let block = Block::default()
        .title(" clickr ")
        .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_gauge(frame: &mut Frame, area: Rect, state: &AppState, ui: &UiState) {
    let settings = state.settings.lock().expect("settings lock");
    let cps = settings.cps;
    let ratio = (cps as f64).ln_1p() / (1000_f64).ln_1p();

    let label = if let Some(buf) = &ui.cps_input {
        format!("CPS: {}▏ (Enter to confirm, Esc to cancel)", buf)
    } else {
        format!("{} CPS · {:.1}ms · [S]et", cps, settings.interval_ms())
    };

    let color = match cps {
        0..=10 => Color::Blue,
        11..=50 => Color::Cyan,
        51..=200 => Color::Yellow,
        _ => Color::Red,
    };

    let title = if ui.cps_input.is_some() {
        " Set CPS "
    } else {
        " Speed [↑/↓/S] "
    };

    let border_color = if ui.cps_input.is_some() {
        Color::Yellow
    } else {
        Color::DarkGray
    };

    let gauge = Gauge::default()
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color)),
        )
        .gauge_style(Style::default().fg(color))
        .ratio(ratio.clamp(0.0, 1.0))
        .label(label);

    frame.render_widget(gauge, area);
}

fn render_settings(frame: &mut Frame, area: Rect, state: &AppState) {
    let settings = state.settings.lock().expect("settings lock");

    let jitter_val = format!("±{}ms", settings.jitter_ms);
    let jitter_display = if settings.jitter_enabled {
        &jitter_val
    } else {
        "Off"
    };
    let jitter_detail = if !settings.jitter_enabled {
        format!(" (±{}ms when on)", settings.jitter_ms)
    } else {
        String::new()
    };

    let rows = vec![
        setting_line("Button", settings.button.label(), "B", Color::Magenta),
        setting_line("Mode", settings.mode.label(), "M", Color::Blue),
        jitter_line(jitter_display, &jitter_detail, settings.jitter_enabled),
        setting_line("Hotkey", settings.hotkey().label, "H", Color::Red),
    ];

    let block = Block::default()
        .title(" Settings ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    frame.render_widget(Paragraph::new(rows).block(block), area);
}

fn render_footer(frame: &mut Frame, area: Rect, state: &AppState, ui: &UiState) {
    let line = if ui.cps_input.is_some() {
        Line::from(vec![
            Span::raw(" Type CPS value (1-1000)  "),
            key_badge("Enter", Color::Green),
            Span::raw(" Confirm  "),
            key_badge("Esc", Color::Red),
            Span::raw(" Cancel"),
        ])
    } else {
        let hotkey_label = {
            let settings = state.settings.lock().expect("settings lock");
            settings.hotkey().label
        };

        Line::from(vec![
            key_badge(hotkey_label, Color::Cyan),
            Span::styled("/", Style::default().fg(Color::DarkGray)),
            key_badge("Space", Color::Cyan),
            Span::raw(" Toggle "),
            key_badge("↑↓", Color::Yellow),
            Span::styled("/", Style::default().fg(Color::DarkGray)),
            key_badge("S", Color::Yellow),
            Span::raw(" CPS "),
            key_badge("B", Color::Magenta),
            key_badge("M", Color::Blue),
            key_badge("J", Color::Green),
            Span::raw(" "),
            key_badge("Q", Color::Red),
            Span::raw(" Quit"),
        ])
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    frame.render_widget(Paragraph::new(line).block(block), area);
}

fn key_badge(key: &str, color: Color) -> Span<'_> {
    Span::styled(
        format!(" {key} "),
        Style::default().fg(Color::Black).bg(color),
    )
}

fn setting_line<'a>(label: &'a str, value: &'a str, key: &'a str, color: Color) -> Line<'a> {
    Line::from(vec![
        Span::raw(format!("  {label:<8} ")),
        Span::styled(
            format!("{value:<10}"),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("[{key}]"), Style::default().fg(Color::DarkGray)),
    ])
}

fn jitter_line<'a>(value: &'a str, detail: &'a str, enabled: bool) -> Line<'a> {
    let color = if enabled { Color::Green } else { Color::DarkGray };
    Line::from(vec![
        Span::raw("  Jitter   "),
        Span::styled(
            format!("{value:<10}"),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled("[J ←/→]", Style::default().fg(Color::DarkGray)),
        Span::styled(detail.to_string(), Style::default().fg(Color::DarkGray)),
    ])
}

fn format_count(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}
