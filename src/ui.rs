use crate::app::{AppState, HOTKEY_OPTIONS};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Gauge, Paragraph};
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

fn event_loop(
    terminal: &mut ratatui::DefaultTerminal,
    state: &Arc<AppState>,
) -> std::io::Result<()> {
    let mut ui = UiState { cps_input: None };

    while !state.should_quit() {
        terminal.draw(|frame| render(frame, state, &ui))?;

        if event::poll(Duration::from_millis(50))?
            && let Event::Key(key) = event::read()?
        {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            // Raw mode swallows SIGINT, so Ctrl+C arrives as a key event.
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                state.quit();
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
                    state.settings.lock().unwrap_or_else(|e| e.into_inner()).cps = clamped;
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
            let mut s = state.settings.lock().unwrap_or_else(|e| e.into_inner());
            if s.cps < 1000 {
                s.cps = (s.cps + speed_step(s.cps)).min(1000);
            }
        }
        KeyCode::Down => {
            let mut s = state.settings.lock().unwrap_or_else(|e| e.into_inner());
            if s.cps > 1 {
                s.cps = s
                    .cps
                    .saturating_sub(speed_step(s.cps.saturating_sub(1)))
                    .max(1);
            }
        }
        KeyCode::Char('b') | KeyCode::Char('B') => {
            let mut s = state.settings.lock().unwrap_or_else(|e| e.into_inner());
            s.button = s.button.next();
        }
        KeyCode::Char('m') | KeyCode::Char('M') => {
            let mut s = state.settings.lock().unwrap_or_else(|e| e.into_inner());
            s.mode = s.mode.next();
        }
        KeyCode::Char('j') | KeyCode::Char('J') => {
            let mut s = state.settings.lock().unwrap_or_else(|e| e.into_inner());
            s.jitter_enabled = !s.jitter_enabled;
        }
        KeyCode::Char('h') | KeyCode::Char('H') => {
            let mut s = state.settings.lock().unwrap_or_else(|e| e.into_inner());
            s.hotkey_index = (s.hotkey_index + 1) % HOTKEY_OPTIONS.len();
        }
        KeyCode::Right => {
            let mut s = state.settings.lock().unwrap_or_else(|e| e.into_inner());
            if s.jitter_ms < 100 {
                s.jitter_ms += 1;
            }
        }
        KeyCode::Left => {
            let mut s = state.settings.lock().unwrap_or_else(|e| e.into_inner());
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
            Constraint::Length(4), // status
            Constraint::Length(3), // gauge
            Constraint::Length(6), // settings
            Constraint::Length(3), // footer
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
        ("  ■ IDLE", Color::Gray)
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
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            if count > 0 {
                Span::styled("  [R]eset", Style::default().fg(Color::Gray))
            } else {
                Span::raw("")
            },
        ]),
    ];

    let border_color = if active {
        Color::Green
    } else {
        Color::DarkGray
    };
    let block = Block::default()
        .title(" clickr ")
        .title_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_gauge(frame: &mut Frame, area: Rect, state: &AppState, ui: &UiState) {
    let settings = state.settings.lock().unwrap_or_else(|e| e.into_inner());
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

    let (title_color, border_color) = if ui.cps_input.is_some() {
        (Color::Yellow, Color::Yellow)
    } else {
        (Color::Gray, Color::DarkGray)
    };

    let ratio = ratio.clamp(0.0, 1.0);
    let block = Block::default()
        .title(title)
        .title_style(Style::default().fg(title_color))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    let inner = block.inner(area);

    // Gauge draws its label in the terminal default colour over the fill, which is
    // unreadable on dark themes. Overlay it ourselves in two tones instead.
    let gauge = Gauge::default()
        .block(block)
        .gauge_style(Style::default().fg(color))
        .ratio(ratio)
        .label("");
    frame.render_widget(gauge, area);

    if inner.is_empty() {
        return;
    }
    let label_width = (label.chars().count() as u16).min(inner.width);
    let label_x = inner.x + (inner.width - label_width) / 2;
    let label_y = inner.y + inner.height / 2;
    // Same rounding as Gauge (non-unicode mode) so the split lands on the fill edge.
    let fill_end = inner.x + (f64::from(inner.width) * ratio).round() as u16;
    let on_fill = fill_end.saturating_sub(label_x).min(label_width) as usize;

    let (filled_part, empty_part) = {
        let mut chars = label.chars();
        let a: String = chars.by_ref().take(on_fill).collect();
        let b: String = chars.take(label_width as usize - on_fill).collect();
        (a, b)
    };
    let overlay = Line::from(vec![
        // The fill is block glyphs in the foreground colour; under text it must become the background.
        Span::styled(
            filled_part,
            Style::default()
                .fg(Color::Black)
                .bg(color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            empty_part,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(overlay),
        Rect::new(label_x, label_y, label_width, 1),
    );
}

fn render_settings(frame: &mut Frame, area: Rect, state: &AppState) {
    let settings = state.settings.lock().unwrap_or_else(|e| e.into_inner());

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
        .title_style(Style::default().fg(Color::Gray))
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
            let settings = state.settings.lock().unwrap_or_else(|e| e.into_inner());
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
        Span::styled(format!("[{key}]"), Style::default().fg(Color::Gray)),
    ])
}

fn jitter_line<'a>(value: &'a str, detail: &'a str, enabled: bool) -> Line<'a> {
    let color = if enabled { Color::Green } else { Color::Gray };
    Line::from(vec![
        Span::raw("  Jitter   "),
        Span::styled(
            format!("{value:<10}"),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled("[J ←/→]", Style::default().fg(Color::Gray)),
        Span::styled(detail.to_string(), Style::default().fg(Color::Gray)),
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
