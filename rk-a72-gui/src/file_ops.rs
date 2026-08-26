use std::collections::HashMap;

use rk_a72_keymap::{KeyMappingCodec, PhysicalKeyboardLayout};

#[derive(Debug, Clone, PartialEq)]
pub struct ImportDiffEntry {
    pub name: String,
    pub layer: u8,
    pub before: String,
    pub after: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportSummary {
    pub entries: Vec<ImportDiffEntry>,
    pub unchanged: u32,
}

pub fn build_import_summary(
    codec: &KeyMappingCodec,
    layout: &PhysicalKeyboardLayout,
    current_buffers: &HashMap<u8, Vec<u8>>,
    slot_maps: &HashMap<u8, HashMap<u16, u32>>,
) -> ImportSummary {
    let mut entries = Vec::new();
    let mut unchanged = 0u32;

    for layer in [0u8, 1u8, 2u8] {
        let Some(slot_map) = slot_maps.get(&layer) else {
            continue;
        };
        let Some(buffer) = current_buffers.get(&layer) else {
            continue;
        };
        let mut sorted_slots: Vec<(u16, u32)> = slot_map.iter().map(|(&s, &v)| (s, v)).collect();
        sorted_slots.sort_unstable_by_key(|(slot, _)| *slot);

        for (slot, value) in sorted_slots {
            let offset = slot as usize * 4;
            let before = codec
                .decode(
                    u32::from_be_bytes(buffer[offset..offset + 4].try_into().unwrap()),
                    None,
                )
                .label();
            let after = codec.decode(value, None).label();
            if before == after {
                unchanged += 1;
                continue;
            }
            entries.push(ImportDiffEntry {
                name: layout.name_for_slot(slot),
                layer,
                before,
                after,
            });
        }
    }

    ImportSummary { entries, unchanged }
}

use crate::app::GuiApp;

pub fn export_to_file(app: &mut GuiApp) {
    let Some(path) = rfd::FileDialog::new()
        .add_filter("YAML", &["yaml", "yml"])
        .save_file()
    else {
        return;
    };
    let text = app.yaml_serializer().dump_yaml(&app.layer_buffers);
    match std::fs::write(&path, text) {
        Ok(()) => app.status_message = Some(format!("Exported to {}", path.display())),
        Err(e) => app.status_message = Some(format!("Failed to write {}: {e}", path.display())),
    }
}

pub fn pick_import_file() -> Option<std::path::PathBuf> {
    rfd::FileDialog::new()
        .add_filter("YAML", &["yaml", "yml"])
        .pick_file()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rk_a72_keymap::{KeyMappingCodec as Codec, ModifierSet};

    fn buffer_with(entries: &[(u16, u32)]) -> Vec<u8> {
        let mut buf =
            vec![0u8; rk_a72_keymap::PhysicalKeyboardLayout::KEYMATRIX_SLOT_COUNT as usize * 4];
        for &(slot, value) in entries {
            let offset = slot as usize * 4;
            buf[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
        }
        buf
    }

    #[test]
    fn reports_a_changed_slot_with_before_and_after_labels() {
        let codec = Codec::new();
        let layout = PhysicalKeyboardLayout::new();
        let esc = Codec::encode_keyboard(41, ModifierSet::empty()); // slot 7 = Esc
        let mute = Codec::encode_raw(0x020000e2);

        let mut current = HashMap::new();
        current.insert(0u8, buffer_with(&[(7, esc)]));

        let mut slot_maps = HashMap::new();
        let mut layer0_changes = HashMap::new();
        layer0_changes.insert(7u16, mute);
        slot_maps.insert(0u8, layer0_changes);
        slot_maps.insert(1u8, HashMap::new());

        let summary = build_import_summary(&codec, &layout, &current, &slot_maps);
        assert_eq!(summary.entries.len(), 1);
        assert_eq!(summary.entries[0].name, "Esc");
        assert_eq!(summary.entries[0].layer, 0);
        assert_eq!(summary.entries[0].before, "Esc");
        assert_eq!(summary.entries[0].after, "Mute");
        assert_eq!(summary.unchanged, 0);
    }

    #[test]
    fn counts_an_identical_relabeled_value_as_unchanged() {
        let codec = Codec::new();
        let layout = PhysicalKeyboardLayout::new();
        let esc = Codec::encode_keyboard(41, ModifierSet::empty());

        let mut current = HashMap::new();
        current.insert(0u8, buffer_with(&[(7, esc)]));

        let mut slot_maps = HashMap::new();
        let mut layer0_changes = HashMap::new();
        layer0_changes.insert(7u16, esc); // same value written back
        slot_maps.insert(0u8, layer0_changes);
        slot_maps.insert(1u8, HashMap::new());

        let summary = build_import_summary(&codec, &layout, &current, &slot_maps);
        assert_eq!(summary.entries.len(), 0);
        assert_eq!(summary.unchanged, 1);
    }

    #[test]
    fn empty_slot_maps_produce_an_empty_summary() {
        let codec = Codec::new();
        let layout = PhysicalKeyboardLayout::new();
        let mut current = HashMap::new();
        current.insert(0u8, buffer_with(&[]));
        current.insert(1u8, buffer_with(&[]));
        let mut slot_maps = HashMap::new();
        slot_maps.insert(0u8, HashMap::new());
        slot_maps.insert(1u8, HashMap::new());

        let summary = build_import_summary(&codec, &layout, &current, &slot_maps);
        assert_eq!(summary.entries.len(), 0);
        assert_eq!(summary.unchanged, 0);
    }

    #[test]
    fn multiple_changed_slots_in_one_layer_are_sorted_by_slot() {
        let codec = Codec::new();
        let layout = PhysicalKeyboardLayout::new();
        let esc = Codec::encode_keyboard(41, ModifierSet::empty()); // slot 7
        let mute = Codec::encode_raw(0x020000e2);
        let vol_up = Codec::encode_raw(0x020000e9);

        // Set slot 7 (Esc) and slot 3 (initially empty) in current state
        let mut current = HashMap::new();
        current.insert(0u8, buffer_with(&[(7, esc), (3, 0u32)]));

        let mut slot_maps = HashMap::new();
        let mut layer0_changes = HashMap::new();
        // Insert slot 7 first, then slot 3 — should be returned in slot order (3, 7)
        layer0_changes.insert(7u16, mute);
        layer0_changes.insert(3u16, vol_up);
        slot_maps.insert(0u8, layer0_changes);
        slot_maps.insert(1u8, HashMap::new());

        let summary = build_import_summary(&codec, &layout, &current, &slot_maps);
        assert_eq!(summary.entries.len(), 2);
        // Verify entries are in slot order: slot 3 before slot 7
        assert_eq!(summary.entries[0].name, layout.name_for_slot(3));
        assert_eq!(summary.entries[1].name, layout.name_for_slot(7));
        assert_eq!(summary.unchanged, 0);
    }

    #[test]
    fn entries_from_multiple_layers_come_back_in_layer_order_not_hashmap_order() {
        let codec = Codec::new();
        let layout = PhysicalKeyboardLayout::new();
        let esc = Codec::encode_keyboard(41, ModifierSet::empty());
        let mute = Codec::encode_raw(0x020000e2);

        let mut current = HashMap::new();
        current.insert(0u8, buffer_with(&[(7, esc)]));
        current.insert(1u8, buffer_with(&[(7, esc)]));

        let mut slot_maps = HashMap::new();
        let mut layer0_changes = HashMap::new();
        layer0_changes.insert(7u16, mute);
        slot_maps.insert(0u8, layer0_changes);
        let mut layer1_changes = HashMap::new();
        layer1_changes.insert(7u16, mute);
        slot_maps.insert(1u8, layer1_changes);

        let summary = build_import_summary(&codec, &layout, &current, &slot_maps);
        assert_eq!(summary.entries.len(), 2);
        assert_eq!(
            summary.entries[0].layer, 0,
            "layer-0 entries must come before layer-1 entries"
        );
        assert_eq!(summary.entries[1].layer, 1);
    }
}
