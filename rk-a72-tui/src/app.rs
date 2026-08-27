use crate::geometry::{geometry_for, A72_GEOMETRY};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Keymap,
    Led,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    Normal,
    Fn,
    Fn2,
}

impl Layer {
    pub fn as_u8(self) -> u8 {
        match self {
            Layer::Normal => 0,
            Layer::Fn => 1,
            Layer::Fn2 => 2,
        }
    }
}

pub struct UiState {
    pub tab: Tab,
    pub layer: Layer,
    pub selected_key: &'static str,
}

impl UiState {
    pub fn new() -> Self {
        Self {
            tab: Tab::Keymap,
            layer: Layer::Normal,
            selected_key: A72_GEOMETRY.first().map(|g| g.name).unwrap_or("Esc"),
        }
    }

    pub fn switch_tab(&mut self, tab: Tab) {
        self.tab = tab;
    }

    pub fn cycle_layer(&mut self) {
        self.layer = match self.layer {
            Layer::Normal => Layer::Fn,
            Layer::Fn => Layer::Fn2,
            Layer::Fn2 => Layer::Normal,
        };
    }

    /// Moves `selected_key` to the geometry entry whose center is nearest in the given
    /// direction from the current key's center, among entries strictly in that direction.
    /// (dx, dy) is a unit-ish direction, e.g. (1, 0) for right, (0, -1) for up. No-op if no
    /// entry exists in that direction.
    pub fn move_cursor(&mut self, dx: i32, dy: i32) {
        let Some(current) = geometry_for(self.selected_key) else { return };
        let (cx, cy) = (
            current.col as i32 + current.w as i32 / 2,
            current.row as i32 + current.h as i32 / 2,
        );

        let mut best: Option<(&'static str, i32)> = None;
        for g in A72_GEOMETRY {
            if g.name == self.selected_key {
                continue;
            }
            let (gx, gy) = (g.col as i32 + g.w as i32 / 2, g.row as i32 + g.h as i32 / 2);
            let (ddx, ddy) = (gx - cx, gy - cy);
            // Must be (weakly) in the requested direction and not directly opposite.
            let in_direction = if dx != 0 { ddx * dx > 0 } else { ddy * dy > 0 };
            if !in_direction {
                continue;
            }
            // Distance-squared, penalizing perpendicular offset so movement prefers
            // staying roughly in line over jumping to a far-off row/column.
            let perpendicular = if dx != 0 { ddy } else { ddx };
            let score = ddx * ddx + ddy * ddy + perpendicular * perpendicular * 4;
            if best.is_none() || score < best.unwrap().1 {
                best = Some((g.name, score));
            }
        }

        if let Some((name, _)) = best {
            self.selected_key = name;
        }
    }
}

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    SwitchTab(Tab),
    CycleLayer,
    MoveCursor(i32, i32),
    OpenActionDialog,
    Save,
    None,
}

/// Maps a raw key event to an application-level `Action`, independent of `UiState` — the
/// same key always maps to the same action regardless of which tab is active; it's the
/// caller's job to ignore actions that don't apply to the current tab (e.g. `OpenActionDialog`
/// is only acted on while `ui.tab == Tab::Keymap`).
pub fn dispatch_key(event: KeyEvent) -> Action {
    match (event.code, event.modifiers) {
        (KeyCode::Char('q'), KeyModifiers::NONE) => Action::Quit,
        (KeyCode::Char('s'), KeyModifiers::CONTROL) => Action::Save,
        (KeyCode::Char('1'), KeyModifiers::NONE) => Action::SwitchTab(Tab::Keymap),
        (KeyCode::Char('2'), KeyModifiers::NONE) => Action::SwitchTab(Tab::Led),
        (KeyCode::Tab, KeyModifiers::NONE) => Action::CycleLayer,
        (KeyCode::Left, KeyModifiers::NONE) => Action::MoveCursor(-1, 0),
        (KeyCode::Right, KeyModifiers::NONE) => Action::MoveCursor(1, 0),
        (KeyCode::Up, KeyModifiers::NONE) => Action::MoveCursor(0, -1),
        (KeyCode::Down, KeyModifiers::NONE) => Action::MoveCursor(0, 1),
        (KeyCode::Enter, KeyModifiers::NONE) => Action::OpenActionDialog,
        _ => Action::None,
    }
}

#[cfg(test)]
mod dispatch_tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn q_quits() {
        assert_eq!(dispatch_key(key(KeyCode::Char('q'))), Action::Quit);
    }

    #[test]
    fn ctrl_s_saves() {
        let event = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL);
        assert_eq!(dispatch_key(event), Action::Save);
    }

    #[test]
    fn plain_s_is_not_save() {
        assert_eq!(dispatch_key(key(KeyCode::Char('s'))), Action::None);
    }

    #[test]
    fn digit_1_switches_to_keymap_tab() {
        assert_eq!(dispatch_key(key(KeyCode::Char('1'))), Action::SwitchTab(Tab::Keymap));
    }

    #[test]
    fn digit_2_switches_to_led_tab() {
        assert_eq!(dispatch_key(key(KeyCode::Char('2'))), Action::SwitchTab(Tab::Led));
    }

    #[test]
    fn tab_cycles_layer() {
        assert_eq!(dispatch_key(key(KeyCode::Tab)), Action::CycleLayer);
    }

    #[test]
    fn arrow_keys_move_cursor() {
        assert_eq!(dispatch_key(key(KeyCode::Left)), Action::MoveCursor(-1, 0));
        assert_eq!(dispatch_key(key(KeyCode::Right)), Action::MoveCursor(1, 0));
        assert_eq!(dispatch_key(key(KeyCode::Up)), Action::MoveCursor(0, -1));
        assert_eq!(dispatch_key(key(KeyCode::Down)), Action::MoveCursor(0, 1));
    }

    #[test]
    fn enter_opens_action_dialog() {
        assert_eq!(dispatch_key(key(KeyCode::Enter)), Action::OpenActionDialog);
    }

    #[test]
    fn unmapped_key_is_none() {
        assert_eq!(dispatch_key(key(KeyCode::Char('z'))), Action::None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_starts_on_keymap_tab_normal_layer() {
        let ui = UiState::new();
        assert_eq!(ui.tab, Tab::Keymap);
        assert_eq!(ui.layer, Layer::Normal);
    }

    #[test]
    fn switch_tab_changes_the_active_tab() {
        let mut ui = UiState::new();
        ui.switch_tab(Tab::Led);
        assert_eq!(ui.tab, Tab::Led);
    }

    #[test]
    fn cycle_layer_goes_normal_fn_fn2_normal() {
        let mut ui = UiState::new();
        assert_eq!(ui.layer, Layer::Normal);
        ui.cycle_layer();
        assert_eq!(ui.layer, Layer::Fn);
        ui.cycle_layer();
        assert_eq!(ui.layer, Layer::Fn2);
        ui.cycle_layer();
        assert_eq!(ui.layer, Layer::Normal);
    }

    #[test]
    fn layer_as_u8_matches_keymatrix_layer_numbering() {
        assert_eq!(Layer::Normal.as_u8(), 0);
        assert_eq!(Layer::Fn.as_u8(), 1);
        assert_eq!(Layer::Fn2.as_u8(), 2);
    }

    #[test]
    fn move_cursor_right_moves_to_the_nearest_key_to_the_right() {
        // Real A72_GEOMETRY (Task 2) has Esc at (col 10, row 5) and Digit1 at
        // (col 14, row 5) — same row, adjacent columns.
        let mut ui = UiState::new();
        ui.selected_key = "Esc";
        ui.move_cursor(1, 0);
        assert_eq!(ui.selected_key, "Digit1");
    }

    #[test]
    fn move_cursor_left_moves_to_the_nearest_key_to_the_left() {
        let mut ui = UiState::new();
        ui.selected_key = "Digit1";
        ui.move_cursor(-1, 0);
        assert_eq!(ui.selected_key, "Esc");
    }

    #[test]
    fn move_cursor_with_no_key_in_that_direction_is_a_no_op() {
        // VolumI sits at col 0, row 5 — the leftmost key on its row in the real
        // A72_GEOMETRY table; nothing exists further left on that row.
        let mut ui = UiState::new();
        ui.selected_key = "VolumI";
        ui.move_cursor(-1, 0);
        assert_eq!(ui.selected_key, "VolumI");
    }

    #[test]
    fn move_cursor_down_moves_to_a_key_on_the_row_below() {
        // M5 is at (col 5, row 5); M4 is at (col 5, row 9) — same column, next row down.
        let mut ui = UiState::new();
        ui.selected_key = "M5";
        ui.move_cursor(0, 1);
        assert_eq!(ui.selected_key, "M4");
    }
}

use rk_a72_keymap::{KeyMatrixRepository, LedColorRepository};

use crate::state::AppState;

/// Runs the interactive loop until the user quits. Save errors are shown on the status
/// line rather than ending the loop, per the spec: working state is preserved on failure
/// and the user can retry.
pub fn run(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut AppState,
    keymap_repo: &KeyMatrixRepository,
    led_repo: &LedColorRepository,
) -> anyhow::Result<()> {
    let mut ui = UiState::new();
    let mut status: Option<String> = None;

    loop {
        terminal.draw(|frame| {
            crate::ui::draw(frame, app, &ui);
            if let Some(msg) = &status {
                let area = frame.area();
                let status_area = ratatui::layout::Rect {
                    x: area.x,
                    y: area.y + area.height.saturating_sub(1),
                    width: area.width,
                    height: 1,
                };
                frame.render_widget(crate::ui::status_line(msg), status_area);
            }
        })?;

        let event = crossterm::event::read()?;
        let crossterm::event::Event::Key(key_event) = event else { continue };
        if key_event.kind != crossterm::event::KeyEventKind::Press {
            continue;
        }

        match dispatch_key(key_event) {
            Action::Quit => return Ok(()),
            Action::SwitchTab(tab) => {
                ui.switch_tab(tab);
                status = None;
            }
            Action::CycleLayer => {
                if ui.tab == Tab::Keymap {
                    ui.cycle_layer();
                }
            }
            Action::MoveCursor(dx, dy) => ui.move_cursor(dx, dy),
            Action::OpenActionDialog => {
                // Full modal input handling (typing a symbol, choosing mods/labels) is UI
                // interaction verified manually against real hardware per the spec; this
                // task wires the dialog's *open* trigger through. The dialog's confirm
                // path writes into app.working_keymap via the same pattern build_layer_buffer
                // already exercises in tests (Task 4) — see ui::keymap_tab::ActionDialogState.
                status = Some(format!("editing {} (layer {:?}) — dialog UI pending", ui.selected_key, ui.layer));
            }
            Action::Save => match app.save(keymap_repo, led_repo) {
                Ok(()) => status = Some("Saved.".to_string()),
                Err(e) => status = Some(format!("Save failed: {e}")),
            },
            Action::None => {}
        }
    }
}
