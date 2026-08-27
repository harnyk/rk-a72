use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::UiState;
use crate::geometry::A72_GEOMETRY;
use crate::state::{AppState, SlotState};

use super::key_box_area;

// ActionKind/ActionDialogState are not yet reachable from any key handler — the
// action-edit dialog itself is future work (see main.rs's module doc comment). Kept
// `#[allow(dead_code)]` rather than silently ignored so it's clear this is expected.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionKind {
    Key,
    Label,
    Raw,
}

/// Editable field state for the action-edit modal, opened when the user presses Enter on
/// a selected key. Holds one buffer per action kind so switching tabs inside the dialog
/// doesn't lose what was typed on another tab.
#[allow(dead_code)]
pub struct ActionDialogState {
    pub selected_tab: ActionKind,
    pub key_symbol: String,
    pub key_mods: Vec<String>,
    pub label: String,
    pub raw_hex: String,
}

#[allow(dead_code)]
impl ActionDialogState {
    pub fn new() -> Self {
        Self {
            selected_tab: ActionKind::Key,
            key_symbol: String::new(),
            key_mods: Vec::new(),
            label: String::new(),
            raw_hex: String::new(),
        }
    }

    pub fn next_tab(&mut self) {
        self.selected_tab = match self.selected_tab {
            ActionKind::Key => ActionKind::Label,
            ActionKind::Label => ActionKind::Raw,
            ActionKind::Raw => ActionKind::Key,
        };
    }

    pub fn prev_tab(&mut self) {
        self.selected_tab = match self.selected_tab {
            ActionKind::Key => ActionKind::Raw,
            ActionKind::Label => ActionKind::Key,
            ActionKind::Raw => ActionKind::Label,
        };
    }
}

fn state_color(state: SlotState) -> Color {
    match state {
        SlotState::Clean => Color::Gray,
        SlotState::Customized => Color::Green,
        SlotState::Dirty => Color::Yellow,
    }
}

pub fn render(frame: &mut Frame, area: Rect, app: &AppState, ui: &UiState) {
    for geo in A72_GEOMETRY {
        let box_area = key_box_area(geo.col, geo.row, geo.w, geo.h, area).intersection(area);
        if box_area.is_empty() {
            continue; // off-screen (or clipped to nothing), skip rather than let ratatui panic
        }

        // Slot lookup: geometry only carries names; resolving a name to its KeyMatrix
        // slot for the active model is the same job PhysicalKeyboardLayout already does
        // (see rk-a72-keymap::layout) — the caller (Task 9's event loop / main.rs) is
        // expected to pass a resolved slot map alongside AppState in the fuller
        // integration; for rendering, slot_state is looked up the same way.
        let Some(slot) = crate::geometry::slot_for(geo.name) else { continue };
        let slot_state = app.keymap_slot_state(ui.layer.as_u8(), slot);
        let selected = geo.name == ui.selected_key;

        let border_set = if selected {
            ratatui::symbols::border::DOUBLE
        } else {
            ratatui::symbols::border::ROUNDED
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(border_set)
            .style(Style::default().fg(state_color(slot_state)));
        let label = Paragraph::new(Line::from(Span::raw(geo.name))).block(block);
        frame.render_widget(label, box_area);
    }
}
