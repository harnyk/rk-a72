//! The set of physical keys on the RK A72, as a compile-time enum — the single source
//! of truth for which KeyMatrix slot each named key occupies and what it's called.
//!
//! Each variant's discriminant IS its KeyMatrix slot (0..{KEYMATRIX_SLOT_COUNT}), so
//! [`PhysicalKey::slot`] is just the discriminant. Not every one of the 126 slots is a
//! named key; unnamed slots have no variant here and are addressed by their raw slot
//! number (the `slotN` string form) through [`crate::layout::PhysicalKeyboardLayout`].

/// A named physical key. The discriminant is the key's KeyMatrix slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum PhysicalKey {
    M5 = 1,
    M4 = 2,
    M3 = 3,
    M2 = 4,
    M1 = 5,
    Esc = 7,
    Tab = 8,
    CapsLock = 9,
    LShift = 10,
    LCtrl = 11,
    Digit1 = 13,
    Q = 14,
    A = 15,
    IntlBackslash = 16,
    LWin = 17,
    Digit2 = 19,
    W = 20,
    S = 21,
    Z = 22,
    LAlt = 23,
    Digit3 = 25,
    E = 26,
    D = 27,
    X = 28,
    Digit4 = 31,
    R = 32,
    F = 33,
    C = 34,
    Digit5 = 37,
    T = 38,
    G = 39,
    V = 40,
    SpaceL = 41,
    Digit6 = 43,
    Y = 44,
    H = 45,
    B = 46,
    Digit7 = 49,
    U = 50,
    J = 51,
    N = 52,
    SpaceR = 53,
    Digit8 = 55,
    I = 56,
    K = 57,
    M = 58,
    Digit9 = 61,
    O = 62,
    L = 63,
    Comma = 64,
    Digit0 = 67,
    P = 68,
    Semicolon = 69,
    Period = 70,
    RAlt = 71,
    Minus = 73,
    BracketLeft = 74,
    Quote = 75,
    Slash = 76,
    Equal = 79,
    BracketRight = 80,
    Hash = 81,
    Fn1 = 83,
    Backspace = 85,
    Backslash = 86,
    Enter = 87,
    RShift = 88,
    Left = 89,
    Up = 94,
    Down = 95,
    Del = 97,
    PgUp = 98,
    PgDn = 99,
    Right = 101,
    Mute = 104,
    PrevTr = 105,
    PlayPause = 106,
    NextTr = 107,
    Logo = 120,
    VolumD = 123,
    VolumI = 125,
}

impl PhysicalKey {
    /// Every named physical key, in ascending slot order.
    pub const ALL: [PhysicalKey; 81] = [
        PhysicalKey::M5,
        PhysicalKey::M4,
        PhysicalKey::M3,
        PhysicalKey::M2,
        PhysicalKey::M1,
        PhysicalKey::Esc,
        PhysicalKey::Tab,
        PhysicalKey::CapsLock,
        PhysicalKey::LShift,
        PhysicalKey::LCtrl,
        PhysicalKey::Digit1,
        PhysicalKey::Q,
        PhysicalKey::A,
        PhysicalKey::IntlBackslash,
        PhysicalKey::LWin,
        PhysicalKey::Digit2,
        PhysicalKey::W,
        PhysicalKey::S,
        PhysicalKey::Z,
        PhysicalKey::LAlt,
        PhysicalKey::Digit3,
        PhysicalKey::E,
        PhysicalKey::D,
        PhysicalKey::X,
        PhysicalKey::Digit4,
        PhysicalKey::R,
        PhysicalKey::F,
        PhysicalKey::C,
        PhysicalKey::Digit5,
        PhysicalKey::T,
        PhysicalKey::G,
        PhysicalKey::V,
        PhysicalKey::SpaceL,
        PhysicalKey::Digit6,
        PhysicalKey::Y,
        PhysicalKey::H,
        PhysicalKey::B,
        PhysicalKey::Digit7,
        PhysicalKey::U,
        PhysicalKey::J,
        PhysicalKey::N,
        PhysicalKey::SpaceR,
        PhysicalKey::Digit8,
        PhysicalKey::I,
        PhysicalKey::K,
        PhysicalKey::M,
        PhysicalKey::Digit9,
        PhysicalKey::O,
        PhysicalKey::L,
        PhysicalKey::Comma,
        PhysicalKey::Digit0,
        PhysicalKey::P,
        PhysicalKey::Semicolon,
        PhysicalKey::Period,
        PhysicalKey::RAlt,
        PhysicalKey::Minus,
        PhysicalKey::BracketLeft,
        PhysicalKey::Quote,
        PhysicalKey::Slash,
        PhysicalKey::Equal,
        PhysicalKey::BracketRight,
        PhysicalKey::Hash,
        PhysicalKey::Fn1,
        PhysicalKey::Backspace,
        PhysicalKey::Backslash,
        PhysicalKey::Enter,
        PhysicalKey::RShift,
        PhysicalKey::Left,
        PhysicalKey::Up,
        PhysicalKey::Down,
        PhysicalKey::Del,
        PhysicalKey::PgUp,
        PhysicalKey::PgDn,
        PhysicalKey::Right,
        PhysicalKey::Mute,
        PhysicalKey::PrevTr,
        PhysicalKey::PlayPause,
        PhysicalKey::NextTr,
        PhysicalKey::Logo,
        PhysicalKey::VolumD,
        PhysicalKey::VolumI,
    ];

    /// This key's KeyMatrix slot (the enum discriminant).
    pub const fn slot(self) -> u16 {
        self as u16
    }

    /// This key's canonical name, as used in HCL configs and `list-keys`.
    pub const fn name(self) -> &'static str {
        match self {
            PhysicalKey::M5 => "M5",
            PhysicalKey::M4 => "M4",
            PhysicalKey::M3 => "M3",
            PhysicalKey::M2 => "M2",
            PhysicalKey::M1 => "M1",
            PhysicalKey::Esc => "Esc",
            PhysicalKey::Tab => "Tab",
            PhysicalKey::CapsLock => "CapsLock",
            PhysicalKey::LShift => "LShift",
            PhysicalKey::LCtrl => "LCtrl",
            PhysicalKey::Digit1 => "Digit1",
            PhysicalKey::Q => "Q",
            PhysicalKey::A => "A",
            PhysicalKey::IntlBackslash => "IntlBackslash",
            PhysicalKey::LWin => "LWin",
            PhysicalKey::Digit2 => "Digit2",
            PhysicalKey::W => "W",
            PhysicalKey::S => "S",
            PhysicalKey::Z => "Z",
            PhysicalKey::LAlt => "LAlt",
            PhysicalKey::Digit3 => "Digit3",
            PhysicalKey::E => "E",
            PhysicalKey::D => "D",
            PhysicalKey::X => "X",
            PhysicalKey::Digit4 => "Digit4",
            PhysicalKey::R => "R",
            PhysicalKey::F => "F",
            PhysicalKey::C => "C",
            PhysicalKey::Digit5 => "Digit5",
            PhysicalKey::T => "T",
            PhysicalKey::G => "G",
            PhysicalKey::V => "V",
            PhysicalKey::SpaceL => "SpaceL",
            PhysicalKey::Digit6 => "Digit6",
            PhysicalKey::Y => "Y",
            PhysicalKey::H => "H",
            PhysicalKey::B => "B",
            PhysicalKey::Digit7 => "Digit7",
            PhysicalKey::U => "U",
            PhysicalKey::J => "J",
            PhysicalKey::N => "N",
            PhysicalKey::SpaceR => "SpaceR",
            PhysicalKey::Digit8 => "Digit8",
            PhysicalKey::I => "I",
            PhysicalKey::K => "K",
            PhysicalKey::M => "M",
            PhysicalKey::Digit9 => "Digit9",
            PhysicalKey::O => "O",
            PhysicalKey::L => "L",
            PhysicalKey::Comma => "Comma",
            PhysicalKey::Digit0 => "Digit0",
            PhysicalKey::P => "P",
            PhysicalKey::Semicolon => "Semicolon",
            PhysicalKey::Period => "Period",
            PhysicalKey::RAlt => "RAlt",
            PhysicalKey::Minus => "Minus",
            PhysicalKey::BracketLeft => "BracketLeft",
            PhysicalKey::Quote => "Quote",
            PhysicalKey::Slash => "Slash",
            PhysicalKey::Equal => "Equal",
            PhysicalKey::BracketRight => "BracketRight",
            PhysicalKey::Hash => "Hash",
            PhysicalKey::Fn1 => "Fn1",
            PhysicalKey::Backspace => "Backspace",
            PhysicalKey::Backslash => "Backslash",
            PhysicalKey::Enter => "Enter",
            PhysicalKey::RShift => "RShift",
            PhysicalKey::Left => "Left",
            PhysicalKey::Up => "Up",
            PhysicalKey::Down => "Down",
            PhysicalKey::Del => "Del",
            PhysicalKey::PgUp => "PgUp",
            PhysicalKey::PgDn => "PgDn",
            PhysicalKey::Right => "Right",
            PhysicalKey::Mute => "Mute",
            PhysicalKey::PrevTr => "PrevTr",
            PhysicalKey::PlayPause => "PlayPause",
            PhysicalKey::NextTr => "NextTr",
            PhysicalKey::Logo => "Logo",
            PhysicalKey::VolumD => "VolumD",
            PhysicalKey::VolumI => "VolumI",
        }
    }

    /// The named key at `slot`, if any. Unnamed slots return `None`.
    pub fn from_slot(slot: u16) -> Option<PhysicalKey> {
        PhysicalKey::ALL.into_iter().find(|k| k.slot() == slot)
    }

    /// The key with this canonical name, if any.
    pub fn from_name(name: &str) -> Option<PhysicalKey> {
        PhysicalKey::ALL.into_iter().find(|k| k.name() == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_are_unique_and_in_range() {
        let mut seen = std::collections::HashSet::new();
        for key in PhysicalKey::ALL {
            assert!(
                (key.slot() as usize) < crate::protocol::KEYMATRIX_SLOT_COUNT,
                "{key:?} slot {} is out of range",
                key.slot()
            );
            assert!(seen.insert(key.slot()), "duplicate slot {} for {key:?}", key.slot());
        }
    }

    #[test]
    fn names_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for key in PhysicalKey::ALL {
            assert!(seen.insert(key.name()), "duplicate name {}", key.name());
        }
    }

    #[test]
    fn from_slot_and_from_name_are_inverses_of_slot_and_name() {
        for key in PhysicalKey::ALL {
            assert_eq!(PhysicalKey::from_slot(key.slot()), Some(key));
            assert_eq!(PhysicalKey::from_name(key.name()), Some(key));
        }
        assert_eq!(PhysicalKey::from_slot(0), None);
        assert_eq!(PhysicalKey::from_name("NotAKey"), None);
    }
}
