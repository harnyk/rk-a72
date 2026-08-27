pub mod keymap_tab;
pub mod led_tab;

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Tabs};
use ratatui::Frame;

use crate::app::{Tab, UiState};
use crate::state::AppState;

pub fn draw(frame: &mut Frame, app: &AppState, ui: &UiState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(frame.area());

    let titles = ["Keymap", "LED"].map(Line::from);
    let selected = match ui.tab {
        Tab::Keymap => 0,
        Tab::Led => 1,
    };
    let tabs = Tabs::new(titles)
        .select(selected)
        .highlight_style(Style::default().fg(Color::Yellow));
    frame.render_widget(tabs, chunks[0]);

    match ui.tab {
        Tab::Keymap => keymap_tab::render(frame, chunks[1], app, ui),
        Tab::Led => led_tab::render(frame, chunks[1], app, ui),
    }
}

pub fn key_box_area(col: u16, row: u16, w: u16, h: u16, origin: Rect) -> Rect {
    // 1 grid unit = 2 terminal columns wide, 1 terminal row tall (units are square in
    // "key fractions" but terminal character cells are roughly 1:2 w:h, so this keeps
    // rendered keys looking roughly square).
    Rect {
        x: origin.x + col * 2,
        y: origin.y + row,
        width: w * 2,
        height: h,
    }
}

pub fn status_line(text: &str) -> Paragraph {
    Paragraph::new(Span::raw(text))
}
