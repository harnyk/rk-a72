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
