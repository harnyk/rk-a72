//! The factory-default KeyMatrix for the RK A72, expressed semantically in Rust — the
//! merge base every keymap import resets un-mentioned slots to. Each entry is a physical
//! key, a layer, and the action that key produces out of the box, dumped from a freshly
//! reset A72 (Fn+SpaceL held 5s) and verified against USB captures.
//!
//! This replaces the old `assets/factory-default.yaml`: the data lives in code, resolved
//! through the same [`KeyMappingCodec`] the rest of the crate uses, so it depends on no
//! text config format (neither YAML nor HCL). "fn2" has no factory mappings — every key
//! is unbound on it out of the box.

use std::collections::HashMap;

use crate::codec::KeyMappingCodec;
use crate::modifiers::ModifierSet;
use crate::physical_key::PhysicalKey;
use crate::protocol::KEYMATRIX_BUFFER_LEN;

/// Which of the A72's three layers a factory mapping belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Layer {
    Normal = 0,
    Fn = 1,
    Fn2 = 2,
}

/// A factory key action, in the same three flavours the KeyMatrix write path understands.
/// Symbols, modifier names and labels are resolved to raw slot values through the codec
/// at load time — the table stays readable (`Action::key("Esc")`), not hex.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Action {
    /// A KeyBoard key (optionally with modifiers), by its KeySymbol and modifier names.
    Key {
        symbol: &'static str,
        mods: &'static [&'static str],
    },
    /// Modifier(s) pressed on their own, with no key — e.g. a bare Shift.
    Mods(&'static [&'static str]),
    /// A non-KeyBoard action, by its label (Media/SpecialFun/...).
    Label(&'static str),
}

impl Action {
    pub(crate) const fn key(symbol: &'static str) -> Action {
        Action::Key { symbol, mods: &[] }
    }

    pub(crate) const fn key_mods(symbol: &'static str, mods: &'static [&'static str]) -> Action {
        Action::Key { symbol, mods }
    }

    pub(crate) const fn mods(mods: &'static [&'static str]) -> Action {
        Action::Mods(mods)
    }

    pub(crate) const fn label(label: &'static str) -> Action {
        Action::Label(label)
    }

    /// Resolve this action to its raw 4-byte KeyMatrix slot value. Panics on a symbol,
    /// modifier or label the codec doesn't know — the table is a compile-time constant of
    /// the crate's own, so an unknown token is a bug to catch in tests, not a user error.
    fn to_slot_value(self, codec: &KeyMappingCodec, context: &PhysicalKey) -> u32 {
        let resolve_mods = |names: &[&str]| {
            names.iter().fold(ModifierSet::empty(), |acc, name| {
                acc | ModifierSet::from_label(name)
                    .unwrap_or_else(|_| panic!("factory default {context:?}: unknown modifier {name:?}"))
            })
        };
        match self {
            Action::Key { symbol, mods } => {
                let key_code = if symbol.is_empty() {
                    0
                } else {
                    codec.symbol_to_keycode(symbol).unwrap_or_else(|| {
                        panic!("factory default {context:?}: unknown key symbol {symbol:?}")
                    })
                };
                KeyMappingCodec::encode_keyboard(key_code, resolve_mods(mods))
            }
            Action::Mods(mods) => KeyMappingCodec::encode_keyboard(0, resolve_mods(mods)),
            Action::Label(label) => {
                KeyMappingCodec::encode_raw(codec.label_to_raw(label).unwrap_or_else(|| {
                    panic!("factory default {context:?}: unknown label {label:?}")
                }))
            }
        }
    }
}

/// The factory-default mappings: (physical key, layer, action). Un-listed (key, layer)
/// pairs are unbound out of the box.
pub(crate) const FACTORY_DEFAULT: &[(PhysicalKey, Layer, Action)] = &[
    (PhysicalKey::M5, Layer::Normal, Action::key_mods("C", &["LCtrl"])),
    (PhysicalKey::M4, Layer::Normal, Action::key_mods("V", &["LCtrl"])),
    (PhysicalKey::M3, Layer::Normal, Action::key_mods("A", &["LCtrl"])),
    (PhysicalKey::M2, Layer::Normal, Action::key_mods("X", &["LCtrl"])),
    (PhysicalKey::M1, Layer::Normal, Action::key_mods("Z", &["LCtrl"])),
    (PhysicalKey::Esc, Layer::Normal, Action::key("Esc")),
    (PhysicalKey::Tab, Layer::Normal, Action::key("Tab")),
    (PhysicalKey::CapsLock, Layer::Normal, Action::key("CapsLock")),
    (PhysicalKey::LShift, Layer::Normal, Action::mods(&["LShift"])),
    (PhysicalKey::LCtrl, Layer::Normal, Action::mods(&["LCtrl"])),
    (PhysicalKey::Digit1, Layer::Normal, Action::key("1")),
    (PhysicalKey::Q, Layer::Normal, Action::key("Q")),
    (PhysicalKey::A, Layer::Normal, Action::key("A")),
    (PhysicalKey::IntlBackslash, Layer::Normal, Action::key("IntlBackslash")),
    (PhysicalKey::LWin, Layer::Normal, Action::mods(&["LWin"])),
    (PhysicalKey::Digit2, Layer::Normal, Action::key("2")),
    (PhysicalKey::W, Layer::Normal, Action::key("W")),
    (PhysicalKey::S, Layer::Normal, Action::key("S")),
    (PhysicalKey::Z, Layer::Normal, Action::key("Z")),
    (PhysicalKey::LAlt, Layer::Normal, Action::mods(&["LAlt"])),
    (PhysicalKey::Digit3, Layer::Normal, Action::key("3")),
    (PhysicalKey::E, Layer::Normal, Action::key("E")),
    (PhysicalKey::D, Layer::Normal, Action::key("D")),
    (PhysicalKey::X, Layer::Normal, Action::key("X")),
    (PhysicalKey::Digit4, Layer::Normal, Action::key("4")),
    (PhysicalKey::R, Layer::Normal, Action::key("R")),
    (PhysicalKey::F, Layer::Normal, Action::key("F")),
    (PhysicalKey::C, Layer::Normal, Action::key("C")),
    (PhysicalKey::Digit5, Layer::Normal, Action::key("5")),
    (PhysicalKey::T, Layer::Normal, Action::key("T")),
    (PhysicalKey::G, Layer::Normal, Action::key("G")),
    (PhysicalKey::V, Layer::Normal, Action::key("V")),
    (PhysicalKey::SpaceL, Layer::Normal, Action::key("Space")),
    (PhysicalKey::Digit6, Layer::Normal, Action::key("6")),
    (PhysicalKey::Y, Layer::Normal, Action::key("Y")),
    (PhysicalKey::H, Layer::Normal, Action::key("H")),
    (PhysicalKey::B, Layer::Normal, Action::key("B")),
    (PhysicalKey::Digit7, Layer::Normal, Action::key("7")),
    (PhysicalKey::U, Layer::Normal, Action::key("U")),
    (PhysicalKey::J, Layer::Normal, Action::key("J")),
    (PhysicalKey::N, Layer::Normal, Action::key("N")),
    (PhysicalKey::SpaceR, Layer::Normal, Action::key("Space")),
    (PhysicalKey::Digit8, Layer::Normal, Action::key("8")),
    (PhysicalKey::I, Layer::Normal, Action::key("I")),
    (PhysicalKey::K, Layer::Normal, Action::key("K")),
    (PhysicalKey::M, Layer::Normal, Action::key("M")),
    (PhysicalKey::Digit9, Layer::Normal, Action::key("9")),
    (PhysicalKey::O, Layer::Normal, Action::key("O")),
    (PhysicalKey::L, Layer::Normal, Action::key("L")),
    (PhysicalKey::Comma, Layer::Normal, Action::key(",")),
    (PhysicalKey::Digit0, Layer::Normal, Action::key("0")),
    (PhysicalKey::P, Layer::Normal, Action::key("P")),
    (PhysicalKey::Semicolon, Layer::Normal, Action::key("Semicolon")),
    (PhysicalKey::Period, Layer::Normal, Action::key(".")),
    (PhysicalKey::RAlt, Layer::Normal, Action::mods(&["RAlt"])),
    (PhysicalKey::Minus, Layer::Normal, Action::key("Minus")),
    (PhysicalKey::BracketLeft, Layer::Normal, Action::key("[")),
    (PhysicalKey::Quote, Layer::Normal, Action::key("Quote")),
    (PhysicalKey::Slash, Layer::Normal, Action::key("/")),
    (PhysicalKey::Equal, Layer::Normal, Action::key("=")),
    (PhysicalKey::BracketRight, Layer::Normal, Action::key("]")),
    (PhysicalKey::Hash, Layer::Normal, Action::key("Hash")),
    (PhysicalKey::Fn1, Layer::Normal, Action::label("Fn1")),
    (PhysicalKey::Backspace, Layer::Normal, Action::key("Backspace")),
    (PhysicalKey::Backslash, Layer::Normal, Action::key("Backslash")),
    (PhysicalKey::Enter, Layer::Normal, Action::key("Enter")),
    (PhysicalKey::RShift, Layer::Normal, Action::mods(&["RShift"])),
    (PhysicalKey::Left, Layer::Normal, Action::key("Left")),
    (PhysicalKey::Up, Layer::Normal, Action::key("Up")),
    (PhysicalKey::Down, Layer::Normal, Action::key("Down")),
    (PhysicalKey::Del, Layer::Normal, Action::key("Delete")),
    (PhysicalKey::PgUp, Layer::Normal, Action::key("PageUp")),
    (PhysicalKey::PgDn, Layer::Normal, Action::key("PageDown")),
    (PhysicalKey::Right, Layer::Normal, Action::key("Right")),
    (PhysicalKey::Mute, Layer::Normal, Action::label("Mute")),
    (PhysicalKey::PrevTr, Layer::Normal, Action::label("PrevTr")),
    (PhysicalKey::PlayPause, Layer::Normal, Action::label("PlayPause")),
    (PhysicalKey::NextTr, Layer::Normal, Action::label("NextTr")),
    (PhysicalKey::Logo, Layer::Normal, Action::label("TouchWWW")),
    (PhysicalKey::VolumD, Layer::Normal, Action::label("VolumD")),
    (PhysicalKey::VolumI, Layer::Normal, Action::label("VolumI")),
    (PhysicalKey::Esc, Layer::Fn, Action::key("Backtick")),
    (PhysicalKey::LCtrl, Layer::Fn, Action::label("SP_KB_CHANGE")),
    (PhysicalKey::Digit1, Layer::Fn, Action::key("F1")),
    (PhysicalKey::Q, Layer::Fn, Action::label("BT0")),
    (PhysicalKey::A, Layer::Fn, Action::label("Windows")),
    (PhysicalKey::LWin, Layer::Fn, Action::label("WinLock")),
    (PhysicalKey::Digit2, Layer::Fn, Action::key("F2")),
    (PhysicalKey::W, Layer::Fn, Action::label("BT1")),
    (PhysicalKey::S, Layer::Fn, Action::label("SP_Mac_Mode")),
    (PhysicalKey::Digit3, Layer::Fn, Action::key("F3")),
    (PhysicalKey::E, Layer::Fn, Action::label("BT2")),
    (PhysicalKey::Digit4, Layer::Fn, Action::key("F4")),
    (PhysicalKey::Digit5, Layer::Fn, Action::key("F5")),
    (PhysicalKey::T, Layer::Fn, Action::label("SP_Touch_Mode")),
    (PhysicalKey::G, Layer::Fn, Action::label("EMITest")),
    (PhysicalKey::SpaceL, Layer::Fn, Action::label("SP_KB_REC_Reset")),
    (PhysicalKey::Digit6, Layer::Fn, Action::key("F6")),
    (PhysicalKey::Digit7, Layer::Fn, Action::key("F7")),
    (PhysicalKey::SpaceR, Layer::Fn, Action::label("SP_KB_REC_Reset")),
    (PhysicalKey::Digit8, Layer::Fn, Action::key("F8")),
    (PhysicalKey::Digit9, Layer::Fn, Action::key("F9")),
    (PhysicalKey::O, Layer::Fn, Action::label("SP_O_Mode")),
    (PhysicalKey::L, Layer::Fn, Action::label("Mac")),
    (PhysicalKey::Comma, Layer::Fn, Action::label("LedColorModeLoop")),
    (PhysicalKey::Digit0, Layer::Fn, Action::key("F10")),
    (PhysicalKey::P, Layer::Fn, Action::label("2.4G")),
    (PhysicalKey::Period, Layer::Fn, Action::label("LogoModelLoop")),
    (PhysicalKey::Minus, Layer::Fn, Action::key("F11")),
    (PhysicalKey::BracketLeft, Layer::Fn, Action::key("PrintScreen")),
    (PhysicalKey::Quote, Layer::Fn, Action::key("Home")),
    (PhysicalKey::Equal, Layer::Fn, Action::key("F12")),
    (PhysicalKey::BracketRight, Layer::Fn, Action::key("Insert")),
    (PhysicalKey::Fn1, Layer::Fn, Action::label("Fn1")),
    (PhysicalKey::Backslash, Layer::Fn, Action::label("LedMode+")),
    (PhysicalKey::Enter, Layer::Fn, Action::label("Battery_Display")),
    (PhysicalKey::Left, Layer::Fn, Action::label("light.fun_4")),
    (PhysicalKey::Up, Layer::Fn, Action::label("light.fun_1")),
    (PhysicalKey::Down, Layer::Fn, Action::label("light.fun_2")),
    (PhysicalKey::Del, Layer::Fn, Action::key("ScrollLock")),
    (PhysicalKey::PgUp, Layer::Fn, Action::key("Pause")),
    (PhysicalKey::PgDn, Layer::Fn, Action::key("End")),
    (PhysicalKey::Right, Layer::Fn, Action::label("light.fun_3")),
    (PhysicalKey::Logo, Layer::Fn, Action::label("SP_Touch_Mode")),
];

/// The factory-default `{layer -> {slot -> raw value}}` maps, resolved from
/// [`FACTORY_DEFAULT`] through `codec`. Layers with no factory mappings ("fn2") are
/// present but empty, matching the old YAML-parsed behaviour.
pub fn factory_default_slot_maps(codec: &KeyMappingCodec) -> HashMap<u8, HashMap<u16, u32>> {
    let mut out: HashMap<u8, HashMap<u16, u32>> = HashMap::new();
    out.insert(Layer::Normal as u8, HashMap::new());
    out.insert(Layer::Fn as u8, HashMap::new());
    out.insert(Layer::Fn2 as u8, HashMap::new());
    for (key, layer, action) in FACTORY_DEFAULT {
        out.get_mut(&(*layer as u8))
            .expect("all three layers preseeded above")
            .insert(key.slot(), action.to_slot_value(codec, key));
    }
    out
}

/// A factory-default raw buffer for one layer, built from [`factory_default_slot_maps`].
/// Slots the default doesn't mention are left zeroed, matching how a freshly reset
/// device's unused slots read.
pub fn factory_default_buffer(codec: &KeyMappingCodec, layer: u8) -> Vec<u8> {
    let mut buffer = vec![0u8; KEYMATRIX_BUFFER_LEN];
    patch_buffer(&mut buffer, &factory_default_slot_maps(codec).remove(&layer).unwrap_or_default());
    buffer
}

/// Overwrite the slots named in `slot_map` (slot -> raw value) in a KeyMatrix `buffer`,
/// leaving every other slot untouched. The shared write primitive both the factory-default
/// baseline and any imported config are patched into a buffer through.
pub fn patch_buffer(buffer: &mut [u8], slot_map: &HashMap<u16, u32>) {
    for (&slot, &value) in slot_map {
        let offset = slot as usize * 4;
        buffer[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every action in the table resolves through the codec without panicking, and each
    /// resolves back to the label/key it names — the table is internally consistent.
    #[test]
    fn every_entry_resolves_and_round_trips_through_decode() {
        let codec = KeyMappingCodec::new();
        for (key, _layer, action) in FACTORY_DEFAULT {
            let raw = action.to_slot_value(&codec, key);
            let decoded = codec.decode(raw, None);
            match action {
                Action::Label(label) => assert_eq!(&decoded.label(), label, "{key:?}"),
                Action::Key { symbol, .. } if !symbol.is_empty() => {
                    assert!(decoded.label().ends_with(symbol), "{key:?}: {}", decoded.label());
                }
                _ => {}
            }
        }
    }

    /// Normal and Fn carry factory mappings; Fn2 is present but empty.
    #[test]
    fn fn2_has_no_factory_mappings() {
        let maps = factory_default_slot_maps(&KeyMappingCodec::new());
        assert!(!maps[&(Layer::Normal as u8)].is_empty());
        assert!(!maps[&(Layer::Fn as u8)].is_empty());
        assert!(maps[&(Layer::Fn2 as u8)].is_empty());
    }

    /// `factory_default_buffer` places each mapping at its slot's byte offset.
    #[test]
    fn buffer_places_esc_at_its_slot() {
        let codec = KeyMappingCodec::new();
        let buffer = factory_default_buffer(&codec, Layer::Normal as u8);
        let slot = PhysicalKey::Esc.slot() as usize;
        let value = u32::from_be_bytes(buffer[slot * 4..slot * 4 + 4].try_into().unwrap());
        assert_eq!(codec.decode(value, None).label(), "Esc");
    }
}
