use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::UiState;
use crate::geometry::{slot_for, A72_GEOMETRY};
use crate::state::AppState;

use super::key_box_area;

fn led_rgb(app: &AppState, slot: u16) -> (u8, u8, u8) {
    let count = app.working_led.len() / 3;
    let (r_off, g_off, b_off) = (slot as usize, slot as usize + count, slot as usize + count * 2);
    (
        app.working_led.get(r_off).copied().unwrap_or(0),
        app.working_led.get(g_off).copied().unwrap_or(0),
        app.working_led.get(b_off).copied().unwrap_or(0),
    )
}

pub fn render(frame: &mut Frame, area: Rect, app: &AppState, ui: &UiState) {
    for geo in A72_GEOMETRY {
        let box_area = key_box_area(geo.col, geo.row, geo.w, geo.h, area).intersection(area);
        if box_area.is_empty() {
            continue;
        }

        let Some(slot) = slot_for(geo.name) else { continue };
        let (r, g, b) = led_rgb(app, slot);
        let selected = geo.name == ui.selected_key;
        let dirty = app.led_slot_dirty(slot);

        let fg = if r as u16 + g as u16 + b as u16 > 380 { Color::Black } else { Color::White };
        let bg = Color::Rgb(r, g, b);

        let mut block = Block::default().style(Style::default().bg(bg).fg(fg));
        if selected {
            block = block.borders(Borders::ALL).border_style(Style::default().fg(Color::White));
        }

        let label = if dirty { format!("{}\u{25CF}", geo.name) } else { geo.name.to_string() };
        let widget = Paragraph::new(Span::raw(label)).block(block);
        frame.render_widget(widget, box_area);
    }
}
