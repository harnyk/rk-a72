use crate::error::KeymapError;

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ModifierSet: u8 {
        const L_CTRL = 1;
        const L_SHIFT = 2;
        const L_ALT = 4;
        const L_WIN = 8;
        const R_CTRL = 16;
        const R_SHIFT = 32;
        const R_ALT = 64;
        const R_WIN = 128;
    }
}

const NAMED: &[(ModifierSet, &str)] = &[
    (ModifierSet::L_CTRL, "LCtrl"),
    (ModifierSet::L_SHIFT, "LShift"),
    (ModifierSet::L_ALT, "LAlt"),
    (ModifierSet::L_WIN, "LWin"),
    (ModifierSet::R_CTRL, "RCtrl"),
    (ModifierSet::R_SHIFT, "RShift"),
    (ModifierSet::R_ALT, "RAlt"),
    (ModifierSet::R_WIN, "RWin"),
];

impl ModifierSet {
    pub fn to_label(self) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        Some(self.active_names().join("+"))
    }

    /// The names of this set's active bits, in canonical `NAMED` order — e.g.
    /// `["LCtrl", "LShift"]`. Unlike [`Self::to_label`], each name is kept separate
    /// rather than joined with `+`, matching the HCL `mods = [...]` array shape (each
    /// element resolves through [`Self::from_label`] on its own).
    pub fn active_names(self) -> Vec<&'static str> {
        NAMED
            .iter()
            .filter(|(bit, _)| self.contains(*bit))
            .map(|(_, name)| *name)
            .collect()
    }

    /// This set's active bits, each as its own single-bit `ModifierSet`, in canonical
    /// `NAMED` order — e.g. `LCtrl+LShift` yields `[L_CTRL, L_SHIFT]`. Used wherever a
    /// multi-bit combination must be expanded into one action per bit (macro press/
    /// release events have no "N modifiers at once" wire representation).
    pub fn active_bits(self) -> Vec<Self> {
        NAMED
            .iter()
            .filter(|(bit, _)| self.contains(*bit))
            .map(|(bit, _)| *bit)
            .collect()
    }

    pub fn from_label(label: &str) -> Result<Self, KeymapError> {
        let mut set = ModifierSet::empty();
        for part in label.split('+') {
            let (bit, _) = NAMED
                .iter()
                .find(|(_, name)| *name == part)
                .ok_or_else(|| KeymapError::UnknownModifier(part.to_string()))?;
            set |= *bit;
        }
        Ok(set)
    }

    /// (bit, name) for every standard USB HID modifier bit — for `list-keys`.
    pub fn list_named() -> Vec<(u8, &'static str)> {
        NAMED
            .iter()
            .map(|(set, name)| (set.bits(), *name))
            .collect()
    }

    /// The standard USB HID keyboard usage code (0xe0-0xe7 / 224-231) for this set's
    /// single active bit — the byte a macro's `ModifyKey` action expects, confirmed
    /// against a real captured LCtrl macro action (`key=224`, see
    /// `docs/superpowers/specs/2026-08-25-macros-capture.md`). This is a different
    /// number space from `.bits()` (the KeyMatrix `keyMappingPara` bitmask, e.g.
    /// LCtrl=1) — the two must never be confused. `None` if this set is empty or has
    /// more than one bit set, since a single macro action presses exactly one
    /// modifier.
    pub fn to_hid_usage_code(self) -> Option<u8> {
        if self.bits().count_ones() != 1 {
            return None;
        }
        NAMED
            .iter()
            .position(|(bit, _)| *bit == self)
            .map(|i| 0xe0 + i as u8)
    }

    /// The inverse of [`Self::to_hid_usage_code`]: the single-bit `ModifierSet` for a
    /// standard USB HID keyboard modifier usage code (0xe0-0xe7 / 224-231). `None`
    /// outside that range.
    pub fn from_hid_usage_code(code: u8) -> Option<Self> {
        let i = code.checked_sub(0xe0)? as usize;
        NAMED.get(i).map(|(bit, _)| *bit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_label_joins_active_bits_with_plus() {
        let set = ModifierSet::L_CTRL | ModifierSet::L_SHIFT;
        assert_eq!(set.to_label(), Some("LCtrl+LShift".to_string()));
    }

    #[test]
    fn to_label_is_none_for_empty_set() {
        assert_eq!(ModifierSet::empty().to_label(), None);
    }

    #[test]
    fn from_label_round_trips_with_to_label() {
        let set = ModifierSet::from_label("LCtrl+LShift").unwrap();
        assert_eq!(set, ModifierSet::L_CTRL | ModifierSet::L_SHIFT);
    }

    #[test]
    fn from_label_rejects_unknown_modifier() {
        let err = ModifierSet::from_label("NotAMod").unwrap_err();
        assert_eq!(err.to_string(), "unknown modifier \"NotAMod\"");
    }

    #[test]
    fn list_named_has_all_8_bits_matching_the_flag_definitions() {
        let named = ModifierSet::list_named();
        assert_eq!(named.len(), 8);
        assert!(named.contains(&(1, "LCtrl")));
        assert!(named.contains(&(128, "RWin")));
    }
}
