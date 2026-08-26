use std::collections::HashMap;

pub struct PhysicalKeyboardLayout {
    name_by_slot: HashMap<u16, String>,
    slot_by_name: HashMap<String, u16>,
    visual: crate::visual::VisualOverrides,
}

fn parse_physical_key_labels(json: &str) -> HashMap<u16, String> {
    let raw: HashMap<String, String> =
        serde_json::from_str(json).expect("physical_key_labels.json must be valid JSON");
    raw.into_iter()
        .filter_map(|(k, v)| k.parse::<u16>().ok().map(|slot| (slot, v)))
        .collect()
}

impl PhysicalKeyboardLayout {
    pub const KEYMATRIX_SLOT_COUNT: u16 = crate::protocol::KEYMATRIX_SLOT_COUNT as u16;

    pub fn new() -> Self {
        let name_by_slot =
            parse_physical_key_labels(include_str!("../data/physical_key_labels.json"));
        let slot_by_name = name_by_slot
            .iter()
            .map(|(&slot, name)| (name.clone(), slot))
            .collect();
        Self {
            name_by_slot,
            slot_by_name,
            visual: crate::visual::VisualOverrides::new(),
        }
    }

    pub fn name_for_slot(&self, slot: u16) -> String {
        self.name_by_slot
            .get(&slot)
            .cloned()
            .unwrap_or_else(|| format!("slot{slot}"))
    }

    pub fn slot_for_name(&self, name: &str) -> Option<u16> {
        if let Some(&slot) = self.slot_by_name.get(name) {
            return Some(slot);
        }
        let rest = name.strip_prefix("slot")?;
        let slot: u16 = rest.parse().ok()?;
        (slot < Self::KEYMATRIX_SLOT_COUNT).then_some(slot)
    }

    pub fn list_named(&self) -> Vec<(String, u16, String)> {
        let mut v: Vec<_> = self
            .name_by_slot
            .iter()
            .map(|(&slot, name)| (name.clone(), slot, self.visual.physical(slot as u32, name)))
            .collect();
        v.sort_by_key(|(_, slot, _)| *slot);
        v
    }
}

impl Default for PhysicalKeyboardLayout {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_for_slot_resolves_known_and_fallback_names() {
        let layout = PhysicalKeyboardLayout::new();
        assert_eq!(layout.name_for_slot(7), "Esc");
        assert_eq!(layout.name_for_slot(0), "slot0"); // slot 0 has no entry in physical_key_labels.json
    }

    #[test]
    fn slot_for_name_is_the_inverse_including_the_slotn_fallback() {
        let layout = PhysicalKeyboardLayout::new();
        assert_eq!(layout.slot_for_name("Esc"), Some(7));
        assert_eq!(layout.slot_for_name("slot0"), Some(0));
        assert_eq!(layout.slot_for_name("NotAKey"), None);
        assert_eq!(layout.slot_for_name("slot9999"), None); // out of range
    }

    #[test]
    fn m_cluster_names_are_reversed_from_slot_index_order() {
        // Confirmed by physically pressing the keycaps (bottom keycap "M1" sends
        // Ctrl+Z, which is slot 5's value) — see physical_key_labels.json.
        let layout = PhysicalKeyboardLayout::new();
        assert_eq!(layout.name_for_slot(1), "M5");
        assert_eq!(layout.name_for_slot(5), "M1");
    }

    #[test]
    fn every_name_is_unique() {
        let layout = PhysicalKeyboardLayout::new();
        let names: Vec<&String> = layout.name_by_slot.values().collect();
        let unique: std::collections::HashSet<&&String> = names.iter().collect();
        assert_eq!(unique.len(), names.len());
    }

    #[test]
    fn renamed_digit_row_keys_resolve_by_their_new_canonical_name_only() {
        let layout = PhysicalKeyboardLayout::new();
        assert_eq!(layout.slot_for_name("Digit1"), Some(13));
        assert_eq!(layout.slot_for_name("1!"), None); // old glyph is no longer valid input
        assert_eq!(layout.name_for_slot(13), "Digit1");
    }

    #[test]
    fn renamed_punctuation_keys_resolve_by_their_new_canonical_name_only() {
        let layout = PhysicalKeyboardLayout::new();
        assert_eq!(layout.slot_for_name("Backslash"), Some(86));
        assert_eq!(layout.slot_for_name("\\|"), None);
    }

    #[test]
    fn renamed_equal_key_resolves_by_its_new_canonical_name_only() {
        let layout = PhysicalKeyboardLayout::new();
        assert_eq!(layout.slot_for_name("Equal"), Some(79));
        assert_eq!(layout.slot_for_name("=+"), None);
    }

    #[test]
    fn every_physical_visual_override_id_exists_in_the_physical_table_and_actually_differs() {
        let layout = PhysicalKeyboardLayout::new();
        assert_eq!(
            layout.visual.physical.len(),
            21,
            "expected exactly 21 physical key renames"
        );
        for (&id, visual_glyph) in &layout.visual.physical {
            let canonical = layout.name_by_slot.get(&(id as u16)).unwrap_or_else(|| {
                panic!("physical override id {id} has no entry in physical_key_labels.json")
            });
            assert_ne!(
                canonical, visual_glyph,
                "slot {id}: override \"{visual_glyph}\" is identical to canonical \"{canonical}\" — override is redundant"
            );
        }
    }
}
