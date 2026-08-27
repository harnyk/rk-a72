//! Keyboard models as data. A `KeyboardModel` is one physical device: its USB ids, its
//! set of named physical keys, and its factory-default KeyMatrix — all expressed as
//! plain data, resolved through the (protocol-level) [`KeyMappingCodec`].
//!
//! This is the aggregation point that keeps adding a device cheap. The *protocol* (byte
//! grammar, buffer sizes, raw<->meaning encoding in `protocol`/`codec`) is shared by
//! every device that speaks it; a model only carries what actually varies per device.
//! A second same-protocol keyboard (e.g. one with a numpad) is a new `const` in
//! [`MODELS`] naming more of the same fixed slot frame — nothing else moves. A device
//! that needed a *different* wire frame would by definition be a different protocol,
//! which is a separate axis not modelled here yet.
//!
//! Physical keys are addressed by name (`"Esc"`, `"M5"`), resolved against *this model's*
//! key set — so a factory entry or an HCL config naming a key the model doesn't have
//! fails against that model, not silently against some global key universe.

use std::collections::HashMap;

use crate::codec::KeyMappingCodec;
use crate::layout::PhysicalKeyboardLayout;
use crate::modifiers::ModifierSet;
use crate::protocol::KEYMATRIX_BUFFER_LEN;

/// Which of the A72-family's three layers a factory mapping belongs to. Shared across
/// models of this family (every one exposes Normal/Fn/Fn2); the slot maps this compiles
/// to are keyed by the same `u8` (0/1/2) the rest of the crate uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    Normal = 0,
    Fn = 1,
    Fn2 = 2,
}

/// A factory key action, in the three flavours the KeyMatrix write path understands.
/// Symbols, modifier names and labels are resolved to raw slot values through the codec
/// at load time, so the tables stay readable (`Action::key("Esc")`), never hex.
#[derive(Debug, Clone, Copy)]
pub enum Action {
    /// A KeyBoard key (optionally with modifiers), by its KeySymbol and modifier names.
    Key { symbol: &'static str, mods: &'static [&'static str] },
    /// Modifier(s) pressed on their own, with no key — e.g. a bare Shift.
    Mods(&'static [&'static str]),
    /// A non-KeyBoard action, by its label (Media/SpecialFun/...).
    Label(&'static str),
}

impl Action {
    pub const fn key(symbol: &'static str) -> Action {
        Action::Key { symbol, mods: &[] }
    }
    pub const fn key_mods(symbol: &'static str, mods: &'static [&'static str]) -> Action {
        Action::Key { symbol, mods }
    }
    pub const fn mods(mods: &'static [&'static str]) -> Action {
        Action::Mods(mods)
    }
    pub const fn label(label: &'static str) -> Action {
        Action::Label(label)
    }

    /// Resolve this action to its raw 4-byte KeyMatrix slot value. Panics on a symbol,
    /// modifier or label the codec doesn't know — a model's factory table is the crate's
    /// own constant, so an unknown token is a bug to catch in tests, not a user error.
    fn to_slot_value(self, codec: &KeyMappingCodec, key: &str) -> u32 {
        let resolve_mods = |names: &[&str]| {
            names.iter().fold(ModifierSet::empty(), |acc, name| {
                acc | ModifierSet::from_label(name)
                    .unwrap_or_else(|_| panic!("factory default {key:?}: unknown modifier {name:?}"))
            })
        };
        match self {
            Action::Key { symbol, mods } => {
                let key_code = if symbol.is_empty() {
                    0
                } else {
                    codec.symbol_to_keycode(symbol).unwrap_or_else(|| {
                        panic!("factory default {key:?}: unknown key symbol {symbol:?}")
                    })
                };
                KeyMappingCodec::encode_keyboard(key_code, resolve_mods(mods))
            }
            Action::Mods(mods) => KeyMappingCodec::encode_keyboard(0, resolve_mods(mods)),
            Action::Label(label) => {
                KeyMappingCodec::encode_raw(codec.label_to_raw(label).unwrap_or_else(|| {
                    panic!("factory default {key:?}: unknown label {label:?}")
                }))
            }
        }
    }
}

/// One physical keyboard: what varies per device, as data.
pub struct KeyboardModel {
    pub vid: u16,
    pub pid: u16,
    pub name: &'static str,
    /// Named physical keys: (KeyMatrix slot, canonical name). Every slot is within the
    /// protocol's fixed frame; unlisted slots are unnamed matrix cells with no keycap.
    keys: &'static [(u16, &'static str)],
    /// Factory-default mappings: (key name, layer, action). Un-listed (key, layer) pairs
    /// are unbound out of the box. Key names must appear in `keys`.
    factory: &'static [(&'static str, Layer, Action)],
}

/// Every known keyboard model. Adding a device = one `const` above and one entry here.
pub static MODELS: &[&KeyboardModel] = &[&RK_A72];

impl KeyboardModel {
    /// The model matching this USB vid/pid, if any is known.
    pub fn for_ids(vid: u16, pid: u16) -> Option<&'static KeyboardModel> {
        MODELS.iter().copied().find(|m| m.vid == vid && m.pid == pid)
    }

    /// The default model (the only one, today) — used where no device context selects one
    /// (shell completion, `list-keys`).
    pub fn default_model() -> &'static KeyboardModel {
        &RK_A72
    }

    /// The protocol codec for this model. Protocol-level and shared today, reached through
    /// the model so a future second protocol stays an additive change here, not a sweep.
    pub fn codec(&self) -> KeyMappingCodec {
        KeyMappingCodec::new()
    }

    /// The string-resolver view (names, `slotN` fallback, visual overrides) over this
    /// model's key set.
    pub fn layout(&self) -> PhysicalKeyboardLayout {
        PhysicalKeyboardLayout::for_model(self)
    }

    /// This model's named keys, as (slot, name) in source order.
    pub fn named_keys(&self) -> impl Iterator<Item = (u16, &'static str)> + '_ {
        self.keys.iter().copied()
    }

    /// The KeyMatrix slot for an exact key name in this model (no `slotN` fallback).
    fn slot_of(&self, name: &str) -> Option<u16> {
        self.keys.iter().find(|(_, n)| *n == name).map(|(s, _)| *s)
    }

    /// The factory-default `{layer -> {slot -> raw value}}` maps, resolved through `codec`.
    /// Layers with no factory mappings ("fn2") are present but empty.
    pub fn factory_slot_maps(&self, codec: &KeyMappingCodec) -> HashMap<u8, HashMap<u16, u32>> {
        let mut out: HashMap<u8, HashMap<u16, u32>> = HashMap::new();
        out.insert(Layer::Normal as u8, HashMap::new());
        out.insert(Layer::Fn as u8, HashMap::new());
        out.insert(Layer::Fn2 as u8, HashMap::new());
        for (key, layer, action) in self.factory {
            let slot = self.slot_of(key).unwrap_or_else(|| {
                panic!("model {:?}: factory default names key {key:?} not in its key set", self.name)
            });
            out.get_mut(&(*layer as u8))
                .expect("all three layers preseeded above")
                .insert(slot, action.to_slot_value(codec, key));
        }
        out
    }

    /// A factory-default raw buffer for one layer, built from [`Self::factory_slot_maps`].
    /// Slots the default doesn't mention are left zeroed, matching a freshly reset device.
    pub fn factory_buffer(&self, codec: &KeyMappingCodec, layer: u8) -> Vec<u8> {
        let mut buffer = vec![0u8; KEYMATRIX_BUFFER_LEN];
        patch_buffer(&mut buffer, &self.factory_slot_maps(codec).remove(&layer).unwrap_or_default());
        buffer
    }
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

// ---------------------------------------------------------------------------
// RK A72 (wired, 258a:0216) — the one device this crate was reverse-engineered
// against. Its 126-slot KeyMatrix is a fixed 6x21 scan matrix; 81 cells are named
// keys, the other 45 are empty matrix positions with no keycap.
// ---------------------------------------------------------------------------

pub const RK_A72: KeyboardModel = KeyboardModel {
    vid: crate::protocol::SUPPORTED_VENDOR_ID,
    pid: crate::protocol::SUPPORTED_PRODUCT_ID,
    name: "RK A72 (wired)",
    keys: A72_KEYS,
    factory: A72_FACTORY,
};

/// (KeyMatrix slot, canonical name) for every named key on the A72.
const A72_KEYS: &[(u16, &str)] = &[
    (1, "M5"),
    (2, "M4"),
    (3, "M3"),
    (4, "M2"),
    (5, "M1"),
    (7, "Esc"),
    (8, "Tab"),
    (9, "CapsLock"),
    (10, "LShift"),
    (11, "LCtrl"),
    (13, "Digit1"),
    (14, "Q"),
    (15, "A"),
    (16, "IntlBackslash"),
    (17, "LWin"),
    (19, "Digit2"),
    (20, "W"),
    (21, "S"),
    (22, "Z"),
    (23, "LAlt"),
    (25, "Digit3"),
    (26, "E"),
    (27, "D"),
    (28, "X"),
    (31, "Digit4"),
    (32, "R"),
    (33, "F"),
    (34, "C"),
    (37, "Digit5"),
    (38, "T"),
    (39, "G"),
    (40, "V"),
    (41, "SpaceL"),
    (43, "Digit6"),
    (44, "Y"),
    (45, "H"),
    (46, "B"),
    (49, "Digit7"),
    (50, "U"),
    (51, "J"),
    (52, "N"),
    (53, "SpaceR"),
    (55, "Digit8"),
    (56, "I"),
    (57, "K"),
    (58, "M"),
    (61, "Digit9"),
    (62, "O"),
    (63, "L"),
    (64, "Comma"),
    (67, "Digit0"),
    (68, "P"),
    (69, "Semicolon"),
    (70, "Period"),
    (71, "RAlt"),
    (73, "Minus"),
    (74, "BracketLeft"),
    (75, "Quote"),
    (76, "Slash"),
    (79, "Equal"),
    (80, "BracketRight"),
    (81, "Hash"),
    (83, "Fn1"),
    (85, "Backspace"),
    (86, "Backslash"),
    (87, "Enter"),
    (88, "RShift"),
    (89, "Left"),
    (94, "Up"),
    (95, "Down"),
    (97, "Del"),
    (98, "PgUp"),
    (99, "PgDn"),
    (101, "Right"),
    (104, "Mute"),
    (105, "PrevTr"),
    (106, "PlayPause"),
    (107, "NextTr"),
    (120, "Logo"),
    (123, "VolumD"),
    (125, "VolumI"),
];

/// The A72 factory default: (key name, layer, action), dumped from a freshly reset A72
/// (Fn+SpaceL held 5s) and verified against USB captures.
const A72_FACTORY: &[(&str, Layer, Action)] = &[
    ("M5", Layer::Normal, Action::key_mods("C", &["LCtrl"])),
    ("M4", Layer::Normal, Action::key_mods("V", &["LCtrl"])),
    ("M3", Layer::Normal, Action::key_mods("A", &["LCtrl"])),
    ("M2", Layer::Normal, Action::key_mods("X", &["LCtrl"])),
    ("M1", Layer::Normal, Action::key_mods("Z", &["LCtrl"])),
    ("Esc", Layer::Normal, Action::key("Esc")),
    ("Tab", Layer::Normal, Action::key("Tab")),
    ("CapsLock", Layer::Normal, Action::key("CapsLock")),
    ("LShift", Layer::Normal, Action::mods(&["LShift"])),
    ("LCtrl", Layer::Normal, Action::mods(&["LCtrl"])),
    ("Digit1", Layer::Normal, Action::key("1")),
    ("Q", Layer::Normal, Action::key("Q")),
    ("A", Layer::Normal, Action::key("A")),
    ("IntlBackslash", Layer::Normal, Action::key("IntlBackslash")),
    ("LWin", Layer::Normal, Action::mods(&["LWin"])),
    ("Digit2", Layer::Normal, Action::key("2")),
    ("W", Layer::Normal, Action::key("W")),
    ("S", Layer::Normal, Action::key("S")),
    ("Z", Layer::Normal, Action::key("Z")),
    ("LAlt", Layer::Normal, Action::mods(&["LAlt"])),
    ("Digit3", Layer::Normal, Action::key("3")),
    ("E", Layer::Normal, Action::key("E")),
    ("D", Layer::Normal, Action::key("D")),
    ("X", Layer::Normal, Action::key("X")),
    ("Digit4", Layer::Normal, Action::key("4")),
    ("R", Layer::Normal, Action::key("R")),
    ("F", Layer::Normal, Action::key("F")),
    ("C", Layer::Normal, Action::key("C")),
    ("Digit5", Layer::Normal, Action::key("5")),
    ("T", Layer::Normal, Action::key("T")),
    ("G", Layer::Normal, Action::key("G")),
    ("V", Layer::Normal, Action::key("V")),
    ("SpaceL", Layer::Normal, Action::key("Space")),
    ("Digit6", Layer::Normal, Action::key("6")),
    ("Y", Layer::Normal, Action::key("Y")),
    ("H", Layer::Normal, Action::key("H")),
    ("B", Layer::Normal, Action::key("B")),
    ("Digit7", Layer::Normal, Action::key("7")),
    ("U", Layer::Normal, Action::key("U")),
    ("J", Layer::Normal, Action::key("J")),
    ("N", Layer::Normal, Action::key("N")),
    ("SpaceR", Layer::Normal, Action::key("Space")),
    ("Digit8", Layer::Normal, Action::key("8")),
    ("I", Layer::Normal, Action::key("I")),
    ("K", Layer::Normal, Action::key("K")),
    ("M", Layer::Normal, Action::key("M")),
    ("Digit9", Layer::Normal, Action::key("9")),
    ("O", Layer::Normal, Action::key("O")),
    ("L", Layer::Normal, Action::key("L")),
    ("Comma", Layer::Normal, Action::key(",")),
    ("Digit0", Layer::Normal, Action::key("0")),
    ("P", Layer::Normal, Action::key("P")),
    ("Semicolon", Layer::Normal, Action::key("Semicolon")),
    ("Period", Layer::Normal, Action::key(".")),
    ("RAlt", Layer::Normal, Action::mods(&["RAlt"])),
    ("Minus", Layer::Normal, Action::key("Minus")),
    ("BracketLeft", Layer::Normal, Action::key("[")),
    ("Quote", Layer::Normal, Action::key("Quote")),
    ("Slash", Layer::Normal, Action::key("/")),
    ("Equal", Layer::Normal, Action::key("=")),
    ("BracketRight", Layer::Normal, Action::key("]")),
    ("Hash", Layer::Normal, Action::key("Hash")),
    ("Fn1", Layer::Normal, Action::label("Fn1")),
    ("Backspace", Layer::Normal, Action::key("Backspace")),
    ("Backslash", Layer::Normal, Action::key("Backslash")),
    ("Enter", Layer::Normal, Action::key("Enter")),
    ("RShift", Layer::Normal, Action::mods(&["RShift"])),
    ("Left", Layer::Normal, Action::key("Left")),
    ("Up", Layer::Normal, Action::key("Up")),
    ("Down", Layer::Normal, Action::key("Down")),
    ("Del", Layer::Normal, Action::key("Delete")),
    ("PgUp", Layer::Normal, Action::key("PageUp")),
    ("PgDn", Layer::Normal, Action::key("PageDown")),
    ("Right", Layer::Normal, Action::key("Right")),
    ("Mute", Layer::Normal, Action::label("Mute")),
    ("PrevTr", Layer::Normal, Action::label("PrevTr")),
    ("PlayPause", Layer::Normal, Action::label("PlayPause")),
    ("NextTr", Layer::Normal, Action::label("NextTr")),
    ("Logo", Layer::Normal, Action::label("TouchWWW")),
    ("VolumD", Layer::Normal, Action::label("VolumD")),
    ("VolumI", Layer::Normal, Action::label("VolumI")),
    ("Esc", Layer::Fn, Action::key("Backtick")),
    ("LCtrl", Layer::Fn, Action::label("SP_KB_CHANGE")),
    ("Digit1", Layer::Fn, Action::key("F1")),
    ("Q", Layer::Fn, Action::label("BT0")),
    ("A", Layer::Fn, Action::label("Windows")),
    ("LWin", Layer::Fn, Action::label("WinLock")),
    ("Digit2", Layer::Fn, Action::key("F2")),
    ("W", Layer::Fn, Action::label("BT1")),
    ("S", Layer::Fn, Action::label("SP_Mac_Mode")),
    ("Digit3", Layer::Fn, Action::key("F3")),
    ("E", Layer::Fn, Action::label("BT2")),
    ("Digit4", Layer::Fn, Action::key("F4")),
    ("Digit5", Layer::Fn, Action::key("F5")),
    ("T", Layer::Fn, Action::label("SP_Touch_Mode")),
    ("G", Layer::Fn, Action::label("EMITest")),
    ("SpaceL", Layer::Fn, Action::label("SP_KB_REC_Reset")),
    ("Digit6", Layer::Fn, Action::key("F6")),
    ("Digit7", Layer::Fn, Action::key("F7")),
    ("SpaceR", Layer::Fn, Action::label("SP_KB_REC_Reset")),
    ("Digit8", Layer::Fn, Action::key("F8")),
    ("Digit9", Layer::Fn, Action::key("F9")),
    ("O", Layer::Fn, Action::label("SP_O_Mode")),
    ("L", Layer::Fn, Action::label("Mac")),
    ("Comma", Layer::Fn, Action::label("LedColorModeLoop")),
    ("Digit0", Layer::Fn, Action::key("F10")),
    ("P", Layer::Fn, Action::label("2.4G")),
    ("Period", Layer::Fn, Action::label("LogoModelLoop")),
    ("Minus", Layer::Fn, Action::key("F11")),
    ("BracketLeft", Layer::Fn, Action::key("PrintScreen")),
    ("Quote", Layer::Fn, Action::key("Home")),
    ("Equal", Layer::Fn, Action::key("F12")),
    ("BracketRight", Layer::Fn, Action::key("Insert")),
    ("Fn1", Layer::Fn, Action::label("Fn1")),
    ("Backslash", Layer::Fn, Action::label("LedMode+")),
    ("Enter", Layer::Fn, Action::label("Battery_Display")),
    ("Left", Layer::Fn, Action::label("light.fun_4")),
    ("Up", Layer::Fn, Action::label("light.fun_1")),
    ("Down", Layer::Fn, Action::label("light.fun_2")),
    ("Del", Layer::Fn, Action::key("ScrollLock")),
    ("PgUp", Layer::Fn, Action::key("Pause")),
    ("PgDn", Layer::Fn, Action::key("End")),
    ("Right", Layer::Fn, Action::label("light.fun_3")),
    ("Logo", Layer::Fn, Action::label("SP_Touch_Mode")),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_slots_are_unique_and_in_frame() {
        let mut seen = std::collections::HashSet::new();
        for (slot, name) in RK_A72.named_keys() {
            assert!(
                (slot as usize) < crate::protocol::KEYMATRIX_SLOT_COUNT,
                "{name} slot {slot} is outside the protocol frame"
            );
            assert!(seen.insert(slot), "duplicate slot {slot} ({name})");
        }
    }

    #[test]
    fn key_names_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for (_, name) in RK_A72.named_keys() {
            assert!(seen.insert(name), "duplicate key name {name}");
        }
    }

    #[test]
    fn for_ids_selects_a72_and_rejects_unknown() {
        let m = KeyboardModel::for_ids(0x258a, 0x0216).expect("A72 must resolve");
        assert_eq!(m.name, "RK A72 (wired)");
        assert!(KeyboardModel::for_ids(0x1234, 0x5678).is_none());
    }

    #[test]
    fn every_factory_entry_resolves_and_round_trips_through_decode() {
        let codec = KeyMappingCodec::new();
        for (key, _layer, action) in RK_A72.factory {
            let raw = action.to_slot_value(&codec, key);
            let decoded = codec.decode(raw, None);
            match action {
                Action::Label(label) => assert_eq!(&decoded.label(), label, "{key}"),
                Action::Key { symbol, .. } if !symbol.is_empty() => {
                    assert!(decoded.label().ends_with(symbol), "{key}: {}", decoded.label());
                }
                _ => {}
            }
        }
    }

    #[test]
    fn fn2_has_no_factory_mappings() {
        let maps = RK_A72.factory_slot_maps(&KeyMappingCodec::new());
        assert!(!maps[&(Layer::Normal as u8)].is_empty());
        assert!(!maps[&(Layer::Fn as u8)].is_empty());
        assert!(maps[&(Layer::Fn2 as u8)].is_empty());
    }

    #[test]
    fn factory_buffer_places_esc_at_its_slot() {
        let codec = KeyMappingCodec::new();
        let buffer = RK_A72.factory_buffer(&codec, Layer::Normal as u8);
        let slot = RK_A72.slot_of("Esc").expect("Esc is a key") as usize;
        let value = u32::from_be_bytes(buffer[slot * 4..slot * 4 + 4].try_into().unwrap());
        assert_eq!(codec.decode(value, None).label(), "Esc");
    }
}
