use std::collections::HashMap;

use serde_yaml_ng::{Mapping, Value};

use crate::codec::{DecodedMapping, KeyMappingCodec};
use crate::error::KeymapError;
use crate::layout::PhysicalKeyboardLayout;
use crate::mapping_type::KeyMappingType;
use crate::modifiers::ModifierSet;
use crate::protocol::KEYMATRIX_SLOT_COUNT;

const LAYER_NORMAL: u8 = 0;
const LAYER_FN: u8 = 1;
const LAYER_FN2: u8 = 2;

fn layer_key(layer: u8) -> &'static str {
    match layer {
        LAYER_NORMAL => "normal",
        LAYER_FN => "fn",
        LAYER_FN2 => "fn2",
        _ => unreachable!("only layers 0, 1 and 2 exist on the A72"),
    }
}

fn layer_for_key(key: &str) -> Option<u8> {
    match key {
        "normal" => Some(LAYER_NORMAL),
        "fn" => Some(LAYER_FN),
        "fn2" => Some(LAYER_FN2),
        _ => None,
    }
}

/// The factory-default KeyMatrix, dumped via `export-keymap` from a freshly reset A72
/// (Fn+SpaceL held 5s). Embedded so imports can merge onto a known baseline instead of
/// onto whatever the device currently holds.
const FACTORY_DEFAULT_YAML: &str = include_str!("../assets/factory-default.yaml");

pub struct KeymapYamlSerializer {
    codec: KeyMappingCodec,
    layout: PhysicalKeyboardLayout,
}

// `raw` is the original 4-byte value this slot was decoded from — passed alongside
// `decoded` rather than reconstructed from the enum's fields, because Macro/Custom
// variants don't carry a `kind`/`raw` field of their own (only Labeled/Unresolved do).
// This mirrors the JS version, whose `decoded` object always retains
// keyMappingType/keyMappingPara/keyCode regardless of which type-specific fields it
// also set, letting one raw-reconstruction formula cover every non-KeyBoard type.
fn slot_to_yaml_value(codec: &KeyMappingCodec, raw: u32, decoded: &DecodedMapping) -> Value {
    let mut map = Mapping::new();
    if let DecodedMapping::KeyBoard {
        key_code,
        symbol,
        modifiers,
    } = decoded
    {
        if *key_code != 0 && symbol.is_none() {
            map.insert("type".into(), "KeyBoard".into());
            map.insert(".comment".into(), decoded.label().into());
            map.insert("raw".into(), format!("0x{raw:08x}").into());
            return Value::Mapping(map);
        }
        map.insert("type".into(), "KeyBoard".into());
        if let Some(s) = symbol {
            map.insert("key".into(), s.clone().into());
        }
        if let Some(m) = modifiers.to_label() {
            map.insert("mod".into(), m.into());
        }
        return Value::Mapping(map);
    }

    // Covers Macro, Custom, Labeled, and Unresolved uniformly — all four render as
    // either {type, label} or {type, ".comment", raw}, never KeyBoard's key/mod shape.
    let type_name = KeyMappingType::from_byte((raw >> 24) as u8).type_name();
    let label = decoded.label();
    if codec.has_label(&label) {
        map.insert("type".into(), type_name.into());
        map.insert("label".into(), label.into());
        return Value::Mapping(map);
    }

    map.insert("type".into(), type_name.into());
    map.insert(".comment".into(), label.into());
    map.insert("raw".into(), format!("0x{raw:08x}").into());
    Value::Mapping(map)
}

impl KeymapYamlSerializer {
    pub fn new(codec: KeyMappingCodec, layout: PhysicalKeyboardLayout) -> Self {
        Self { codec, layout }
    }

    /// Renders one already-decoded slot value the same way `dump_yaml` renders a
    /// slot's entry for one layer — `{type, label}` / `{type, key, mod}` /
    /// `{type, ".comment", raw}` — for callers that want that shape for a single key
    /// (e.g. `get-keymap`) without producing a whole keymap document.
    pub fn describe_slot(&self, raw: u32) -> Value {
        let decoded = self.codec.decode(raw, None);
        slot_to_yaml_value(&self.codec, raw, &decoded)
    }

    /// Dumps every populated slot (`raw != 0`), regardless of whether it matches the
    /// factory default. Use [`Self::dump_yaml_diff`] for the compact, default-relative
    /// form.
    pub fn dump_yaml(&self, buffers_by_layer: &HashMap<u8, Vec<u8>>) -> String {
        self.dump_yaml_impl(buffers_by_layer, None)
    }

    /// Dumps only slots whose raw value differs from the embedded factory default for
    /// that slot and layer — the compact export form. Compares raw bytes, not decoded
    /// labels, so a difference invisible in the label (e.g. a Macro's repeat count) is
    /// still surfaced.
    pub fn dump_yaml_diff(&self, buffers_by_layer: &HashMap<u8, Vec<u8>>) -> String {
        let baseline: HashMap<u8, Vec<u8>> = [LAYER_NORMAL, LAYER_FN, LAYER_FN2]
            .into_iter()
            .map(|layer| (layer, self.factory_default_buffer(layer)))
            .collect();
        self.dump_yaml_impl(buffers_by_layer, Some(&baseline))
    }

    fn dump_yaml_impl(
        &self,
        buffers_by_layer: &HashMap<u8, Vec<u8>>,
        baseline: Option<&HashMap<u8, Vec<u8>>>,
    ) -> String {
        let mut doc = Mapping::new();
        for slot in 0..KEYMATRIX_SLOT_COUNT as u16 {
            let mut entry = Mapping::new();
            for &layer in &[LAYER_NORMAL, LAYER_FN, LAYER_FN2] {
                let Some(buf) = buffers_by_layer.get(&layer) else {
                    continue;
                };
                let offset = slot as usize * 4;
                let value = u32::from_be_bytes(buf[offset..offset + 4].try_into().unwrap());
                let skip_value = baseline
                    .and_then(|b| b.get(&layer))
                    .map(|b| u32::from_be_bytes(b[offset..offset + 4].try_into().unwrap()))
                    .unwrap_or(0);
                if value == skip_value {
                    continue;
                }
                let decoded = self.codec.decode(value, None);
                entry.insert(
                    layer_key(layer).into(),
                    slot_to_yaml_value(&self.codec, value, &decoded),
                );
            }
            if !entry.is_empty() {
                doc.insert(
                    self.layout.name_for_slot(slot).into(),
                    Value::Mapping(entry),
                );
            }
        }
        serde_yaml_ng::to_string(&Value::Mapping(doc)).expect("serializing a Mapping never fails")
    }
}

fn value_to_uint32(
    codec: &KeyMappingCodec,
    name: &str,
    layer_name: &str,
    value: &Value,
) -> Result<u32, KeymapError> {
    let Value::Mapping(map) = value else {
        return Err(KeymapError::ExpectedObject {
            name: name.to_string(),
            layer: layer_name.to_string(),
        });
    };

    if let Some(raw_value) = map.get("raw") {
        // `raw` is normally a quoted hex string ("0x02000192") the way dump_yaml
        // writes it, but serde_yaml_ng::Value has no Display/ToString impl, so an
        // already-numeric YAML scalar (an unquoted `raw: 0x02000192`, which YAML
        // resolves straight to a decimal integer, not a string to re-parse as hex)
        // is taken directly via as_u64 instead of being stringified and re-parsed.
        let raw = if let Some(s) = raw_value.as_str() {
            u32::from_str_radix(s.trim_start_matches("0x"), 16).map_err(|_| {
                KeymapError::InvalidRaw {
                    name: name.to_string(),
                    layer: layer_name.to_string(),
                    raw: s.to_string(),
                }
            })?
        } else if let Some(n) = raw_value.as_u64() {
            u32::try_from(n).map_err(|_| KeymapError::InvalidRaw {
                name: name.to_string(),
                layer: layer_name.to_string(),
                raw: n.to_string(),
            })?
        } else {
            return Err(KeymapError::InvalidRaw {
                name: name.to_string(),
                layer: layer_name.to_string(),
                raw: "<non-scalar>".to_string(),
            });
        };
        return Ok(KeyMappingCodec::encode_raw(raw));
    }

    if let Some(label_value) = map.get("label") {
        let label = label_value.as_str().unwrap_or_default();
        return codec
            .label_to_raw(label)
            .map(KeyMappingCodec::encode_raw)
            .ok_or_else(|| KeymapError::UnknownLabel {
                name: name.to_string(),
                layer: layer_name.to_string(),
                label: label.to_string(),
            });
    }

    if map.get("type").and_then(|v| v.as_str()) == Some("KeyBoard") {
        let mut key_code = 0u16;
        if let Some(key_value) = map.get("key") {
            let key = key_value.as_str().unwrap_or_default();
            key_code =
                codec
                    .symbol_to_keycode(key)
                    .ok_or_else(|| KeymapError::UnknownKeyboardSymbol {
                        name: name.to_string(),
                        layer: layer_name.to_string(),
                        key: key.to_string(),
                    })?;
        }
        let mut modifiers = ModifierSet::empty();
        if let Some(mod_value) = map.get("mod") {
            let mod_str = mod_value.as_str().unwrap_or_default();
            modifiers =
                ModifierSet::from_label(mod_str).map_err(|e| KeymapError::UnknownModifierIn {
                    name: name.to_string(),
                    layer: layer_name.to_string(),
                    inner: e.to_string(),
                    mod_value: mod_str.to_string(),
                })?;
        }
        return Ok(KeyMappingCodec::encode_keyboard(key_code, modifiers));
    }

    Err(KeymapError::MissingLabelOrRaw {
        name: name.to_string(),
        layer: layer_name.to_string(),
    })
}

impl KeymapYamlSerializer {
    pub fn parse_yaml(&self, text: &str) -> Result<HashMap<u8, HashMap<u16, u32>>, KeymapError> {
        let doc: Value = serde_yaml_ng::from_str(text)?;
        let mut result: HashMap<u8, HashMap<u16, u32>> = HashMap::new();
        result.insert(LAYER_NORMAL, HashMap::new());
        result.insert(LAYER_FN, HashMap::new());
        result.insert(LAYER_FN2, HashMap::new());

        let Value::Mapping(top) = doc else {
            return Ok(result); // an empty/null document parses to no slots
        };

        for (name_value, layers_value) in top {
            let name = name_value.as_str().unwrap_or_default().to_string();
            let slot = self
                .layout
                .slot_for_name(&name)
                .ok_or_else(|| KeymapError::UnknownPhysicalKey(name.clone()))?;

            let Value::Mapping(layers) = &layers_value else {
                return Err(KeymapError::ExpectedLayerObject(name));
            };

            for (layer_name_value, value) in layers {
                let layer_name = layer_name_value.as_str().unwrap_or_default().to_string();
                let layer =
                    layer_for_key(&layer_name).ok_or_else(|| KeymapError::UnknownLayerKey {
                        name: name.clone(),
                        layer: layer_name.clone(),
                    })?;
                let encoded = value_to_uint32(&self.codec, &name, &layer_name, value)?;
                result.get_mut(&layer).unwrap().insert(slot, encoded);
            }
        }

        Ok(result)
    }

    pub fn patch_buffer(&self, buffer: &mut [u8], slot_map: &HashMap<u16, u32>) {
        for (&slot, &value) in slot_map {
            let offset = slot as usize * 4;
            buffer[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
        }
    }

    /// The factory-default `{layer -> {slot -> value}}` maps, parsed from the
    /// embedded reference dump. Import paths merge a user config onto this
    /// baseline rather than onto whatever the device currently holds, so slots
    /// the config doesn't mention reset to factory instead of keeping stale
    /// customization.
    pub fn factory_default_slot_maps(&self) -> HashMap<u8, HashMap<u16, u32>> {
        self.parse_yaml(FACTORY_DEFAULT_YAML)
            .expect("embedded factory-default.yaml must always parse")
    }

    /// A factory-default raw buffer for one layer, built from
    /// [`Self::factory_default_slot_maps`]. Slots the default dump doesn't
    /// mention are left zeroed, matching how a freshly reset device's unused
    /// slots read.
    pub fn factory_default_buffer(&self, layer: u8) -> Vec<u8> {
        let mut buffer = vec![0u8; crate::protocol::KEYMATRIX_BUFFER_LEN];
        if let Some(slot_map) = self.factory_default_slot_maps().get(&layer) {
            self.patch_buffer(&mut buffer, slot_map);
        }
        buffer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_serializer() -> KeymapYamlSerializer {
        KeymapYamlSerializer::new(KeyMappingCodec::new(), PhysicalKeyboardLayout::new())
    }

    fn buffer_with(entries: &[(u16, u32)]) -> Vec<u8> {
        let mut buf = vec![0u8; crate::protocol::KEYMATRIX_BUFFER_LEN];
        for &(slot, value) in entries {
            let offset = slot as usize * 4;
            buf[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
        }
        buf
    }

    fn layers(layer0: Vec<u8>) -> HashMap<u8, Vec<u8>> {
        let mut map = HashMap::new();
        map.insert(0, layer0);
        map.insert(1, buffer_with(&[]));
        map
    }

    #[test]
    fn plain_key_on_one_layer_only() {
        let serializer = new_serializer();
        let esc = KeyMappingCodec::encode_keyboard(41, ModifierSet::empty());
        let text = serializer.dump_yaml(&layers(buffer_with(&[(7, esc)])));
        let doc: serde_yaml_ng::Value = serde_yaml_ng::from_str(&text).unwrap();
        assert_eq!(doc["Esc"]["normal"]["type"], "KeyBoard");
        assert_eq!(doc["Esc"]["normal"]["key"], "Esc");
    }

    #[test]
    fn combo_key_has_both_key_and_mod() {
        let serializer = new_serializer();
        let ctrl_c = KeyMappingCodec::encode_keyboard(6, ModifierSet::L_CTRL);
        let text = serializer.dump_yaml(&layers(buffer_with(&[(1, ctrl_c)])));
        let doc: serde_yaml_ng::Value = serde_yaml_ng::from_str(&text).unwrap();
        assert_eq!(doc["M5"]["normal"]["key"], "C");
        assert_eq!(doc["M5"]["normal"]["mod"], "LCtrl");
    }

    #[test]
    fn non_keyboard_entry_with_a_resolvable_label_uses_label_not_raw() {
        let serializer = new_serializer();
        let mute = KeyMappingCodec::encode_raw(0x020000e2);
        let text = serializer.dump_yaml(&layers(buffer_with(&[(104, mute)])));
        let doc: serde_yaml_ng::Value = serde_yaml_ng::from_str(&text).unwrap();
        assert_eq!(doc["Mute"]["normal"]["type"], "Media");
        assert_eq!(doc["Mute"]["normal"]["label"], "Mute");
        assert!(doc["Mute"]["normal"].get("raw").is_none());
    }

    #[test]
    fn non_keyboard_entry_with_an_unresolvable_label_falls_back_to_raw_and_comment() {
        let serializer = new_serializer();
        let unknown = KeyMappingCodec::encode_raw(0x07000099);
        let text = serializer.dump_yaml(&layers(buffer_with(&[(104, unknown)])));
        let doc: serde_yaml_ng::Value = serde_yaml_ng::from_str(&text).unwrap();
        assert_eq!(
            doc["Mute"]["normal"][".comment"],
            "Unknown(SpecialFun, raw=117440665)"
        );
        assert_eq!(doc["Mute"]["normal"]["raw"], "0x07000099");
    }

    #[test]
    fn populated_slot_with_no_known_name_falls_back_to_slotn() {
        let serializer = new_serializer();
        let a = KeyMappingCodec::encode_keyboard(4, ModifierSet::empty());
        let text = serializer.dump_yaml(&layers(buffer_with(&[(0, a)])));
        let doc: serde_yaml_ng::Value = serde_yaml_ng::from_str(&text).unwrap();
        assert_eq!(doc["slot0"]["normal"]["key"], "A");
    }

    #[test]
    fn macro_entry_dumps_as_comment_and_raw_not_a_panic() {
        // Regression test: Macro/Custom variants don't carry a `kind`/`raw` field of
        // their own (only Labeled/Unresolved do) — slot_to_yaml_value must still
        // handle them via the raw value passed in alongside the decoded enum, not by
        // trying to read a nonexistent field off the Macro variant.
        let serializer = new_serializer();
        let macro_entry = KeyMappingCodec::encode_raw(0x03000002); // Macro type, index 2
        let text = serializer.dump_yaml(&layers(buffer_with(&[(7, macro_entry)])));
        let doc: serde_yaml_ng::Value = serde_yaml_ng::from_str(&text).unwrap();
        assert_eq!(doc["Esc"]["normal"]["type"], "Macro");
        assert_eq!(doc["Esc"]["normal"][".comment"], "Macro #2");
        assert_eq!(doc["Esc"]["normal"]["raw"], "0x03000002");
    }

    #[test]
    fn round_trip_dump_then_parse_then_patch_reproduces_the_original_buffer() {
        let serializer = new_serializer();
        let ctrl_c = KeyMappingCodec::encode_keyboard(6, ModifierSet::L_CTRL);
        let mute = KeyMappingCodec::encode_raw(0x020000e2);
        let original0 = buffer_with(&[(1, ctrl_c), (104, mute)]);

        let text = serializer.dump_yaml(&layers(original0.clone()));
        let slot_maps = serializer.parse_yaml(&text).unwrap();

        let mut target = vec![0u8; crate::protocol::KEYMATRIX_BUFFER_LEN];
        serializer.patch_buffer(&mut target, &slot_maps[&0]);
        assert_eq!(target, original0);
        assert_eq!(slot_maps[&1].len(), 0);
    }

    #[test]
    fn parse_yaml_rejects_an_unknown_physical_key_name() {
        let serializer = new_serializer();
        let text = "NotAKey:\n  normal: { type: KeyBoard, key: A }\n";
        let err = serializer.parse_yaml(text).unwrap_err();
        assert!(matches!(err, KeymapError::UnknownPhysicalKey(name) if name == "NotAKey"));
    }

    #[test]
    fn parse_yaml_rejects_an_unknown_keyboard_key_symbol() {
        let serializer = new_serializer();
        let text = "Esc:\n  normal: { type: KeyBoard, key: NotAKey }\n";
        let err = serializer.parse_yaml(text).unwrap_err();
        assert!(matches!(err, KeymapError::UnknownKeyboardSymbol { key, .. } if key == "NotAKey"));
    }

    #[test]
    fn parse_yaml_rejects_an_unknown_modifier_name() {
        let serializer = new_serializer();
        let text = "Esc:\n  normal: { type: KeyBoard, key: A, mod: NotAMod }\n";
        let err = serializer.parse_yaml(text).unwrap_err();
        assert!(
            matches!(err, KeymapError::UnknownModifierIn { mod_value, .. } if mod_value == "NotAMod")
        );
    }

    #[test]
    fn parse_yaml_rejects_a_non_keyboard_entry_with_no_label_or_raw() {
        let serializer = new_serializer();
        let text = "Mute:\n  normal: { type: Media }\n";
        let err = serializer.parse_yaml(text).unwrap_err();
        assert!(matches!(err, KeymapError::MissingLabelOrRaw { .. }));
    }

    #[test]
    fn parse_yaml_rejects_an_unrecognized_label() {
        let serializer = new_serializer();
        let text = "Mute:\n  normal: { type: Media, label: NotALabel }\n";
        let err = serializer.parse_yaml(text).unwrap_err();
        assert!(matches!(err, KeymapError::UnknownLabel { label, .. } if label == "NotALabel"));
    }

    #[test]
    fn round_trip_through_a_resolvable_label_reproduces_the_original_raw() {
        let serializer = new_serializer();
        let mute = KeyMappingCodec::encode_raw(0x020000e2);
        let original0 = buffer_with(&[(104, mute)]);
        let text = serializer.dump_yaml(&layers(original0.clone()));
        let slot_maps = serializer.parse_yaml(&text).unwrap();
        let mut target = vec![0u8; crate::protocol::KEYMATRIX_BUFFER_LEN];
        serializer.patch_buffer(&mut target, &slot_maps[&0]);
        assert_eq!(target, original0);
    }

    #[test]
    fn parse_yaml_rejects_an_unrecognized_layer_key() {
        let serializer = new_serializer();
        let text = "Esc:\n  weird: { type: KeyBoard, key: A }\n";
        let err = serializer.parse_yaml(text).unwrap_err();
        assert!(matches!(err, KeymapError::UnknownLayerKey { layer, .. } if layer == "weird"));
    }

    #[test]
    fn parse_yaml_ignores_slots_not_mentioned_in_the_yaml() {
        let serializer = new_serializer();
        let slot_maps = serializer
            .parse_yaml("Esc:\n  normal: { type: KeyBoard, key: A }\n")
            .unwrap();
        assert_eq!(slot_maps[&0].len(), 1);
        assert_eq!(slot_maps[&1].len(), 0);
    }

    #[test]
    fn keyboard_keycode_with_no_known_symbol_falls_back_to_raw_and_comment() {
        let serializer = new_serializer();
        assert_eq!(serializer.codec.keycode_symbol(135), None);
        let intl1 = KeyMappingCodec::encode_keyboard(135, ModifierSet::empty());
        let original0 = buffer_with(&[(7, intl1)]);

        let text = serializer.dump_yaml(&layers(original0.clone()));
        let doc: serde_yaml_ng::Value = serde_yaml_ng::from_str(&text).unwrap();
        assert_eq!(doc["Esc"]["normal"][".comment"], "key(135)");
        assert_eq!(doc["Esc"]["normal"]["raw"], "0x00000087");

        let slot_maps = serializer.parse_yaml(&text).unwrap();
        let mut target = vec![0u8; crate::protocol::KEYMATRIX_BUFFER_LEN];
        serializer.patch_buffer(&mut target, &slot_maps[&0]);
        assert_eq!(target, original0);
    }

    #[test]
    fn raw_wins_over_key_mod_when_both_are_present() {
        let serializer = new_serializer();
        let text = "Esc:\n  normal: { type: KeyBoard, key: A, raw: \"0x02000192\" }\n";
        let slot_maps = serializer.parse_yaml(text).unwrap();
        assert_eq!(slot_maps[&0][&7], KeyMappingCodec::encode_raw(0x02000192));
    }

    #[test]
    fn dump_yaml_diff_omits_slots_that_match_the_factory_default() {
        let serializer = new_serializer();
        let layer0 = serializer.factory_default_buffer(LAYER_NORMAL);
        let layer1 = serializer.factory_default_buffer(LAYER_FN);
        let mut buffers = HashMap::new();
        buffers.insert(LAYER_NORMAL, layer0);
        buffers.insert(LAYER_FN, layer1);

        let text = serializer.dump_yaml_diff(&buffers);
        let doc: serde_yaml_ng::Value = serde_yaml_ng::from_str(&text).unwrap();
        assert!(
            doc.as_mapping().is_none_or(|m| m.is_empty()),
            "expected an empty document when the buffers exactly match factory default, got: {text}"
        );
    }

    #[test]
    fn dump_yaml_diff_includes_only_the_slot_that_was_changed() {
        let serializer = new_serializer();
        let mut layer0 = serializer.factory_default_buffer(LAYER_NORMAL);
        let layer1 = serializer.factory_default_buffer(LAYER_FN);

        // Esc is slot 7 (see the other tests in this module) — flip it away from
        // whatever the factory default encodes.
        let b = KeyMappingCodec::encode_keyboard(5, ModifierSet::empty()); // "B"
        layer0[7 * 4..7 * 4 + 4].copy_from_slice(&b.to_be_bytes());

        let mut buffers = HashMap::new();
        buffers.insert(LAYER_NORMAL, layer0);
        buffers.insert(LAYER_FN, layer1);

        let text = serializer.dump_yaml_diff(&buffers);
        let doc: serde_yaml_ng::Value = serde_yaml_ng::from_str(&text).unwrap();
        let map = doc.as_mapping().expect("expected a mapping");
        assert_eq!(map.len(), 1, "expected exactly one changed key, got: {text}");
        assert_eq!(doc["Esc"]["normal"]["key"], "B");
    }

    #[test]
    fn parse_yaml_rejects_an_out_of_range_slotn_fallback_name() {
        let serializer = new_serializer();
        let err = serializer
            .parse_yaml("slot9999:\n  normal: { type: KeyBoard, key: A }\n")
            .unwrap_err();
        assert!(matches!(err, KeymapError::UnknownPhysicalKey(name) if name == "slot9999"));
    }
}
