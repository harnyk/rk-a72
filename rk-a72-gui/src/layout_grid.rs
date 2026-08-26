/// (physical key name, row, column) for every entry in `physical_key_labels.json`.
/// An approximate uniform grid, not pixel-accurate ANSI keycap sizing — rows follow
/// the keyboard's real row groupings; odd/non-standard keys (Logo, IntlBackslash,
/// Del, Hash) are grouped into a trailing "other keys" row rather than placed at
/// their exact physical position, since exact keycap shapes are out of scope.
const GRID: &[(&str, u8, u8)] = &[
    // Row 0: macro cluster (left side) + RK-logo touch key
    ("M5", 0, 0),
    ("M4", 0, 1),
    ("M3", 0, 2),
    ("M2", 0, 3),
    ("M1", 0, 4),
    ("Logo", 0, 6),
    // Row 1: number row
    ("Esc", 1, 0),
    ("Digit1", 1, 1),
    ("Digit2", 1, 2),
    ("Digit3", 1, 3),
    ("Digit4", 1, 4),
    ("Digit5", 1, 5),
    ("Digit6", 1, 6),
    ("Digit7", 1, 7),
    ("Digit8", 1, 8),
    ("Digit9", 1, 9),
    ("Digit0", 1, 10),
    ("Minus", 1, 11),
    ("Equal", 1, 12),
    ("Backspace", 1, 13),
    // Row 2: QWERTY row
    ("Tab", 2, 0),
    ("Q", 2, 1),
    ("W", 2, 2),
    ("E", 2, 3),
    ("R", 2, 4),
    ("T", 2, 5),
    ("Y", 2, 6),
    ("U", 2, 7),
    ("I", 2, 8),
    ("O", 2, 9),
    ("P", 2, 10),
    ("BracketLeft", 2, 11),
    ("BracketRight", 2, 12),
    ("Backslash", 2, 13),
    ("PgUp", 2, 14),
    // Row 3: ASDF row
    ("CapsLock", 3, 0),
    ("A", 3, 1),
    ("S", 3, 2),
    ("D", 3, 3),
    ("F", 3, 4),
    ("G", 3, 5),
    ("H", 3, 6),
    ("J", 3, 7),
    ("K", 3, 8),
    ("L", 3, 9),
    ("Semicolon", 3, 10),
    ("Quote", 3, 11),
    ("Enter", 3, 12),
    ("PgDn", 3, 13),
    // Row 4: ZXCV row
    ("LShift", 4, 0),
    ("Z", 4, 1),
    ("X", 4, 2),
    ("C", 4, 3),
    ("V", 4, 4),
    ("B", 4, 5),
    ("N", 4, 6),
    ("M", 4, 7),
    ("Comma", 4, 8),
    ("Period", 4, 9),
    ("Slash", 4, 10),
    ("RShift", 4, 11),
    ("Up", 4, 12),
    // Row 5: bottom row
    ("LCtrl", 5, 0),
    ("LWin", 5, 1),
    ("LAlt", 5, 2),
    ("SpaceL", 5, 3),
    ("SpaceR", 5, 4),
    ("RAlt", 5, 5),
    ("Fn1", 5, 6),
    ("Left", 5, 7),
    ("Down", 5, 8),
    ("Right", 5, 9),
    // Row 6: media cluster
    ("Mute", 6, 0),
    ("PrevTr", 6, 1),
    ("PlayPause", 6, 2),
    ("NextTr", 6, 3),
    ("VolumD", 6, 4),
    ("VolumI", 6, 5),
    // Row 7: other/non-standard keys, grouped rather than exactly positioned
    ("IntlBackslash", 7, 0),
    ("Del", 7, 1),
    ("Hash", 7, 2),
];

/// Looks up the (row, column) grid position for a physical key's canonical name
/// (from `PhysicalKeyboardLayout::list_named()`). Returns `None` for a name not in
/// the static table — `keyboard_view` (Task 5) should skip rendering such a key
/// rather than panic, since this table is meant to stay in sync with
/// `physical_key_labels.json` but a future addition there could briefly outrun it.
pub fn grid_position(name: &str) -> Option<(u8, u8)> {
    GRID.iter()
        .find(|(n, _, _)| *n == name)
        .map(|(_, r, c)| (*r, *c))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rk_a72_keymap::PhysicalKeyboardLayout;
    use std::collections::HashSet;

    #[test]
    fn every_physical_key_has_a_unique_grid_position() {
        let layout = PhysicalKeyboardLayout::new();
        let mut seen = HashSet::new();
        for (name, _slot, _visual) in layout.list_named() {
            let pos = grid_position(&name)
                .unwrap_or_else(|| panic!("no grid position defined for physical key \"{name}\""));
            assert!(
                seen.insert(pos),
                "grid position {pos:?} is used by more than one key (also occupied before \"{name}\")"
            );
        }
    }

    #[test]
    fn unknown_key_name_has_no_grid_position() {
        assert_eq!(grid_position("NotAKey"), None);
    }
}
