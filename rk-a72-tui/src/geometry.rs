pub struct KeyGeometry {
    pub name: &'static str,
    pub col: u16,
    pub row: u16,
    pub w: u16,
    pub h: u16,
}

/// Model keys with no physical keycap on this specific A72 — confirmed against real
/// hardware. `KeyboardModel`'s key set includes them because the underlying protocol
/// family's scan matrix reserves the slot; this board just doesn't populate it. Kept as an
/// explicit, named exception (rather than a silent gap) so the consistency tests below
/// distinguish "deliberately unplaced" from "forgotten."
const KEYS_WITH_NO_PHYSICAL_KEYCAP: &[&str] = &["Mute", "IntlBackslash", "Hash"];

pub static A72_GEOMETRY: &[KeyGeometry] = &[
    KeyGeometry { name: "M5", col: 5, row: 5, w: 4, h: 4 },
    KeyGeometry { name: "M4", col: 5, row: 9, w: 4, h: 4 },
    KeyGeometry { name: "M3", col: 5, row: 13, w: 4, h: 4 },
    KeyGeometry { name: "M2", col: 5, row: 17, w: 4, h: 4 },
    KeyGeometry { name: "M1", col: 5, row: 21, w: 4, h: 4 },
    KeyGeometry { name: "Esc", col: 10, row: 5, w: 4, h: 4 },
    KeyGeometry { name: "Tab", col: 10, row: 9, w: 5, h: 4 },
    KeyGeometry { name: "CapsLock", col: 10, row: 13, w: 6, h: 4 },
    KeyGeometry { name: "LShift", col: 10, row: 17, w: 8, h: 4 },
    KeyGeometry { name: "LCtrl", col: 10, row: 21, w: 4, h: 4 },
    KeyGeometry { name: "Digit1", col: 14, row: 5, w: 4, h: 4 },
    KeyGeometry { name: "Q", col: 15, row: 9, w: 4, h: 4 },
    KeyGeometry { name: "A", col: 16, row: 13, w: 4, h: 4 },
    KeyGeometry { name: "LWin", col: 14, row: 21, w: 4, h: 4 },
    KeyGeometry { name: "Digit2", col: 18, row: 5, w: 4, h: 4 },
    KeyGeometry { name: "W", col: 19, row: 9, w: 4, h: 4 },
    KeyGeometry { name: "S", col: 20, row: 13, w: 4, h: 4 },
    KeyGeometry { name: "Z", col: 18, row: 17, w: 4, h: 4 },
    KeyGeometry { name: "LAlt", col: 18, row: 21, w: 4, h: 4 },
    KeyGeometry { name: "Digit3", col: 22, row: 5, w: 4, h: 4 },
    KeyGeometry { name: "E", col: 23, row: 9, w: 4, h: 4 },
    KeyGeometry { name: "D", col: 24, row: 13, w: 4, h: 4 },
    KeyGeometry { name: "X", col: 22, row: 17, w: 4, h: 4 },
    KeyGeometry { name: "Digit4", col: 26, row: 5, w: 4, h: 4 },
    KeyGeometry { name: "R", col: 27, row: 9, w: 4, h: 4 },
    KeyGeometry { name: "F", col: 28, row: 13, w: 4, h: 4 },
    KeyGeometry { name: "C", col: 26, row: 17, w: 4, h: 4 },
    KeyGeometry { name: "Digit5", col: 30, row: 5, w: 4, h: 4 },
    KeyGeometry { name: "T", col: 31, row: 9, w: 4, h: 4 },
    KeyGeometry { name: "G", col: 32, row: 13, w: 4, h: 4 },
    KeyGeometry { name: "V", col: 30, row: 17, w: 4, h: 4 },
    KeyGeometry { name: "SpaceL", col: 22, row: 21, w: 17, h: 4 },
    KeyGeometry { name: "Digit6", col: 34, row: 5, w: 4, h: 4 },
    KeyGeometry { name: "Y", col: 43, row: 9, w: 4, h: 4 },
    KeyGeometry { name: "H", col: 43, row: 13, w: 4, h: 4 },
    KeyGeometry { name: "B", col: 34, row: 17, w: 4, h: 4 },
    KeyGeometry { name: "Digit7", col: 46, row: 5, w: 4, h: 4 },
    KeyGeometry { name: "U", col: 47, row: 9, w: 4, h: 4 },
    KeyGeometry { name: "J", col: 47, row: 13, w: 4, h: 4 },
    KeyGeometry { name: "N", col: 44, row: 17, w: 4, h: 4 },
    KeyGeometry { name: "SpaceR", col: 41, row: 21, w: 17, h: 4 },
    KeyGeometry { name: "Digit8", col: 50, row: 5, w: 4, h: 4 },
    KeyGeometry { name: "I", col: 51, row: 9, w: 4, h: 4 },
    KeyGeometry { name: "K", col: 51, row: 13, w: 4, h: 4 },
    KeyGeometry { name: "M", col: 48, row: 17, w: 4, h: 4 },
    KeyGeometry { name: "Digit9", col: 54, row: 5, w: 4, h: 4 },
    KeyGeometry { name: "O", col: 55, row: 9, w: 4, h: 4 },
    KeyGeometry { name: "L", col: 55, row: 13, w: 4, h: 4 },
    KeyGeometry { name: "Comma", col: 52, row: 17, w: 4, h: 4 },
    KeyGeometry { name: "Digit0", col: 58, row: 5, w: 4, h: 4 },
    KeyGeometry { name: "P", col: 59, row: 9, w: 4, h: 4 },
    KeyGeometry { name: "Semicolon", col: 59, row: 13, w: 4, h: 4 },
    KeyGeometry { name: "Period", col: 56, row: 17, w: 4, h: 4 },
    KeyGeometry { name: "RAlt", col: 58, row: 21, w: 4, h: 4 },
    KeyGeometry { name: "Minus", col: 62, row: 5, w: 4, h: 4 },
    KeyGeometry { name: "BracketLeft", col: 63, row: 9, w: 4, h: 4 },
    KeyGeometry { name: "Quote", col: 63, row: 13, w: 4, h: 4 },
    KeyGeometry { name: "Slash", col: 60, row: 17, w: 4, h: 4 },
    KeyGeometry { name: "Equal", col: 66, row: 5, w: 4, h: 4 },
    KeyGeometry { name: "BracketRight", col: 67, row: 9, w: 4, h: 4 },
    KeyGeometry { name: "Fn1", col: 62, row: 21, w: 4, h: 4 },
    KeyGeometry { name: "Backspace", col: 70, row: 5, w: 8, h: 4 },
    KeyGeometry { name: "Backslash", col: 71, row: 9, w: 7, h: 4 },
    KeyGeometry { name: "Enter", col: 67, row: 13, w: 11, h: 4 },
    KeyGeometry { name: "RShift", col: 64, row: 17, w: 8, h: 4 },
    KeyGeometry { name: "Left", col: 70, row: 23, w: 4, h: 4 },
    KeyGeometry { name: "Up", col: 74, row: 19, w: 4, h: 4 },
    KeyGeometry { name: "Down", col: 74, row: 23, w: 4, h: 4 },
    KeyGeometry { name: "Del", col: 79, row: 5, w: 4, h: 4 },
    KeyGeometry { name: "PgUp", col: 79, row: 9, w: 4, h: 4 },
    KeyGeometry { name: "PgDn", col: 79, row: 13, w: 4, h: 4 },
    KeyGeometry { name: "Right", col: 78, row: 23, w: 4, h: 4 },
    KeyGeometry { name: "PrevTr", col: 0, row: 13, w: 4, h: 4 },
    KeyGeometry { name: "PlayPause", col: 0, row: 17, w: 4, h: 4 },
    KeyGeometry { name: "NextTr", col: 0, row: 21, w: 4, h: 4 },
    KeyGeometry { name: "Logo", col: 39, row: 0, w: 6, h: 4 },
    KeyGeometry { name: "VolumD", col: 0, row: 9, w: 4, h: 4 },
    KeyGeometry { name: "VolumI", col: 0, row: 5, w: 4, h: 4 },
];

pub fn geometry_for(name: &str) -> Option<&'static KeyGeometry> {
    A72_GEOMETRY.iter().find(|g| g.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rk_a72_keymap::KeyboardModel;

    #[test]
    fn every_model_key_has_a_geometry_entry_or_is_a_known_unplaced_exception() {
        let model = KeyboardModel::default_model();
        for (_, name) in model.named_keys() {
            let placed = geometry_for(name).is_some();
            let known_exception = KEYS_WITH_NO_PHYSICAL_KEYCAP.contains(&name);
            assert!(
                placed || known_exception,
                "key {name:?} has no geometry entry in A72_GEOMETRY and isn't listed in \
                 KEYS_WITH_NO_PHYSICAL_KEYCAP — was it forgotten, or does this board really \
                 lack it?"
            );
        }
    }

    #[test]
    fn no_known_exception_actually_has_a_geometry_entry() {
        // Catches the table drifting out of sync the other way: if a "no physical keycap"
        // key later gets a geometry entry (e.g. corrected after further hardware
        // inspection), it must be removed from KEYS_WITH_NO_PHYSICAL_KEYCAP too.
        for &name in KEYS_WITH_NO_PHYSICAL_KEYCAP {
            assert!(
                geometry_for(name).is_none(),
                "{name:?} is listed as having no physical keycap, but has a geometry entry \
                 anyway — remove it from KEYS_WITH_NO_PHYSICAL_KEYCAP"
            );
        }
    }

    #[test]
    fn every_geometry_entry_matches_a_real_model_key() {
        let model = KeyboardModel::default_model();
        let known: std::collections::HashSet<&str> =
            model.named_keys().map(|(_, name)| name).collect();
        for g in A72_GEOMETRY {
            assert!(
                known.contains(g.name),
                "geometry entry {:?} names a key not in the model's key set",
                g.name
            );
        }
    }
}
