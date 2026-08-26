use std::collections::HashMap;

use crate::mapping_type::KeyMappingType;
use crate::modifiers::ModifierSet;

#[derive(Debug, Clone, PartialEq)]
pub enum DecodedMapping {
    KeyBoard {
        key_code: u16,
        symbol: Option<String>,
        modifiers: ModifierSet,
    },
    Macro {
        index: u8,
        repeat_count: u8,
        name: Option<String>,
    },
    Custom {
        code: u16,
    },
    Labeled {
        kind: KeyMappingType,
        raw: u32,
        label: String,
    },
    Unresolved {
        kind: KeyMappingType,
        raw: u32,
    },
}

impl DecodedMapping {
    pub fn label(&self) -> String {
        match self {
            DecodedMapping::KeyBoard {
                key_code,
                symbol,
                modifiers,
            } => {
                let key_part = symbol.clone().unwrap_or_else(|| format!("key({key_code})"));
                match modifiers.to_label() {
                    Some(m) => format!("{m}+{key_part}"),
                    None => key_part,
                }
            }
            DecodedMapping::Macro { index, name, .. } => match name {
                Some(n) => format!("Macro: {n}"),
                None => format!("Macro #{index}"),
            },
            DecodedMapping::Custom { code } => format!("Define {code}"),
            DecodedMapping::Labeled { label, .. } => label.clone(),
            DecodedMapping::Unresolved { kind, raw } => {
                format!("Unknown({}, raw={})", kind.type_name(), raw)
            }
        }
    }
}

pub struct KeyMappingCodec {
    key_labels: HashMap<u32, String>,
    raw_by_label: HashMap<String, u32>,
    hid_keycodes: HashMap<u16, String>,
    keycode_by_symbol: HashMap<String, u16>,
    visual: crate::visual::VisualOverrides,
}

fn to_key_raw(type_byte: u8, para: u8, key_code: u16) -> u32 {
    ((type_byte as u32) << 24) | ((para as u32) << 16) | (key_code as u32)
}

// Types decode()/encode_raw() consult `key_labels` for — everything except KeyBoard
// (own symbol/mod fields), Macro (own index/repeat fields), and Custom (a bare
// numeric code, no lookup). Matches JS's LABEL_LOOKUP_EXCLUDED_TYPES.
fn is_label_lookup_excluded(type_byte: u8) -> bool {
    matches!(type_byte, 0 | 3 | 4)
}

fn parse_key_labels(json: &str) -> HashMap<u32, String> {
    let raw: HashMap<String, Vec<String>> =
        serde_json::from_str(json).expect("key_mapping_table.json must be valid JSON");
    raw.into_iter()
        .filter_map(|(k, v)| {
            let raw_val: u32 = k.parse().ok()?;
            v.into_iter().next().map(|label| (raw_val, label))
        })
        .collect()
}

fn parse_hid_keycodes(json: &str) -> HashMap<u16, String> {
    let raw: HashMap<String, String> =
        serde_json::from_str(json).expect("hid_keycode_table.json must be valid JSON");
    raw.into_iter()
        .filter_map(|(k, v)| k.parse::<u16>().ok().map(|code| (code, v)))
        .collect()
}

impl KeyMappingCodec {
    pub fn new() -> Self {
        let key_labels = parse_key_labels(include_str!("../data/key_mapping_table.json"));
        let hid_keycodes = parse_hid_keycodes(include_str!("../data/hid_keycode_table.json"));
        let keycode_by_symbol = hid_keycodes
            .iter()
            .map(|(&code, symbol)| (symbol.clone(), code))
            .collect();
        let mut raw_by_label = HashMap::new();
        for (&raw, label) in &key_labels {
            let type_byte = (raw >> 24) as u8;
            if is_label_lookup_excluded(type_byte) {
                continue;
            }
            raw_by_label.insert(label.clone(), raw);
        }
        Self {
            key_labels,
            raw_by_label,
            hid_keycodes,
            keycode_by_symbol,
            visual: crate::visual::VisualOverrides::new(),
        }
    }

    pub fn decode(&self, value: u32, macro_names: Option<&[String]>) -> DecodedMapping {
        let type_byte = (value >> 24) as u8;
        let para = ((value >> 16) & 0xff) as u8;
        let key_code = (value & 0xffff) as u16;
        let kind = KeyMappingType::from_byte(type_byte);

        match kind {
            KeyMappingType::KeyBoard => DecodedMapping::KeyBoard {
                key_code,
                symbol: self.keycode_symbol(key_code),
                modifiers: ModifierSet::from_bits_truncate(para),
            },
            KeyMappingType::Macro => {
                let index = (key_code & 0xff) as u8;
                let repeat_count = (key_code >> 8) as u8;
                let name = macro_names
                    .and_then(|names| names.get(index as usize))
                    .cloned();
                DecodedMapping::Macro {
                    index,
                    repeat_count,
                    name,
                }
            }
            KeyMappingType::Custom => DecodedMapping::Custom { code: key_code },
            _ => {
                let raw = to_key_raw(type_byte, para, key_code);
                match self.key_labels.get(&raw) {
                    Some(label) => DecodedMapping::Labeled {
                        kind,
                        raw,
                        label: label.clone(),
                    },
                    None => DecodedMapping::Unresolved { kind, raw },
                }
            }
        }
    }

    pub fn encode_raw(raw: u32) -> u32 {
        raw
    }

    pub fn encode_keyboard(key_code: u16, modifiers: ModifierSet) -> u32 {
        to_key_raw(
            KeyMappingType::KeyBoard.to_byte(),
            modifiers.bits(),
            key_code,
        )
    }

    pub fn keycode_symbol(&self, code: u16) -> Option<String> {
        self.hid_keycodes.get(&code).cloned()
    }

    pub fn symbol_to_keycode(&self, symbol: &str) -> Option<u16> {
        self.keycode_by_symbol.get(symbol).copied()
    }

    pub fn has_label(&self, label: &str) -> bool {
        self.raw_by_label.contains_key(label)
    }

    pub fn label_to_raw(&self, label: &str) -> Option<u32> {
        self.raw_by_label.get(label).copied()
    }

    pub fn list_labels(&self) -> Vec<(String, u32, String)> {
        let mut v: Vec<_> = self
            .raw_by_label
            .iter()
            .map(|(l, &r)| (l.clone(), r, self.visual.label(r, l)))
            .collect();
        v.sort_by_key(|(_, r, _)| *r);
        v
    }

    pub fn list_keycode_symbols(&self) -> Vec<(u16, String, String)> {
        let mut v: Vec<_> = self
            .hid_keycodes
            .iter()
            .map(|(&c, s)| (c, s.clone(), self.visual.keycode(c as u32, s)))
            .collect();
        v.sort_by_key(|(c, _, _)| *c);
        v
    }

    pub fn list_modifier_names(&self) -> Vec<(u8, &'static str)> {
        crate::modifiers::ModifierSet::list_named()
    }
}

impl Default for KeyMappingCodec {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_encode_round_trips_a_plain_keyboard_key() {
        let codec = KeyMappingCodec::new();
        let raw = KeyMappingCodec::encode_keyboard(4, ModifierSet::empty());
        let decoded = codec.decode(raw, None);
        assert_eq!(
            decoded,
            DecodedMapping::KeyBoard {
                key_code: 4,
                symbol: Some("A".to_string()),
                modifiers: ModifierSet::empty(),
            }
        );
        assert_eq!(decoded.label(), "A");
    }

    #[test]
    fn decode_labels_a_keyboard_combo_with_modifier() {
        let codec = KeyMappingCodec::new();
        let raw = KeyMappingCodec::encode_keyboard(6, ModifierSet::L_CTRL);
        assert_eq!(codec.decode(raw, None).label(), "LCtrl+C");
    }

    #[test]
    fn decode_resolves_a_non_keyboard_label_media_mute() {
        let codec = KeyMappingCodec::new();
        let decoded = codec.decode(0x020000e2, None);
        match &decoded {
            DecodedMapping::Labeled { kind, label, .. } => {
                assert_eq!(kind.type_name(), "Media");
                assert_eq!(label, "Mute");
            }
            other => panic!("expected Labeled, got {other:?}"),
        }
    }

    #[test]
    fn decode_falls_back_to_unresolved_for_an_unrecognized_raw() {
        let codec = KeyMappingCodec::new();
        let decoded = codec.decode(0x07000099, None);
        assert_eq!(decoded.label(), "Unknown(SpecialFun, raw=117440665)");
    }

    #[test]
    fn decode_labels_a_macro_entry_by_index_with_and_without_a_name() {
        let codec = KeyMappingCodec::new();
        let raw = KeyMappingCodec::encode_raw(0x03000002);
        assert_eq!(codec.decode(raw, None).label(), "Macro #2");
        let names = vec!["a".to_string(), "b".to_string(), "MyMacro".to_string()];
        assert_eq!(codec.decode(raw, Some(&names)).label(), "Macro: MyMacro");
    }

    #[test]
    fn decode_labels_type_17_touch_via_the_rk_logo_touch_action() {
        let codec = KeyMappingCodec::new();
        let decoded = codec.decode(285212672, None);
        match &decoded {
            DecodedMapping::Labeled { kind, label, .. } => {
                assert_eq!(kind.type_name(), "Touch");
                assert_eq!(label, "TouchWWW");
            }
            other => panic!("expected Labeled, got {other:?}"),
        }
    }

    #[test]
    fn label_to_raw_has_label_are_the_inverse_of_decode_for_non_keyboard_types() {
        let codec = KeyMappingCodec::new();
        assert!(codec.has_label("Mute"));
        assert_eq!(codec.label_to_raw("Mute"), Some(0x020000e2));
        assert!(!codec.has_label("NotALabel"));
        assert_eq!(codec.label_to_raw("NotALabel"), None);
    }

    #[test]
    fn keycode_symbol_symbol_to_keycode_round_trip_and_report_unknowns() {
        let codec = KeyMappingCodec::new();
        assert_eq!(codec.keycode_symbol(4), Some("A".to_string()));
        assert_eq!(codec.symbol_to_keycode("A"), Some(4));
        assert_eq!(codec.keycode_symbol(135), None);
        assert_eq!(codec.symbol_to_keycode("NotAKey"), None);
    }

    #[test]
    fn renamed_keyboard_symbols_encode_decode_and_label_by_their_new_canonical_name() {
        let codec = KeyMappingCodec::new();
        let minus_code = codec
            .symbol_to_keycode("Minus")
            .expect("Minus must resolve");
        let raw = KeyMappingCodec::encode_keyboard(minus_code, ModifierSet::empty());
        assert_eq!(codec.decode(raw, None).label(), "Minus");
        assert_eq!(codec.symbol_to_keycode("-"), None); // old glyph is no longer valid input

        let backtick_code = codec
            .symbol_to_keycode("Backtick")
            .expect("Backtick must resolve");
        let raw = KeyMappingCodec::encode_keyboard(backtick_code, ModifierSet::empty());
        assert_eq!(codec.decode(raw, None).label(), "Backtick");
    }

    #[test]
    fn renamed_labels_resolve_by_their_new_canonical_name_only() {
        let codec = KeyMappingCodec::new();
        assert!(codec.has_label("EMITest"));
        assert!(!codec.has_label("EMI Test"));
        assert!(codec.has_label("RKWeb"));
        assert!(!codec.has_label("RK Web"));
    }

    #[test]
    fn every_keycode_visual_override_id_exists_in_the_keycode_table_and_actually_differs() {
        let codec = KeyMappingCodec::new();
        assert_eq!(
            codec.visual.keycode.len(),
            12,
            "expected exactly 12 keycode renames"
        );
        for (&id, visual_glyph) in &codec.visual.keycode {
            let canonical = codec.hid_keycodes.get(&(id as u16)).unwrap_or_else(|| {
                panic!("keycode override id {id} has no entry in hid_keycode_table.json")
            });
            assert_ne!(
                canonical, visual_glyph,
                "keycode {id}: override \"{visual_glyph}\" is identical to canonical \"{canonical}\" — override is redundant"
            );
        }
    }

    #[test]
    fn every_label_visual_override_id_exists_in_the_label_table_and_actually_differs() {
        let codec = KeyMappingCodec::new();
        assert_eq!(
            codec.visual.label.len(),
            2,
            "expected exactly 2 label renames"
        );
        for (&id, visual_glyph) in &codec.visual.label {
            let canonical = codec.key_labels.get(&id).unwrap_or_else(|| {
                panic!("label override id {id} has no entry in key_mapping_table.json")
            });
            assert_ne!(
                canonical, visual_glyph,
                "label {id}: override \"{visual_glyph}\" is identical to canonical \"{canonical}\" — override is redundant"
            );
        }
    }
}
